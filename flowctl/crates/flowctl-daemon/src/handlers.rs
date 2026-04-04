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

use flowctl_core::id::is_task_id;
use flowctl_core::state_machine::{Status, Transition};
use flowctl_scheduler::TimestampedEvent;

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
    pub db: std::sync::Mutex<rusqlite::Connection>,
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

/// POST /api/v1/tasks/start -- start a task (validates state transition).
pub async fn start_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);

    // Get current task to validate transition.
    let task = repo
        .get(&body.task_id)
        .map_err(|_| AppError::InvalidInput(format!("task not found: {}", body.task_id)))?;

    Transition::new(task.status, Status::InProgress).map_err(|e| {
        AppError::InvalidTransition(format!(
            "cannot start task '{}': {}",
            body.task_id, e
        ))
    })?;

    repo.update_status(&body.task_id, Status::InProgress)?;
    Ok(Json(
        serde_json::json!({"success": true, "id": body.task_id}),
    ))
}

/// POST /api/v1/tasks/done -- complete a task (validates state transition).
pub async fn done_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db_lock()?;
    let repo = flowctl_db::TaskRepo::new(&conn);

    // Get current task to validate transition.
    let task = repo
        .get(&body.task_id)
        .map_err(|_| AppError::InvalidInput(format!("task not found: {}", body.task_id)))?;

    Transition::new(task.status, Status::Done).map_err(|e| {
        AppError::InvalidTransition(format!(
            "cannot complete task '{}': {}",
            body.task_id, e
        ))
    })?;

    repo.update_status(&body.task_id, Status::Done)?;
    Ok(Json(
        serde_json::json!({"success": true, "id": body.task_id}),
    ))
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
