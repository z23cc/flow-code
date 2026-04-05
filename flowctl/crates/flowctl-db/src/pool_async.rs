//! Async connection pool using libSQL.
//!
//! This module is the new target data layer for flowctl. The legacy sync
//! pool (`pool.rs`) will be removed in the final task of the migration epic.
//!
//! # Architecture
//!
//! - **libSQL** is fully async, Tokio-based. All DB calls are `.await`.
//! - Schema is applied once on open via `apply_schema()` — a single SQL
//!   blob (see `schema_libsql.sql`). Fresh DBs only; no migrations from
//!   the old rusqlite schema.
//! - `libsql::Connection` is cheap and `Clone`. Pass by value; do not wrap
//!   in `Arc<Mutex<_>>`.
//! - PRAGMAs (WAL, busy_timeout, foreign_keys) are set per-connection on
//!   each `open_async()` call.

use std::path::{Path, PathBuf};

use libsql::{Builder, Connection, Database};

use crate::error::DbError;
use crate::pool::{resolve_db_path, resolve_state_dir};

/// Embedded schema applied to fresh databases.
const SCHEMA_SQL: &str = include_str!("schema_libsql.sql");

/// Apply production PRAGMAs to a libSQL connection.
async fn apply_pragmas(conn: &Connection) -> Result<(), DbError> {
    // Set pragmas individually; libsql execute_batch behaves differently
    // from rusqlite for multi-statement SQL.
    for pragma in [
        "PRAGMA journal_mode = WAL",
        "PRAGMA busy_timeout = 5000",
        "PRAGMA synchronous = NORMAL",
        "PRAGMA foreign_keys = ON",
        "PRAGMA wal_autocheckpoint = 1000",
    ] {
        conn.execute(pragma, ())
            .await
            .map_err(|e| DbError::Migration(format!("pragma {pragma}: {e}")))?;
    }
    Ok(())
}

/// Apply the full libSQL schema to a fresh database.
///
/// Idempotent — uses `CREATE TABLE IF NOT EXISTS` throughout. Safe to call
/// on both fresh and existing DBs.
async fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(SCHEMA_SQL)
        .await
        .map_err(|e| DbError::Migration(format!("schema apply failed: {e}")))?;
    Ok(())
}

/// Resolve the libSQL database file path.
///
/// Uses a different filename (`flowctl_libsql.db`) than the legacy rusqlite
/// DB so both can coexist during migration. Removed in the final cleanup task.
pub fn resolve_libsql_path(working_dir: &Path) -> Result<PathBuf, DbError> {
    let state_dir = resolve_state_dir(working_dir)?;
    Ok(state_dir.join("flowctl_libsql.db"))
}

/// Open a file-backed libSQL database with schema applied.
pub async fn open_async(working_dir: &Path) -> Result<Database, DbError> {
    let db_path = resolve_libsql_path(working_dir)?;

    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            DbError::StateDir(format!("failed to create {}: {e}", parent.display()))
        })?;
    }

    let db = Builder::new_local(&db_path)
        .build()
        .await
        .map_err(|e| DbError::Migration(format!("libsql open: {e}")))?;

    let conn = db
        .connect()
        .map_err(|e| DbError::Migration(format!("libsql connect: {e}")))?;

    apply_pragmas(&conn).await?;
    apply_schema(&conn).await?;

    Ok(db)
}

/// Open an in-memory libSQL database for testing.
///
/// Returns both the `Database` handle and a `Connection` with schema applied.
/// The connection must be kept alive to access the in-memory database
/// (libsql `:memory:` DBs are connection-scoped).
pub async fn open_memory_async() -> Result<(Database, Connection), DbError> {
    let db = Builder::new_local(":memory:")
        .build()
        .await
        .map_err(|e| DbError::Migration(format!("libsql open_memory: {e}")))?;

    let conn = db
        .connect()
        .map_err(|e| DbError::Migration(format!("libsql connect: {e}")))?;

    // In-memory dbs ignore journal_mode but other pragmas are fine.
    apply_pragmas(&conn).await.ok();
    apply_schema(&conn).await?;

    Ok((db, conn))
}

