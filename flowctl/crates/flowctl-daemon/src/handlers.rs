//! HTTP API route handlers for the daemon.
//!
//! Provides REST endpoints for status, epics, tasks, and a WebSocket
//! endpoint for streaming live events to connected clients.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use flowctl_core::id::{is_task_id, slugify};
use flowctl_core::state_machine::{Status, Transition};
use flowctl_core::types::{FLOW_DIR, CONFIG_FILE};
use flowctl_scheduler::TimestampedEvent;
use flowctl_service::lifecycle::{BlockTaskRequest, DoneTaskRequest, RestartTaskRequest, StartTaskRequest};
use flowctl_service::ServiceError;

use crate::lifecycle::DaemonRuntime;

/// Application-level error type with proper HTTP status mapping.
#[derive(Debug)]
pub enum AppError {
    /// Database error (query failed, constraint violation, etc.)
    Db(String),
    /// Invalid state transition (e.g., done → in_progress)
    InvalidTransition(String),
    /// Invalid input (bad ID format, missing fields, etc.)
    InvalidInput(String),
    /// Internal error (serialization failure, lock poisoned, etc.)
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            AppError::Db(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::InvalidTransition(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl From<flowctl_db::DbError> for AppError {
    fn from(e: flowctl_db::DbError) -> Self {
        AppError::Db(e.to_string())
    }
}

/// Shared application state for all handlers.
pub type AppState = Arc<DaemonState>;

/// Combined daemon state: runtime + event bus + shared DB connection.
pub struct DaemonState {
    pub runtime: DaemonRuntime,
    pub event_bus: flowctl_scheduler::EventBus,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl DaemonState {
    /// Acquire DB lock, returning AppError instead of panicking.
    fn db_lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, AppError> {
        self.db
            .lock()
            .map_err(|_| AppError::Internal("DB lock poisoned".to_string()))
    }
}

/// GET /api/v1/health -- simple liveness check.
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// GET /api/v1/metrics -- daemon health metrics.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = state.runtime.health();
    (StatusCode::OK, Json(metrics))
}

/// POST /api/v1/shutdown -- initiate graceful shutdown.
pub async fn shutdown_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.runtime.initiate_shutdown();
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "shutting_down"})),
    )
}

/// GET /api/v1/status -- combined daemon status overview.
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let health = state.runtime.health();
    let subscribers = state.event_bus.subscriber_count();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "running",
            "uptime_secs": health.uptime_secs,
            "pid": health.pid,
            "memory_bytes": health.memory_bytes,
            "wal_size_bytes": health.wal_size_bytes,
            "event_subscribers": subscribers,
        })),
    )
}

/// GET /api/v1/epics -- list epics from the database.
pub async fn epics_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::EpicRepo::new(&conn);
    let epics = repo.list(None)?;
    let value = serde_json::to_value(&epics)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    Ok(Json(value))
}

/// GET /api/v1/tasks -- list tasks, optionally filtered by epic_id query param.
pub async fn tasks_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TasksQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);
    let result = if let Some(ref epic_id) = params.epic_id {
        repo.list_by_epic(epic_id)
    } else {
        repo.list_all(None, None)
    };
    let tasks = result?;
    let value = serde_json::to_value(&tasks)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    Ok(Json(value))
}

/// Query parameters for the tasks endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct TasksQuery {
    pub epic_id: Option<String>,
}

// ── Write endpoints ─────────────────────────────────────────────

/// POST /api/v1/tasks/create -- create a new task.
pub async fn create_task_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Validate task ID format.
    if !is_task_id(&body.id) {
        return Err(AppError::InvalidInput(format!(
            "invalid task ID format: '{}'. Expected format: epic-id.N",
            body.id
        )));
    }

    let conn = state.db_lock()?;
    let task = flowctl_core::types::Task {
        schema_version: 1,
        id: body.id.clone(),
        epic: body.epic_id.clone(),
        title: body.title.clone(),
        status: Status::Todo,
        priority: None,
        domain: flowctl_core::types::Domain::General,
        depends_on: body.depends_on.unwrap_or_default(),
        files: vec![],
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("tasks/{}.md", body.id)),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let repo = flowctl_db::TaskRepo::new(&conn);
    repo.upsert_with_body(&task, &body.body.unwrap_or_default())?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "id": body.id})),
    ))
}

