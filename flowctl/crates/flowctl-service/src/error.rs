//! Service-layer error types.
//!
//! `ServiceError` is the canonical error type for all business logic
//! operations. It wraps lower-level errors from `flowctl-core` and
//! `flowctl-db` and adds service-specific variants.

use thiserror::Error;

/// Top-level error type for service operations.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Task not found in the database.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// Epic not found in the database.
    #[error("epic not found: {0}")]
    EpicNotFound(String),

    /// Invalid state transition (e.g., done → in_progress without restart).
    #[error("invalid transition: {0}")]
    InvalidTransition(String),

    /// A dependency is not satisfied (blocking task not done/skipped).
    #[error("dependency unsatisfied: task {task} blocked by {dependency}")]
    DependencyUnsatisfied { task: String, dependency: String },

    /// Cross-actor violation (e.g., modifying another agent's locked task).
    #[error("cross-actor violation: {0}")]
    CrossActorViolation(String),

    /// Underlying database error.
    #[error("database error: {0}")]
    DbError(#[from] flowctl_db::DbError),

    /// I/O error (file reads, state directory operations).
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    /// Validation error (bad input, missing fields, constraint checks).
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Core-layer error (ID parsing, DAG operations).
    #[error("core error: {0}")]
    CoreError(#[from] flowctl_core::CoreError),
}

/// Convenience alias used throughout the service layer.
pub type ServiceResult<T> = Result<T, ServiceError>;
