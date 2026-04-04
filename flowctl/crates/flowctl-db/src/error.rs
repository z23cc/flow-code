//! Error types for the flowctl-db crate.

use thiserror::Error;

/// Top-level error type for database operations.
#[derive(Debug, Error)]
pub enum DbError {
    /// SQLite error from rusqlite.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Migration error.
    #[error("migration error: {0}")]
    Migration(String),

    /// State directory resolution error.
    #[error("state directory error: {0}")]
    StateDir(String),

    /// Entity not found.
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    /// Constraint violation (e.g., duplicate key, FK violation).
    #[error("constraint violation: {0}")]
    Constraint(String),

    /// Serialization error (JSON payloads in evidence, events).
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