/// POST /api/v1/tasks/start -- start a task via service layer.
pub async fn start_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let db = state.db.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| ServiceError::ValidationError("DB lock poisoned".to_string()))?;
        let flow_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(FLOW_DIR);
        let req = StartTaskRequest {
            task_id,
            force: false,
            actor: "daemon".to_string(),
        };
        flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, req)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking failed: {e}")))?;

    match result {
        Ok(resp) => Ok(Json(
            serde_json::json!({"success": true, "id": resp.task_id}),
        )),
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/done -- complete a task via service layer.
pub async fn done_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskDoneRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let summary = body.summary.clone();
    let db = state.db.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| ServiceError::ValidationError("DB lock poisoned".to_string()))?;
        let flow_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(FLOW_DIR);
        let req = DoneTaskRequest {
            task_id,
            summary,
            summary_file: None,
            evidence_json: None,
            evidence_inline: None,
            force: true,
            actor: "daemon".to_string(),
        };
        flowctl_service::lifecycle::done_task(Some(&conn), &flow_dir, req)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking failed: {e}")))?;

    match result {
        Ok(resp) => Ok(Json(
            serde_json::json!({"success": true, "id": resp.task_id}),
        )),
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateTaskRequest {
    pub id: String,
    pub epic_id: String,
    pub title: String,
    pub depends_on: Option<Vec<String>>,
    pub body: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskIdRequest {
    pub task_id: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskDoneRequest {
    pub task_id: String,
    pub summary: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskReasonRequest {
    pub task_id: String,
    pub reason: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateEpicRequest {
    pub title: String,
}

/// Query parameters for the memory endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct MemoryQuery {
    pub track: Option<String>,
    pub module: Option<String>,
}

/// Query parameters for the tokens endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct TokensQuery {
    pub epic_id: Option<String>,
    pub task_id: Option<String>,
}

/// GET /api/v1/tokens -- token usage, filtered by epic_id or task_id.
pub async fn tokens_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TokensQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let log = flowctl_db::EventLog::new(&conn);

    if let Some(ref task_id) = params.task_id {
        let rows = log.tokens_by_task(task_id)?;
        let value = serde_json::to_value(&rows)
            .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
        Ok(Json(value))
    } else if let Some(ref epic_id) = params.epic_id {
        let summaries = log.tokens_by_epic(epic_id)?;
        let value = serde_json::to_value(&summaries)
            .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
        Ok(Json(value))
    } else {
        Err(AppError::InvalidInput(
            "either epic_id or task_id query parameter is required".to_string(),
        ))
    }
}

/// Map service-layer errors to HTTP-appropriate AppErrors.
fn service_error_to_app_error(e: ServiceError) -> AppError {
    match e {
        ServiceError::TaskNotFound(msg) => AppError::InvalidInput(msg),
        ServiceError::EpicNotFound(msg) => AppError::InvalidInput(msg),
        ServiceError::InvalidTransition(msg) => AppError::InvalidTransition(msg),
        ServiceError::DependencyUnsatisfied { task, dependency } => {
            AppError::InvalidInput(format!("task {task} blocked by {dependency}"))
        }
        ServiceError::CrossActorViolation(msg) => AppError::InvalidInput(msg),
        ServiceError::ValidationError(msg) => AppError::InvalidInput(msg),
        ServiceError::DbError(e) => AppError::Db(e.to_string()),
        ServiceError::IoError(e) => AppError::Internal(e.to_string()),
        ServiceError::CoreError(e) => AppError::Internal(e.to_string()),
    }
}

/// Query parameters for the DAG endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct DagQuery {
    pub epic_id: String,
}

/// A node in the DAG visualization.
#[derive(Debug, serde::Serialize)]
pub struct DagNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub domain: String,
    pub x: f64,
    pub y: f64,
}

/// An edge in the DAG visualization.
#[derive(Debug, serde::Serialize)]
pub struct DagEdge {
    pub from: String,
    pub to: String,
}

/// Response for the DAG endpoint.
#[derive(Debug, serde::Serialize)]
pub struct DagResponse {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
}

