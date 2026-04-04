//! Runtime state migration from Python's JSON files to SQLite.
//!
//! Reads Python runtime state from `git-common-dir/flow-state/tasks/*.state.json`
//! and inserts into the `runtime_state` table. Also provides auto-detection
//! of missing SQLite databases when `.flow/` JSON files exist.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use tracing::{info, warn};

use flowctl_core::id::is_task_id;

use crate::error::DbError;

/// Result of a migration operation.
#[derive(Debug, Default)]
pub struct MigrationResult {
    /// Number of runtime state files migrated.
    pub states_migrated: usize,
    /// Files that could not be migrated.
    pub files_skipped: usize,
    /// Warnings collected during migration.
    pub warnings: Vec<String>,
}

/// Migrate Python runtime state files into SQLite.
///
/// Reads `{state_dir}/tasks/*.state.json` files and inserts/replaces
/// rows in the `runtime_state` table. This is idempotent.
///
/// # Arguments
/// * `conn` - Open database connection
/// * `state_dir` - Path to the state directory (e.g., `git-common-dir/flow-state/`)
pub fn migrate_runtime_state(
    conn: &Connection,
    state_dir: &Path,
) -> Result<MigrationResult, DbError> {
    let mut result = MigrationResult::default();

    let tasks_state_dir = state_dir.join("tasks");
    if !tasks_state_dir.is_dir() {
        return Ok(result);
    }

    let entries = match fs::read_dir(&tasks_state_dir) {
        Ok(e) => e,
        Err(_) => return Ok(result),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Only process *.state.json files.
        if !name.ends_with(".state.json") {
            continue;
        }

        // Extract task ID: "fn-1-test.1.state.json" -> "fn-1-test.1"
        let task_id = name.trim_end_matches(".state.json");
        if !is_task_id(task_id) {
            let msg = format!("skipping non-task state file: {name}");
            warn!("{}", msg);
            result.warnings.push(msg);
            result.files_skipped += 1;
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        let state: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("invalid JSON in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        conn.execute(
            "INSERT OR REPLACE INTO runtime_state
             (task_id, assignee, claimed_at, completed_at, duration_secs, blocked_reason, baseline_rev, final_rev)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                task_id,
                state.get("assignee").and_then(|v| v.as_str()),
                state.get("claimed_at").and_then(|v| v.as_str()),
                state.get("completed_at").and_then(|v| v.as_str()),
                state
                    .get("duration_secs")
                    .or_else(|| state.get("duration_seconds"))
                    .and_then(|v| v.as_i64()),
                state.get("blocked_reason").and_then(|v| v.as_str()),
                state.get("baseline_rev").and_then(|v| v.as_str()),
                state.get("final_rev").and_then(|v| v.as_str()),
            ],
        )?;

        result.states_migrated += 1;
    }

    info!(
        migrated = result.states_migrated,
        skipped = result.files_skipped,
        "runtime state migration complete"
    );

    Ok(result)
}

/// Check if a reindex is needed: SQLite DB is missing but `.flow/` has data.
///
/// Returns `true` if `.flow/epics/` or `.flow/tasks/` contain `.md` files
/// but the SQLite database does not exist at the expected path.
pub fn needs_reindex(flow_dir: &Path, db_path: &Path) -> bool {
    if db_path.exists() {
        return false;
    }

    has_md_files(&flow_dir.join("epics")) || has_md_files(&flow_dir.join("tasks"))
}

/// Check if a directory contains `.flow/` JSON state files that indicate
/// a Python runtime was in use.
///
/// Returns `true` if `{state_dir}/tasks/*.state.json` files exist.
pub fn has_legacy_state(state_dir: &Path) -> bool {
    let tasks_dir = state_dir.join("tasks");
    if !tasks_dir.is_dir() {
        return false;
    }

    match fs::read_dir(&tasks_dir) {
        Ok(entries) => entries
            .flatten()
            .any(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(".state.json"))
                    .unwrap_or(false)
            }),
        Err(_) => false,
    }
}

