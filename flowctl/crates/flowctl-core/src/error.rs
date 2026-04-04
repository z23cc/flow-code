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
