//! HTTP API route handlers for the daemon.
//!
//! Provides REST endpoints for status, epics, tasks, and a WebSocket
//! endpoint for streaming live events to connected clients.

pub mod common;
pub mod dag;
pub mod epic;
pub mod task;
pub mod ws;

// Re-export all public types from submodules for backward compatibility.
pub use common::{AppError, AppState, DaemonState};
pub use dag::{
    add_dep_handler, dag_detail_handler, dag_handler, dag_mutate_handler, remove_dep_handler,
    DagEdge, DagNode, DagResponse,
};
pub use epic::{create_epic_handler, set_epic_plan_handler, start_epic_work_handler};
pub use task::{
    block_task_handler, block_task_rest_handler, create_task_handler, done_task_handler,
    done_task_rest_handler, get_task_handler, restart_task_handler, restart_task_rest_handler,
    skip_task_handler, skip_task_rest_handler, start_task_handler, start_task_rest_handler,
};
pub use ws::events_ws_handler;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use flowctl_core::types::CONFIG_FILE;

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