// Placeholder for the legacy-compat alias path — kept unused for now. Legacy
// rusqlite path is the current `pool::resolve_db_path`, which uses
// `flowctl.db` filename. Tasks 2-6 migrate callers to use open_async.
#[allow(dead_code)]
fn _keep_legacy_path_referenced(wd: &Path) -> Result<PathBuf, DbError> {
    resolve_db_path(wd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn test_debug_real_schema() {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute_batch(SCHEMA_SQL).await;
        assert!(r.is_ok(), "schema apply failed: {r:?}");

        let mut rows = conn
            .query("SELECT name FROM sqlite_master WHERE type='table'", ())
            .await
            .unwrap();
        let mut tables = vec![];
        while let Some(row) = rows.next().await.unwrap() {
            tables.push(row.get::<String>(0).unwrap());
        }
        assert!(tables.contains(&"epics".to_string()), "tables={tables:?}");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_debug_minimal_batch() {
        let db = Builder::new_local(":memory:").build().await.unwrap();
        let conn = db.connect().unwrap();
        let r = conn
            .execute_batch("CREATE TABLE t1 (id INTEGER); CREATE TABLE t2 (id INTEGER);")
            .await;
        assert!(r.is_ok(), "batch result: {r:?}");

        let mut rows = conn
            .query("SELECT name FROM sqlite_master WHERE type='table'", ())
            .await
            .unwrap();
        let mut tables = vec![];
        while let Some(row) = rows.next().await.unwrap() {
            tables.push(row.get::<String>(0).unwrap());
        }
        assert!(tables.contains(&"t1".to_string()), "tables={tables:?}");
        assert!(tables.contains(&"t2".to_string()), "tables={tables:?}");

        // Try F32_BLOB column
        let r2 = conn
            .execute_batch("CREATE TABLE v (id INTEGER, emb F32_BLOB(4));")
            .await;
        assert!(r2.is_ok(), "f32_blob result: {r2:?}");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_open_memory_async() {
        let (_db, conn) = open_memory_async()
            .await
            .expect("should open in-memory libsql db");

        // Verify core tables exist.
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
                (),
            )
            .await
            .unwrap();

        let mut tables: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            tables.push(row.get::<String>(0).unwrap());
        }

        assert!(tables.contains(&"epics".to_string()), "epics table missing");
        assert!(tables.contains(&"tasks".to_string()), "tasks table missing");
        assert!(
            tables.contains(&"task_deps".to_string()),
            "task_deps table missing"
        );
        assert!(
            tables.contains(&"runtime_state".to_string()),
            "runtime_state table missing"
        );
        assert!(
            tables.contains(&"memory".to_string()),
            "memory table missing"
        );
        assert!(
            tables.contains(&"events".to_string()),
            "events table missing"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_insert_and_query_async() {
        let (_db, conn) = open_memory_async().await.unwrap();

        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![
                "fn-test-1",
                "Test Epic",
                "open",
                "epics/fn-test-1.md",
                "2026-04-05T00:00:00Z",
                "2026-04-05T00:00:00Z"
            ],
        )
        .await
        .unwrap();

        let mut rows = conn
            .query(
                "SELECT title FROM epics WHERE id = ?1",
                libsql::params!["fn-test-1"],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().expect("row exists");
        let title: String = row.get(0).unwrap();
        assert_eq!(title, "Test Epic");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_memory_has_embedding_column() {
        let (_db, conn) = open_memory_async().await.unwrap();

        // Verify the embedding column exists.
        let mut rows = conn
            .query("SELECT name FROM pragma_table_info('memory')", ())
            .await
            .unwrap();

        let mut cols: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            cols.push(row.get::<String>(0).unwrap());
        }

        assert!(
            cols.contains(&"embedding".to_string()),
            "embedding column missing: {cols:?}"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_event_trigger_fires() {
        let (_db, conn) = open_memory_async().await.unwrap();

        // Need an epic first for FK.
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            libsql::params![
                "fn-trg",
                "Trigger Test",
                "open",
                "epics/fn-trg.md",
                "2026-04-05T00:00:00Z",
                "2026-04-05T00:00:00Z"
            ],
        )
        .await
        .unwrap();

        conn.execute(
            "INSERT INTO events (epic_id, task_id, event_type, actor) VALUES (?1, ?2, ?3, ?4)",
            libsql::params!["fn-trg", "fn-trg.1", "task_completed", "worker"],
        )
        .await
        .unwrap();

        let mut rows = conn
            .query(
                "SELECT tasks_completed FROM daily_rollup WHERE epic_id = ?1",
                libsql::params!["fn-trg"],
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().expect("rollup row exists");
        let completed: i64 = row.get(0).unwrap();
        assert_eq!(completed, 1);
    }
}
