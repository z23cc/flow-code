//! Connection management and state directory resolution.
//!
//! Resolves the database path via `git rev-parse --git-common-dir` so that
//! worktrees share a single database. Opens connections with production
//! PRAGMAs (WAL, busy_timeout, etc.) and runs embedded migrations.

use std::path::{Path, PathBuf};
use std::process::Command;

use include_dir::{include_dir, Dir};
use rusqlite::Connection;
use rusqlite_migration::Migrations;

use crate::error::DbError;

/// Embedded migration files, compiled into the binary.
static MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/src/migrations");

/// Lazily built migrations from the embedded directory.
fn migrations() -> Migrations<'static> {
    Migrations::from_directory(&MIGRATIONS_DIR).expect("valid migration directory")
}

/// Resolve the state directory for the flowctl database.
///
/// Strategy: `git rev-parse --git-common-dir` + `/flow-state/`.
/// This ensures all worktrees share a single database file.
/// Falls back to `.flow/.state/` in the current directory if not in a git repo.
pub fn resolve_state_dir(working_dir: &Path) -> Result<PathBuf, DbError> {
    // Try git first for worktree-aware state sharing.
    // Falls back to local .flow/.state/ if git is unavailable or not a repo.
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
        _ => {
            // git not available, not a repo, or command failed — use local fallback.
            Ok(working_dir.join(".flow").join(".state"))
        }
    }
}

/// Resolve the full database file path.
pub fn resolve_db_path(working_dir: &Path) -> Result<PathBuf, DbError> {
    let state_dir = resolve_state_dir(working_dir)?;
    Ok(state_dir.join("flowctl.db"))
}

/// Apply production PRAGMAs to a connection.
///
/// These are set per-connection (not in migration files) because PRAGMAs
/// like journal_mode persist at the database level, while others like
/// busy_timeout are per-connection.
///
/// Note on macOS: SQLite's default fsync does not use F_FULLFSYNC, which
/// means data can be lost on power failure with certain hardware. For
/// flowctl this is acceptable because the SQLite database is a rebuildable
/// cache -- `flowctl reindex` recovers indexed data from Markdown files.
/// Runtime-only data (events, metrics) is best-effort by design.
fn apply_pragmas(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA wal_autocheckpoint = 1000;",
    )
    .map_err(DbError::Sqlite)
}

/// Open a database connection with production PRAGMAs and run migrations.
///
/// Creates the state directory and database file if they don't exist.
pub fn open(working_dir: &Path) -> Result<Connection, DbError> {
    let db_path = resolve_db_path(working_dir)?;

    // Ensure the state directory exists.
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            DbError::StateDir(format!("failed to create {}: {e}", parent.display()))
        })?;
    }

    let mut conn = Connection::open(&db_path).map_err(DbError::Sqlite)?;
    apply_pragmas(&conn)?;

    // Run pending migrations.
    migrations()
        .to_latest(&mut conn)
        .map_err(|e| DbError::Migration(e.to_string()))?;

    Ok(conn)
}

/// Open an in-memory database for testing. Applies PRAGMAs and migrations.
pub fn open_memory() -> Result<Connection, DbError> {
    let mut conn = Connection::open_in_memory().map_err(DbError::Sqlite)?;
    apply_pragmas(&conn)?;
    migrations()
        .to_latest(&mut conn)
        .map_err(|e| DbError::Migration(e.to_string()))?;
    Ok(conn)
}

/// Run auto-cleanup: delete old events and daily rollups.
///
/// - events older than 90 days
/// - daily_rollup older than 365 days
pub fn cleanup(conn: &Connection) -> Result<usize, DbError> {
    let events_deleted: usize = conn
        .execute(
            "DELETE FROM events WHERE timestamp < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-90 days')",
            [],
        )
        .map_err(DbError::Sqlite)?;

    let rollups_deleted: usize = conn
        .execute(
            "DELETE FROM daily_rollup WHERE day < strftime('%Y-%m-%d', 'now', '-365 days')",
            [],
        )
        .map_err(DbError::Sqlite)?;

    Ok(events_deleted + rollups_deleted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_memory() {
        let conn = open_memory().expect("should open in-memory db");
        // Verify tables exist by querying sqlite_master.
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"epics".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"task_deps".to_string()));
        assert!(tables.contains(&"epic_deps".to_string()));
        assert!(tables.contains(&"file_ownership".to_string()));
        assert!(tables.contains(&"runtime_state".to_string()));
        assert!(tables.contains(&"file_locks".to_string()));
        assert!(tables.contains(&"heartbeats".to_string()));
        assert!(tables.contains(&"phase_progress".to_string()));
        assert!(tables.contains(&"evidence".to_string()));
        assert!(tables.contains(&"events".to_string()));
        assert!(tables.contains(&"token_usage".to_string()));
        assert!(tables.contains(&"daily_rollup".to_string()));
        assert!(tables.contains(&"monthly_rollup".to_string()));
        assert!(tables.contains(&"memory".to_string()));
    }

    #[test]
    fn test_pragmas_applied() {
        let conn = open_memory().expect("should open in-memory db");

        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        // In-memory databases use "memory" journal mode regardless of setting.
        assert!(journal_mode == "memory" || journal_mode == "wal");

        let busy_timeout: i64 = conn
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);

        let foreign_keys: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn test_trigger_daily_rollup() {
        let conn = open_memory().expect("should open in-memory db");

        // Insert an epic first (FK constraint).
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test', 'open', 'epics/fn-1-test.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Insert a task_started event.
        conn.execute(
            "INSERT INTO events (epic_id, task_id, event_type, actor)
             VALUES ('fn-1-test', 'fn-1-test.1', 'task_started', 'worker')",
            [],
        )
        .unwrap();

        // Insert a task_completed event.
        conn.execute(
            "INSERT INTO events (epic_id, task_id, event_type, actor)
             VALUES ('fn-1-test', 'fn-1-test.1', 'task_completed', 'worker')",
            [],
        )
        .unwrap();

        // Verify daily_rollup was auto-populated.
        let (started, completed): (i64, i64) = conn
            .query_row(
                "SELECT tasks_started, tasks_completed FROM daily_rollup WHERE epic_id = 'fn-1-test'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(started, 1);
        assert_eq!(completed, 1);
    }

    #[test]
    fn test_resolve_state_dir_in_git_repo() {
        // Create a temp dir with a git repo.
        let tmp = std::env::temp_dir().join("flowctl-test-state-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output()
            .unwrap();

        let state_dir = resolve_state_dir(&tmp).unwrap();
        assert!(state_dir.to_string_lossy().contains("flow-state"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_open_file_based() {
        let tmp = std::env::temp_dir().join("flowctl-test-open-file");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output()
            .unwrap();

        let conn = open(&tmp).expect("should open file-based db");

        // Verify tables exist.
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // 14 tables + sqlite_sequence (from AUTOINCREMENT)
        assert!(count >= 14, "expected at least 14 tables, got {count}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_cleanup_noop_on_fresh_db() {
        let conn = open_memory().expect("should open in-memory db");
        let deleted = cleanup(&conn).unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_idempotent_migrations() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_pragmas(&conn).unwrap();

        // Run migrations twice -- should be idempotent.
        migrations().to_latest(&mut conn).unwrap();
        migrations().to_latest(&mut conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count >= 14);
    }
}
