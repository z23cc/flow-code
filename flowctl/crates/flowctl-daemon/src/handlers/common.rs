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
    pub fn db_lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, AppError> {
        self.db
            .lock()
            .map_err(|_| AppError::Internal("DB lock poisoned".to_string()))
    }
}

/// Map service-layer errors to HTTP-appropriate AppErrors.
pub fn service_error_to_app_error(e: ServiceError) -> AppError {
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
pub fn touch_updated_at(conn: &rusqlite::Connection, task_id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE tasks SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![chrono::Utc::now().to_rfc3339(), task_id],
    ).map_err(|e| AppError::Db(e.to_string()))?;
    Ok(())
}
