//! Core error types for flowctl-core.
//!
//! Uses `thiserror` for library errors (not `anyhow`, which is for apps).

use thiserror::Error;

/// Top-level error type for flowctl-core operations.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Invalid ID format.
    #[error("invalid ID: {0}")]
    InvalidId(String),

    /// Invalid state transition.
    #[error("invalid transition from {from} to {to}")]
    InvalidTransition {
        from: crate::state_machine::Status,
        to: crate::state_machine::Status,
    },

    /// Slug generation produced empty result.
    #[error("slugify produced empty result for input: {0}")]
    EmptySlug(String),

    /// Task not found.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// Epic not found.
    #[error("epic not found: {0}")]
    EpicNotFound(String),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Frontmatter parse error.
    #[error("frontmatter parse error: {0}")]
    FrontmatterParse(String),

    /// Frontmatter serialization error.
    #[error("frontmatter serialization error: {0}")]
    FrontmatterSerialize(String),

    /// Cycle detected in task dependency graph.
    #[error("cycle detected in task DAG: {0}")]
    CycleDetected(String),

    /// Dependency references a task not in the graph.
    #[error("unknown dependency: task {task} depends on {dependency}")]
    UnknownDependency { task: String, dependency: String },

    /// Duplicate task ID in input.
    #[error("duplicate task ID: {0}")]
    DuplicateTask(String),
}

// ── ServiceError ────────────────────────────────────────────────────

/// Top-level error type for service/lifecycle operations.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// Task not found.
    #[error("task not found: {0}")]
    TaskNotFound(String),

    /// Epic not found.
    #[error("epic not found: {0}")]
    EpicNotFound(String),

    /// Invalid state transition (e.g., done -> in_progress without restart).
    #[error("invalid transition: {0}")]
    InvalidTransition(String),

    /// A dependency is not satisfied (blocking task not done/skipped).
    #[error("dependency unsatisfied: task {task} blocked by {dependency}")]
    DependencyUnsatisfied { task: String, dependency: String },

    /// Cross-actor violation (e.g., modifying another agent's locked task).
    #[error("cross-actor violation: {0}")]
    CrossActorViolation(String),

    /// Underlying store error.
    #[error("store error: {0}")]
    StoreError(#[from] crate::json_store::StoreError),

    /// I/O error (file reads, state directory operations).
    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    /// Validation error (bad input, missing fields, constraint checks).
    #[error("validation error: {0}")]
    ValidationError(String),

    /// Core-layer error (ID parsing, DAG operations).
    #[error("core error: {0}")]
    CoreError(#[from] CoreError),
}

/// Convenience alias used throughout the service layer.
pub type ServiceResult<T> = Result<T, ServiceError>;