/// GET /api/v1/dag?epic_id=X -- returns DAG layout for visualization.
///
/// Builds the task dependency graph using petgraph, then computes a simplified
/// Sugiyama layout (layer assignment via longest-path + node positioning) server-side.
pub async fn dag_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<DagQuery>,
) -> Result<Json<DagResponse>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);
    let tasks = repo.list_by_epic(&params.epic_id)?;

    if tasks.is_empty() {
        return Ok(Json(DagResponse {
            nodes: vec![],
            edges: vec![],
        }));
    }

    // Build DAG from tasks.
    let dag = flowctl_core::TaskDag::from_tasks(&tasks)
        .map_err(|e| AppError::Internal(format!("DAG build error: {e}")))?;

    // Map task IDs to their tasks for title/status lookup.
    let task_map: std::collections::HashMap<&str, &flowctl_core::types::Task> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    // Build node index → task ID mapping from topo order.
    let task_ids = dag.task_ids();

    // Compute layers via longest-path from roots.
    let mut layer: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for id in &task_ids {
        let deps = dag.dependencies(id);
        if deps.is_empty() {
            layer.insert(id.clone(), 0);
        } else {
            let max_dep_layer = deps
                .iter()
                .map(|d| layer.get(d.as_str()).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            layer.insert(id.clone(), max_dep_layer + 1);
        }
    }

    // Group nodes by layer for horizontal positioning.
    let max_layer = layer.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![vec![]; max_layer + 1];
    for (id, &l) in &layer {
        layers[l].push(id.clone());
    }
    // Sort within each layer for determinism.
    for l in &mut layers {
        l.sort();
    }

    // Compute x,y positions: layers go left-to-right, nodes within a layer are stacked vertically.
    let node_spacing_x = 200.0;
    let node_spacing_y = 100.0;

    let mut nodes = Vec::with_capacity(tasks.len());
    for (layer_idx, layer_nodes) in layers.iter().enumerate() {
        let layer_height = layer_nodes.len() as f64 * node_spacing_y;
        let y_offset = -layer_height / 2.0 + node_spacing_y / 2.0;
        for (pos, id) in layer_nodes.iter().enumerate() {
            let task = task_map.get(id.as_str());
            nodes.push(DagNode {
                id: id.clone(),
                title: task.map(|t| t.title.clone()).unwrap_or_default(),
                status: task
                    .map(|t| format!("{:?}", t.status).to_lowercase())
                    .unwrap_or_else(|| "todo".to_string()),
                domain: task
                    .map(|t| t.domain.to_string())
                    .unwrap_or_else(|| "general".to_string()),
                x: layer_idx as f64 * node_spacing_x,
                y: y_offset + pos as f64 * node_spacing_y,
            });
        }
    }

    // Build edges from dependency relationships.
    let mut edges = Vec::new();
    for id in &task_ids {
        for dep in dag.dependencies(id) {
            edges.push(DagEdge {
                from: dep,
                to: id.clone(),
            });
        }
    }

    Ok(Json(DagResponse { nodes, edges }))
}

// ── DAG mutation endpoint ───────────────────────────────────────

/// Request body for POST /api/v1/dag/mutate.
#[derive(Debug, serde::Deserialize)]
pub struct DagMutateRequest {
    pub action: String,
    pub params: serde_json::Value,
    /// Optimistic lock: client sends the `updated_at` timestamp it last saw.
    pub version: String,
}

