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
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::EpicRepo::new(conn);
    let epics = repo.list(None).await?;
    let value = serde_json::to_value(&epics)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    Ok(Json(value))
}

/// GET /api/v1/tasks -- list tasks, optionally filtered by epic_id query param.
pub async fn tasks_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TasksQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::TaskRepo::new(conn);
    let tasks = if let Some(ref epic_id) = params.epic_id {
        repo.list_by_epic(epic_id).await?
    } else {
        repo.list_all(None, None).await?
    };
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
    let conn = state.db.clone();
    let log = flowctl_db_lsql::EventLog::new(conn);

    if let Some(ref task_id) = params.task_id {
        let rows = log.tokens_by_task(task_id).await?;
        let value = serde_json::to_value(&rows)
            .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
        Ok(Json(value))
    } else if let Some(ref epic_id) = params.epic_id {
        let summaries = log.tokens_by_epic(epic_id).await?;
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
    let conn = state.db.clone();

    // Build SQL with optional filters.
    let mut sql = String::from(
        "SELECT id, entry_type, content, summary, module, severity, problem_type, track, created_at FROM memory WHERE 1=1",
    );
    let mut bind: Vec<libsql::Value> = Vec::new();

    if let Some(ref track) = params.track {
        bind.push(libsql::Value::Text(track.clone()));
        sql.push_str(&format!(" AND track = ?{}", bind.len()));
    }
    if let Some(ref module) = params.module {
        bind.push(libsql::Value::Text(module.clone()));
        sql.push_str(&format!(" AND module = ?{}", bind.len()));
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut rows = conn
        .query(&sql, bind)
        .await
        .map_err(|e| AppError::Db(e.to_string()))?;

    let mut entries: Vec<serde_json::Value> = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| AppError::Db(e.to_string()))? {
        entries.push(serde_json::json!({
            "id": row.get::<i64>(0).unwrap_or(0),
            "entry_type": row.get::<String>(1).unwrap_or_default(),
            "content": row.get::<String>(2).unwrap_or_default(),
            "summary": row.get::<Option<String>>(3).unwrap_or_default(),
            "module": row.get::<Option<String>>(4).unwrap_or_default(),
            "severity": row.get::<Option<String>>(5).unwrap_or_default(),
            "problem_type": row.get::<Option<String>>(6).unwrap_or_default(),
            "track": row.get::<Option<String>>(7).unwrap_or_default(),
            "created_at": row.get::<String>(8).unwrap_or_default(),
        }));
    }
    Ok(Json(serde_json::json!({"entries": entries})))
}

/// GET /api/v1/stats -- global statistics.
pub async fn stats_handler(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let stats = flowctl_db_lsql::StatsQuery::new(conn);
    let summary = stats.summary().await?;
    let value = serde_json::to_value(&summary)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    Ok(Json(value))
}
