//! Shared types and helpers used across all handler modules.

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

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
    /// Resource not found.
    NotFound(String),
    /// Internal error (serialization failure, lock poisoned, etc.)
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match &self {
            AppError::Db(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::InvalidTransition(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::InvalidInput(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl From<flowctl_db_lsql::DbError> for AppError {
    fn from(e: flowctl_db_lsql::DbError) -> Self {
        use flowctl_db_lsql::DbError;
        match e {
            DbError::NotFound(msg) => AppError::NotFound(msg),
            DbError::Constraint(msg) => AppError::InvalidInput(msg),
            DbError::InvalidInput(msg) => AppError::InvalidInput(msg),
            other => AppError::Db(other.to_string()),
        }
    }
}

/// Shared application state for all handlers.
pub type AppState = Arc<DaemonState>;

/// Combined daemon state: runtime + event bus + shared DB connection.
///
/// `libsql::Connection` is `Send + Sync + Clone` (cheap), so we hold it by
/// value and callers `state.db.clone()` when they need an owned copy for a
/// repository.
pub struct DaemonState {
    pub runtime: DaemonRuntime,
    pub event_bus: flowctl_scheduler::EventBus,
    pub db: libsql::Connection,
}

/// Map service-layer errors to HTTP-appropriate AppErrors.
pub fn service_error_to_app_error(e: ServiceError) -> AppError {
    match e {
        ServiceError::TaskNotFound(msg) => AppError::NotFound(msg),
        ServiceError::EpicNotFound(msg) => AppError::NotFound(msg),
        ServiceError::InvalidTransition(msg) => AppError::InvalidTransition(msg),
        ServiceError::DependencyUnsatisfied { task, dependency } => {
            AppError::InvalidInput(format!("task {task} blocked by {dependency}"))
        }
        ServiceError::CrossActorViolation(msg) => AppError::InvalidInput(msg),
        ServiceError::ValidationError(msg) => AppError::InvalidInput(msg),
        ServiceError::DbError(e) => AppError::from(e),
        ServiceError::IoError(e) => AppError::Internal(e.to_string()),
        ServiceError::CoreError(e) => AppError::Internal(e.to_string()),
    }
}

/// Check optimistic lock version against the task's `updated_at`.
pub fn check_version(task: &flowctl_core::types::Task, version: &str) -> Result<(), AppError> {
    let task_version = task.updated_at.to_rfc3339();
    if task_version != version {
        return Err(AppError::InvalidTransition(format!(
            "version conflict: expected {version}, got {task_version}"
        )));
    }
    Ok(())
}

/// Update the `updated_at` timestamp for a task.
pub async fn touch_updated_at(conn: &libsql::Connection, task_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
        libsql::params![chrono::Utc::now().to_rfc3339(), task_id.to_string()],
    )
    .await
    .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}
