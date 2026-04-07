//! Error types for the file-based storage layer.

use std::fmt;

/// Unified error type for flowctl-db operations.
#[derive(Debug)]
pub enum DbError {
    /// Wraps a `json_store::StoreError`.
    Store(flowctl_core::json_store::StoreError),

    /// Serialization / deserialization error.
    Serialize(serde_json::Error),

    /// Entity not found.
    NotFound(String),

    /// Constraint violation (e.g. file lock conflict).
    Constraint(String),

    /// Invalid input.
    InvalidInput(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => write!(f, "store error: {e}"),
            Self::Serialize(e) => write!(f, "serialization error: {e}"),
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::Constraint(msg) => write!(f, "constraint violation: {msg}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<flowctl_core::json_store::StoreError> for DbError {
    fn from(e: flowctl_core::json_store::StoreError) -> Self {
        match e {
            flowctl_core::json_store::StoreError::NotFound(msg) => Self::NotFound(msg),
            other => Self::Store(other),
        }
    }
}

impl From<serde_json::Error> for DbError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialize(e)
    }
}