/// POST /api/v1/dag/mutate -- apply a DAG mutation with optimistic locking.
///
/// Supported actions:
/// - `add_dep`: params `{task_id, depends_on}`
/// - `remove_dep`: params `{task_id, depends_on}`
/// - `retry_task`: params `{task_id}`
/// - `skip_task`: params `{task_id}`
///
/// Returns 409 on version conflict, broadcasts refresh event on success.
pub async fn dag_mutate_handler(
    State(state): State<AppState>,
    Json(body): Json<DagMutateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);

    match body.action.as_str() {
        "add_dep" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;
            let depends_on = body.params.get("depends_on")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.depends_on".into()))?;

            let task = repo.get(task_id)
                .map_err(|_| AppError::InvalidInput(format!("task not found: {task_id}")))?;
            let _dep = repo.get(depends_on)
                .map_err(|_| AppError::InvalidInput(format!("dependency task not found: {depends_on}")))?;

            check_version(&task, &body.version)?;

            // Cycle check: build hypothetical task list with new dep.
            let epic_tasks = repo.list_by_epic(&task.epic)?;
            let test_tasks: Vec<flowctl_core::types::Task> = epic_tasks.into_iter().map(|mut t| {
                if t.id == task_id && !t.depends_on.contains(&depends_on.to_string()) {
                    t.depends_on.push(depends_on.to_string());
                }
                t
            }).collect();

            if let Err(e) = flowctl_core::TaskDag::from_tasks(&test_tasks) {
                return Err(AppError::InvalidInput(format!("would create cycle: {e}")));
            }

            conn.execute(
                "INSERT OR IGNORE INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                rusqlite::params![task_id, depends_on],
            ).map_err(|e| AppError::Db(e.to_string()))?;
            touch_updated_at(&conn, task_id)?;

            state.event_bus.emit(flowctl_scheduler::FlowEvent::TaskReady {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "add_dep"})))
        }

        "remove_dep" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;
            let depends_on = body.params.get("depends_on")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.depends_on".into()))?;

            let task = repo.get(task_id)
                .map_err(|_| AppError::InvalidInput(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            conn.execute(
                "DELETE FROM task_deps WHERE task_id = ?1 AND depends_on = ?2",
                rusqlite::params![task_id, depends_on],
            ).map_err(|e| AppError::Db(e.to_string()))?;
            touch_updated_at(&conn, task_id)?;

            state.event_bus.emit(flowctl_scheduler::FlowEvent::TaskReady {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "remove_dep"})))
        }

        "retry_task" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;

            let task = repo.get(task_id)
                .map_err(|_| AppError::InvalidInput(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            Transition::new(task.status, Status::Todo).map_err(|e| {
                AppError::InvalidTransition(format!("cannot retry task '{}': {}", task_id, e))
            })?;

            repo.update_status(task_id, Status::Todo)?;

            state.event_bus.emit(flowctl_scheduler::FlowEvent::TaskReady {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "retry_task"})))
        }

        "skip_task" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;

            let task = repo.get(task_id)
                .map_err(|_| AppError::InvalidInput(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            Transition::new(task.status, Status::Skipped).map_err(|e| {
                AppError::InvalidTransition(format!("cannot skip task '{}': {}", task_id, e))
            })?;

            repo.update_status(task_id, Status::Skipped)?;

            state.event_bus.emit(flowctl_scheduler::FlowEvent::TaskReady {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "skip_task"})))
        }

        other => Err(AppError::InvalidInput(format!("unknown action: {other}"))),
    }
}

/// Check optimistic lock version against the task's `updated_at`.
fn check_version(task: &flowctl_core::types::Task, version: &str) -> Result<(), AppError> {
    let task_version = task.updated_at.to_rfc3339();
    if task_version != version {
        return Err(AppError::InvalidTransition(format!(
            "version conflict: expected {version}, got {task_version}"
        )));
    }
    Ok(())
}

/// Update the `updated_at` timestamp for a task.
fn touch_updated_at(conn: &rusqlite::Connection, task_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![chrono::Utc::now().to_rfc3339(), task_id],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}

// ── New API endpoints ──────────────────────────────────────────

/// POST /api/v1/epics/create -- create a new epic.
pub async fn create_epic_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateEpicRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let title = body.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::InvalidInput("title is required".to_string()));
    }

    let conn = state.db_lock()?;

    // Determine next epic number from DB.
    let max_num: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(CAST(SUBSTR(id, 4, INSTR(SUBSTR(id, 4), '-') - 1) AS INTEGER)), 0) FROM epics WHERE id LIKE 'fn-%'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let epic_num = (max_num + 1) as u32;

    let slug = slugify(&title, 40).unwrap_or_else(|| format!("epic{epic_num}"));
    let epic_id = format!("fn-{epic_num}-{slug}");

    let epic = flowctl_core::types::Epic {
        schema_version: 1,
        id: epic_id.clone(),
        title: title.clone(),
        status: flowctl_core::types::EpicStatus::Open,
        branch_name: None,
        plan_review: flowctl_core::types::ReviewStatus::Unknown,
        completion_review: flowctl_core::types::ReviewStatus::Unknown,
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        file_path: Some(format!("epics/{epic_id}.md")),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let repo = flowctl_db::EpicRepo::new(&conn);
    repo.upsert(&epic)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "id": epic_id, "title": title})),
    ))
}

/// POST /api/v1/tasks/skip -- skip a task.
pub async fn skip_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskReasonRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);
    let task = repo
        .get(&body.task_id)
        .map_err(|_| AppError::InvalidInput(format!("task not found: {}", body.task_id)))?;

    Transition::new(task.status, Status::Skipped).map_err(|e| {
        AppError::InvalidTransition(format!("cannot skip task '{}': {}", body.task_id, e))
    })?;

    repo.update_status(&body.task_id, Status::Skipped)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "id": body.task_id,
        "status": "skipped"
    })))
}