/// Check if a directory contains any `.md` files.
fn has_md_files(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .any(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("md")
            }),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_migrate_runtime_state_basic() {
        let conn = open_memory().unwrap();
        let state_dir = TempDir::new().unwrap();

        let tasks_dir = state_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        write_file(
            &tasks_dir,
            "fn-1-test.1.state.json",
            r#"{
                "assignee": "worker-1",
                "claimed_at": "2026-01-01T00:00:00Z",
                "duration_seconds": 120,
                "baseline_rev": "abc123"
            }"#,
        );

        let result = migrate_runtime_state(&conn, state_dir.path()).unwrap();
        assert_eq!(result.states_migrated, 1);
        assert_eq!(result.files_skipped, 0);

        // Verify data in DB.
        let assignee: String = conn
            .query_row(
                "SELECT assignee FROM runtime_state WHERE task_id = 'fn-1-test.1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assignee, "worker-1");

        let duration: i64 = conn
            .query_row(
                "SELECT duration_secs FROM runtime_state WHERE task_id = 'fn-1-test.1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(duration, 120);
    }

    #[test]
    fn test_migrate_runtime_state_invalid_json() {
        let conn = open_memory().unwrap();
        let state_dir = TempDir::new().unwrap();

        let tasks_dir = state_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        write_file(&tasks_dir, "fn-1-test.1.state.json", "not json");

        let result = migrate_runtime_state(&conn, state_dir.path()).unwrap();
        assert_eq!(result.states_migrated, 0);
        assert_eq!(result.files_skipped, 1);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_migrate_runtime_state_idempotent() {
        let conn = open_memory().unwrap();
        let state_dir = TempDir::new().unwrap();

        let tasks_dir = state_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        write_file(
            &tasks_dir,
            "fn-1-test.1.state.json",
            r#"{"assignee": "worker-1"}"#,
        );

        let r1 = migrate_runtime_state(&conn, state_dir.path()).unwrap();
        let r2 = migrate_runtime_state(&conn, state_dir.path()).unwrap();
        assert_eq!(r1.states_migrated, 1);
        assert_eq!(r2.states_migrated, 1);

        // Only one row in DB.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM runtime_state", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_migrate_runtime_state_no_state_dir() {
        let conn = open_memory().unwrap();
        let tmp = TempDir::new().unwrap();

        // No tasks/ subdirectory.
        let result = migrate_runtime_state(&conn, tmp.path()).unwrap();
        assert_eq!(result.states_migrated, 0);
    }

    #[test]
    fn test_needs_reindex_no_db_with_md() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();
        let db_path = tmp.path().join("nonexistent.db");

        // Create .flow/epics/ with an MD file.
        let epics_dir = flow_dir.join("epics");
        fs::create_dir_all(&epics_dir).unwrap();
        write_file(&epics_dir, "fn-1-test.md", "---\nid: test\n---\n");

        assert!(needs_reindex(flow_dir, &db_path));
    }

    #[test]
    fn test_needs_reindex_db_exists() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        // Create a fake DB file.
        let db_path = tmp.path().join("flowctl.db");
        write_file(tmp.path(), "flowctl.db", "");

        // Even with MD files, should return false since DB exists.
        let epics_dir = flow_dir.join("epics");
        fs::create_dir_all(&epics_dir).unwrap();
        write_file(&epics_dir, "fn-1-test.md", "---\nid: test\n---\n");

        assert!(!needs_reindex(flow_dir, &db_path));
    }

    #[test]
    fn test_needs_reindex_no_md_files() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();
        let db_path = tmp.path().join("nonexistent.db");

        // Empty directories.
        fs::create_dir_all(flow_dir.join("epics")).unwrap();
        fs::create_dir_all(flow_dir.join("tasks")).unwrap();

        assert!(!needs_reindex(flow_dir, &db_path));
    }

    #[test]
    fn test_has_legacy_state() {
        let tmp = TempDir::new().unwrap();

        // No tasks dir.
        assert!(!has_legacy_state(tmp.path()));

        // Empty tasks dir.
        let tasks_dir = tmp.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();
        assert!(!has_legacy_state(tmp.path()));

        // With a state file.
        write_file(&tasks_dir, "fn-1-test.1.state.json", "{}");
        assert!(has_legacy_state(tmp.path()));
    }

    #[test]
    fn test_migrate_skips_non_task_ids() {
        let conn = open_memory().unwrap();
        let state_dir = TempDir::new().unwrap();

        let tasks_dir = state_dir.path().join("tasks");
        fs::create_dir_all(&tasks_dir).unwrap();

        // Not a valid task ID (no dot-number suffix).
        write_file(&tasks_dir, "notes.state.json", r#"{"assignee": "x"}"#);

        let result = migrate_runtime_state(&conn, state_dir.path()).unwrap();
        assert_eq!(result.states_migrated, 0);
        assert_eq!(result.files_skipped, 1);
    }
}
