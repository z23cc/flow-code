//! Connection management for the service layer (async libSQL).
//!
//! `libsql::Connection` is `Send + Sync` and cheap to `Clone`. Callers pass
//! it by value or reference. No mutex wrapping is needed.

use std::path::{Path, PathBuf};

use libsql::Connection;

use crate::error::{ServiceError, ServiceResult};

/// File-backed connection provider using a working directory.
///
/// Wraps `flowctl_db_lsql::open_async()` so callers can re-open as needed.
#[derive(Debug, Clone)]
pub struct FileConnectionProvider {
    working_dir: PathBuf,
}

impl FileConnectionProvider {
    /// Create a provider rooted at the given working directory.
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.into(),
        }
    }

    /// Return the working directory this provider is rooted at.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    /// Open a new libSQL connection asynchronously.
    pub async fn connect(&self) -> ServiceResult<Connection> {
        let db = flowctl_db_lsql::open_async(&self.working_dir)
            .await
            .map_err(ServiceError::from)?;
        db.connect().map_err(|e| {
            ServiceError::DbError(flowctl_db_lsql::DbError::LibSql(e))
        })
    }
}

/// Open a connection asynchronously (convenience wrapper around
/// `flowctl_db_lsql::open_async`).
pub async fn open_async(working_dir: &Path) -> ServiceResult<Connection> {
    let db = flowctl_db_lsql::open_async(working_dir)
        .await
        .map_err(ServiceError::from)?;
    db.connect()
        .map_err(|e| ServiceError::DbError(flowctl_db_lsql::DbError::LibSql(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_provider_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = FileConnectionProvider::new(tmp.path());
        let conn = provider.connect().await;
        assert!(conn.is_ok(), "should open file-backed connection: {:?}", conn.err());
    }

    #[tokio::test]
    async fn open_async_works() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_async(tmp.path()).await;
        assert!(conn.is_ok(), "open_async should succeed: {:?}", conn.err());
    }
}
