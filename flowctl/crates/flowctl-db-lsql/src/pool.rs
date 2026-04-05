//! Async libSQL connection setup and schema application.
//!
//! # Architecture
//!
//! - **libSQL** is fully async, Tokio-based. All DB calls are `.await`.
//! - Schema is applied once on open via `apply_schema()` — a single SQL
//!   blob (see `schema.sql`). Fresh DBs only; no migration story.
//! - `libsql::Connection` is cheap and `Clone`. Pass by value; do not wrap
//!   in `Arc<Mutex<_>>`.
//! - PRAGMAs (WAL, busy_timeout, foreign_keys) are set per-connection on
//!   each `open_async()` call.
//!
//! # In-memory databases
//!
//! libSQL `:memory:` databases are **connection-scoped**: schema applied on
//! one connection is not visible to another from the same `Database`.
//! `open_memory_async()` returns both the `Database` AND the `Connection`
//! with schema applied — callers must use that connection directly.

use std::path::{Path, PathBuf};
use std::process::Command;

use libsql::{Builder, Connection, Database};

use crate::error::DbError;

/// Embedded schema applied to fresh databases.
const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Resolve the state directory for the flowctl database.
///
/// Uses `git rev-parse --git-common-dir` so worktrees share a single DB.
/// Falls back to `.flow/.state/` if not in a git repo.
pub fn resolve_state_dir(working_dir: &Path) -> Result<PathBuf, DbError> {
    let git_result = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(working_dir)
        .output();

    match git_result {
        Ok(output) if output.status.success() => {
            let git_common = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let git_common_path = if Path::new(&git_common).is_absolute() {
                PathBuf::from(git_common)
            } else {
                working_dir.join(git_common)
            };
            Ok(git_common_path.join("flow-state"))
        }
        _ => Ok(working_dir.join(".flow").join(".state")),
    }
}

/// Resolve the full libSQL database file path.
pub fn resolve_libsql_path(working_dir: &Path) -> Result<PathBuf, DbError> {
    let state_dir = resolve_state_dir(working_dir)?;
    Ok(state_dir.join("flowctl.db"))
}

/// Apply production PRAGMAs to a libSQL connection.
async fn apply_pragmas(conn: &Connection) -> Result<(), DbError> {
    for pragma in [
        "PRAGMA journal_mode = WAL",
        "PRAGMA busy_timeout = 5000",
        "PRAGMA synchronous = NORMAL",
        "PRAGMA foreign_keys = ON",
        "PRAGMA wal_autocheckpoint = 1000",
    ] {
        conn.execute(pragma, ())
            .await
            .map_err(|e| DbError::Schema(format!("pragma {pragma}: {e}")))?;
    }
    Ok(())
}

/// Apply the full libSQL schema to a fresh database.
async fn apply_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(SCHEMA_SQL)
        .await
        .map_err(|e| DbError::Schema(format!("schema apply failed: {e}")))?;
    Ok(())
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
        .map_err(|e| DbError::Schema(format!("libsql open: {e}")))?;

    let conn = db.connect()?;
    apply_pragmas(&conn).await?;
    apply_schema(&conn).await?;

    Ok(db)
}

/// Open an in-memory libSQL database for testing.
///
/// Returns both the `Database` handle and a `Connection` with schema
/// applied. The connection must be kept alive to access the in-memory
/// database (libsql `:memory:` DBs are connection-scoped).
pub async fn open_memory_async() -> Result<(Database, Connection), DbError> {
    let db = Builder::new_local(":memory:")
        .build()
        .await
        .map_err(|e| DbError::Schema(format!("libsql open_memory: {e}")))?;

    let conn = db.connect()?;
    apply_pragmas(&conn).await.ok();
    apply_schema(&conn).await?;

    Ok((db, conn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_memory_async() {
        let (_db, conn) = open_memory_async()
            .await
            .expect("should open in-memory libsql db");

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

        for expected in [
            "epics",
            "tasks",
            "task_deps",
            "epic_deps",
            "file_ownership",
            "runtime_state",
            "file_locks",
            "heartbeats",
            "phase_progress",
            "evidence",
            "events",
            "token_usage",
            "daily_rollup",
            "monthly_rollup",
            "memory",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "{expected} table missing; tables={tables:?}"
            );
        }
    }

    #[tokio::test]
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
    async fn test_memory_has_embedding_column() {
        let (_db, conn) = open_memory_async().await.unwrap();

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
    async fn test_event_trigger_fires() {
        let (_db, conn) = open_memory_async().await.unwrap();

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