/// POST /api/v1/tasks/block -- block a task.
pub async fn block_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskReasonRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let reason = body.reason.clone();
    let db = state.db.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| ServiceError::ValidationError("DB lock poisoned".to_string()))?;
        let flow_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(FLOW_DIR);
        let req = BlockTaskRequest { task_id, reason };
        flowctl_service::lifecycle::block_task(Some(&conn), &flow_dir, req)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking failed: {e}")))?;

    match result {
        Ok(resp) => Ok(Json(serde_json::json!({
            "success": true,
            "id": resp.task_id,
            "status": "blocked"
        }))),
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/restart -- restart a task and cascade.
pub async fn restart_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let db = state.db.clone();

    let result = tokio::task::spawn_blocking(move || {
        let conn = db
            .lock()
            .map_err(|_| ServiceError::ValidationError("DB lock poisoned".to_string()))?;
        let flow_dir = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(FLOW_DIR);
        let req = RestartTaskRequest {
            task_id,
            dry_run: false,
            force: true,
        };
        flowctl_service::lifecycle::restart_task(Some(&conn), &flow_dir, req)
    })
    .await
    .map_err(|e| AppError::Internal(format!("spawn_blocking failed: {e}")))?;

    match result {
        Ok(resp) => Ok(Json(serde_json::json!({
            "success": true,
            "reset": resp.reset_ids
        }))),
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// GET /api/v1/config -- read .flow/config.json.
pub async fn config_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config_path = state
        .runtime
        .paths
        .state_dir
        .parent() // .flow/
        .map(|flow_dir| flow_dir.join(CONFIG_FILE))
        .ok_or_else(|| AppError::Internal("cannot resolve .flow/ directory".to_string()))?;

    if !config_path.exists() {
        return Ok(Json(serde_json::json!({})));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| AppError::Internal(format!("failed to read config: {e}")))?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AppError::Internal(format!("invalid config JSON: {e}")))?;
    Ok(Json(value))
}

/// GET /api/v1/memory -- list memory entries.
pub async fn memory_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<MemoryQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;

    let mut sql = String::from("SELECT id, entry_type, content, summary, module, severity, problem_type, track, created_at FROM memory WHERE 1=1");
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref track) = params.track {
        bind_values.push(track.clone());
        sql.push_str(&format!(" AND track = ?{}", bind_values.len()));
    }
    if let Some(ref module) = params.module {
        bind_values.push(module.clone());
        sql.push_str(&format!(" AND module = ?{}", bind_values.len()));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut stmt = conn.prepare(&sql).map_err(|e| AppError::Db(e.to_string()))?;
    let params_slice: Vec<&dyn rusqlite::types::ToSql> =
        bind_values.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt
        .query_map(params_slice.as_slice(), |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "entry_type": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "summary": row.get::<_, Option<String>>(3)?,
                "module": row.get::<_, Option<String>>(4)?,
                "severity": row.get::<_, Option<String>>(5)?,
                "problem_type": row.get::<_, Option<String>>(6)?,
                "track": row.get::<_, Option<String>>(7)?,
                "created_at": row.get::<_, String>(8)?,
            }))
        })
        .map_err(|e| AppError::Db(e.to_string()))?;

    let entries: Vec<serde_json::Value> = rows.filter_map(|r| r.ok()).collect();
    Ok(Json(serde_json::json!({"entries": entries})))
}

/// GET /api/v1/stats -- global statistics.
pub async fn stats_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let stats = flowctl_db::StatsQuery::new(&conn);
    let summary = stats.summary()?;
    let value = serde_json::to_value(&summary)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    Ok(Json(value))
}

/// GET /api/v1/events -- WebSocket upgrade for live event streaming.
pub async fn events_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rx = state.event_bus.subscribe();
    let cancel = state.runtime.cancel.clone();
    ws.on_upgrade(move |socket| handle_event_socket(socket, rx, cancel))
}

/// Handle a single WebSocket connection: stream events until the client
/// disconnects or the daemon shuts down.
async fn handle_event_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<TimestampedEvent>,
    cancel: tokio_util::sync::CancellationToken,
) {
    info!("WebSocket client connected for event streaming");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("daemon shutting down, closing WebSocket");
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        match serde_json::to_string(&event) {
                            Ok(json) => {
                                if socket.send(Message::Text(json.into())).await.is_err() {
                                    debug!("WebSocket client disconnected");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("failed to serialize event: {e}");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "WebSocket client lagged, skipping events");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("event bus closed, closing WebSocket");
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("WebSocket client disconnected");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(_)) => {
                        // Ignore other messages from client.
                    }
                    Some(Err(e)) => {
                        debug!("WebSocket error: {e}");
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}
