//! Error types for the libSQL storage layer.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("libsql error: {0}")]
    LibSql(#[from] libsql::Error),

    #[error("state directory error: {0}")]
    StateDir(String),

    #[error("schema error: {0}")]
    Schema(String),

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("constraint violation: {0}")]
    Constraint(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),
}
