//! Connection management for the service layer.
//!
//! Wraps `flowctl_db::open()` behind a trait so that:
//! - Sync callers (CLI) use it directly
//! - Async callers (daemon) use `spawn_blocking` to avoid blocking the runtime
//!
//! The `ConnectionProvider` trait enables testing with in-memory databases.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{ServiceError, ServiceResult};

/// Trait for obtaining a database connection.
///
/// The default implementation opens a file-backed SQLite database via
/// `flowctl_db::open()`. Tests can provide an in-memory alternative.
pub trait ConnectionProvider: Send + Sync {
    /// Open a new database connection.
    ///
    /// Each call returns a fresh `Connection`. rusqlite `Connection` is
    /// `!Send`, so callers in async contexts must use `spawn_blocking`.
    fn connect(&self) -> ServiceResult<Connection>;
}

/// File-backed connection provider using a working directory.
///
/// Resolves the database path via `flowctl_db::pool::resolve_db_path()`
/// and opens with production PRAGMAs + migrations.
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
}

impl ConnectionProvider for FileConnectionProvider {
    fn connect(&self) -> ServiceResult<Connection> {
        flowctl_db::open(&self.working_dir).map_err(ServiceError::from)
    }
}

/// Open a connection synchronously (convenience for CLI callers).
pub fn open_sync(working_dir: &Path) -> ServiceResult<Connection> {
    flowctl_db::open(working_dir).map_err(ServiceError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory connection provider for tests.
    pub struct MemoryConnectionProvider;

    impl ConnectionProvider for MemoryConnectionProvider {
        fn connect(&self) -> ServiceResult<Connection> {
            let conn = Connection::open_in_memory()
                .map_err(|e| ServiceError::DbError(flowctl_db::DbError::Sqlite(e)))?;
            Ok(conn)
        }
    }

    #[test]
    fn file_provider_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = FileConnectionProvider::new(tmp.path());
        let conn = provider.connect();
        assert!(conn.is_ok(), "should open file-backed connection");
    }

    #[test]
    fn memory_provider_works() {
        let provider = MemoryConnectionProvider;
        let conn = provider.connect();
        assert!(conn.is_ok(), "should open in-memory connection");
    }

    #[test]
    fn open_sync_works() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = open_sync(tmp.path());
        assert!(conn.is_ok(), "open_sync should succeed");
    }
}
