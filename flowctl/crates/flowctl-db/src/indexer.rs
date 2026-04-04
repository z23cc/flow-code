//! Reindex engine: scans `.flow/` Markdown files and rebuilds SQLite index tables.
//!
//! The reindex process:
//! 1. Acquires an exclusive file lock on the database to prevent concurrent reindex
//! 2. Disables triggers during bulk import
//! 3. Clears all indexed tables (epics, tasks, task_deps, epic_deps, file_ownership)
//! 4. Scans `.flow/epics/*.md` and `.flow/tasks/*.md`
//! 5. Parses YAML frontmatter via `flowctl_core::frontmatter::parse()`
//! 6. INSERTs into SQLite index tables
//! 7. Migrates Python runtime state from `flow-state/tasks/*.state.json`
//! 8. Re-enables triggers
//!
//! The operation is idempotent: running twice produces the same result.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use tracing::{info, warn};

use flowctl_core::frontmatter;
use flowctl_core::id::{is_epic_id, is_task_id};
use flowctl_core::types::{Epic, Task};

use crate::error::DbError;
use crate::repo::{EpicRepo, TaskRepo};

/// Result of a reindex operation.
#[derive(Debug, Default)]
pub struct ReindexResult {
    /// Number of epics indexed.
    pub epics_indexed: usize,
    /// Number of tasks indexed.
    pub tasks_indexed: usize,
    /// Number of files skipped (invalid frontmatter, non-task files, etc.).
    pub files_skipped: usize,
    /// Number of runtime state files migrated.
    pub runtime_states_migrated: usize,
    /// Warnings collected during indexing.
    pub warnings: Vec<String>,
}

/// Perform a full reindex of `.flow/` Markdown files into SQLite.
///
/// This is the main entry point for `flowctl reindex`. It acquires an
/// exclusive lock, clears indexed tables, scans files, and rebuilds.
///
/// # Arguments
/// * `conn` - Open database connection (with migrations already applied)
/// * `flow_dir` - Path to the `.flow/` directory
/// * `state_dir` - Path to the state directory (for runtime state migration)
pub fn reindex(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: Option<&Path>,
) -> Result<ReindexResult, DbError> {
    let mut result = ReindexResult::default();

    // Use a transaction for atomicity.
    conn.execute_batch("BEGIN EXCLUSIVE")?;

    let outcome = reindex_inner(conn, flow_dir, state_dir, &mut result);

    match outcome {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            info!(
                epics = result.epics_indexed,
                tasks = result.tasks_indexed,
                skipped = result.files_skipped,
                runtime = result.runtime_states_migrated,
                "reindex complete"
            );
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Inner reindex logic, separated for transaction management.
fn reindex_inner(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: Option<&Path>,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    // Step 1: Disable triggers during bulk import.
    disable_triggers(conn)?;

    // Step 2: Clear all indexed tables (order matters for FK constraints).
    clear_indexed_tables(conn)?;

    // Step 3: Scan and index epics.
    let epics_dir = flow_dir.join("epics");
    let indexed_epics = if epics_dir.is_dir() {
        index_epics(conn, &epics_dir, result)?
    } else {
        HashMap::new()
    };

    // Step 4: Scan and index tasks.
    let tasks_dir = flow_dir.join("tasks");
    if tasks_dir.is_dir() {
        index_tasks(conn, &tasks_dir, &indexed_epics, result)?;
    }

    // Step 5: Migrate Python runtime state files if present.
    if let Some(sd) = state_dir {
        migrate_runtime_state(conn, sd, result)?;
    }

    // Step 6: Re-enable triggers.
    enable_triggers(conn)?;

    Ok(())
}

/// Disable auto-aggregation triggers during bulk import.
fn disable_triggers(conn: &Connection) -> Result<(), DbError> {
    // Drop the trigger temporarily. We recreate it after import.
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS trg_daily_rollup;"
    )?;
    Ok(())
}

/// Re-enable auto-aggregation triggers after bulk import.
fn enable_triggers(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS trg_daily_rollup AFTER INSERT ON events
         WHEN NEW.event_type IN ('task_completed', 'task_failed', 'task_started')
         BEGIN
             INSERT INTO daily_rollup (day, epic_id, tasks_completed, tasks_failed, tasks_started)
             VALUES (DATE(NEW.timestamp), NEW.epic_id,
                     CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
                     CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
                     CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END)
             ON CONFLICT(day, epic_id) DO UPDATE SET
                 tasks_completed = tasks_completed +
                     CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
                 tasks_failed = tasks_failed +
                     CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
                 tasks_started = tasks_started +
                     CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END;
         END;"
    )?;
    Ok(())
}

/// Clear all indexed (rebuildable) tables.
fn clear_indexed_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "DELETE FROM file_ownership;
         DELETE FROM task_deps;
         DELETE FROM epic_deps;
         DELETE FROM tasks;
         DELETE FROM epics;",
    )?;
    Ok(())
}

/// Scan `.flow/epics/*.md`, parse frontmatter, insert into DB.
/// Returns a map of epic ID -> file path for duplicate detection.
fn index_epics(
    conn: &Connection,
    epics_dir: &Path,
    result: &mut ReindexResult,
) -> Result<HashMap<String, PathBuf>, DbError> {
    let repo = EpicRepo::new(conn);
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    let entries = read_md_files(epics_dir);

    for path in entries {
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

        // Validate filename stem is a valid epic ID.
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_epic_id(stem) {
            let msg = format!("skipping non-epic file: {}", path.display());
            warn!("{}", msg);
            result.warnings.push(msg);
            result.files_skipped += 1;
            continue;
        }

        let mut epic: Epic = match frontmatter::parse_frontmatter(&content) {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("invalid frontmatter in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        // Check for duplicate IDs.
        if let Some(prev_path) = seen.get(&epic.id) {
            return Err(DbError::Constraint(format!(
                "duplicate epic ID '{}' in {} and {}",
                epic.id,
                prev_path.display(),
                path.display()
            )));
        }

        // Set the file_path to the relative path within .flow/.
        epic.file_path = Some(format!("epics/{}", path.file_name().unwrap().to_string_lossy()));

        repo.upsert(&epic)?;
        seen.insert(epic.id.clone(), path.clone());
        result.epics_indexed += 1;
    }

    Ok(seen)
}

/// Scan `.flow/tasks/*.md`, parse frontmatter, insert into DB.
fn index_tasks(
    conn: &Connection,
    tasks_dir: &Path,
    indexed_epics: &HashMap<String, PathBuf>,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    let task_repo = TaskRepo::new(conn);
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    let entries = read_md_files(tasks_dir);

    for path in entries {
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

        // Validate filename stem is a valid task ID.
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_task_id(stem) {
            let msg = format!("skipping non-task file: {}", path.display());
            warn!("{}", msg);
            result.warnings.push(msg);
            result.files_skipped += 1;
            continue;
        }

        let mut task: Task = match frontmatter::parse_frontmatter(&content) {
            Ok(t) => t,
            Err(e) => {
                let msg = format!("invalid frontmatter in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        // Check for duplicate IDs.
        if let Some(prev_path) = seen.get(&task.id) {
            return Err(DbError::Constraint(format!(
                "duplicate task ID '{}' in {} and {}",
                task.id,
                prev_path.display(),
                path.display()
            )));
        }

        // Warn about orphan tasks (referencing non-existent epic) but still index them.
        if !indexed_epics.contains_key(&task.epic) {
            let msg = format!(
                "orphan task '{}' references non-existent epic '{}' (indexing anyway)",
                task.id, task.epic
            );
            warn!("{}", msg);
            result.warnings.push(msg);

            // Insert a placeholder epic so FK constraint is satisfied.
            insert_placeholder_epic(conn, &task.epic)?;
        }

        // Set the file_path to the relative path within .flow/.
        task.file_path = Some(format!("tasks/{}", path.file_name().unwrap().to_string_lossy()));

        task_repo.upsert(&task)?;
        seen.insert(task.id.clone(), path.clone());
        result.tasks_indexed += 1;
    }

    Ok(())
}

/// Insert a minimal placeholder epic for orphan task FK satisfaction.
fn insert_placeholder_epic(conn: &Connection, epic_id: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO epics (id, title, status, file_path, created_at, updated_at)
         VALUES (?1, ?2, 'open', '', datetime('now'), datetime('now'))",
        params![epic_id, format!("[placeholder] {}", epic_id)],
    )?;
    Ok(())
}

/// Migrate Python runtime state files from `flow-state/tasks/*.state.json` into
/// the `runtime_state` table.
///
/// Each JSON file has the structure matching `RuntimeState` fields.
/// This migration is idempotent (INSERT OR REPLACE).
fn migrate_runtime_state(
    conn: &Connection,
    state_dir: &Path,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    let tasks_state_dir = state_dir.join("tasks");
    if !tasks_state_dir.is_dir() {
        return Ok(());
    }

    let entries = match fs::read_dir(&tasks_state_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
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

        // Extract task ID from filename: "fn-1-test.1.state.json" -> "fn-1-test.1"
        let task_id = name.trim_end_matches(".state.json");
        if !is_task_id(task_id) {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read runtime state {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                continue;
            }
        };

        let state: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("invalid JSON in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
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
                state.get("duration_secs").or_else(|| state.get("duration_seconds")).and_then(|v| v.as_i64()),
                state.get("blocked_reason").and_then(|v| v.as_str()),
                state.get("baseline_rev").and_then(|v| v.as_str()),
                state.get("final_rev").and_then(|v| v.as_str()),
            ],
        )?;

        result.runtime_states_migrated += 1;
    }

    Ok(())
}

/// Read all `.md` files in a directory, sorted by name for deterministic ordering.
fn read_md_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a temporary .flow/ directory with test fixtures.
    fn setup_flow_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let flow = tmp.path();
        fs::create_dir_all(flow.join("epics")).unwrap();
        fs::create_dir_all(flow.join("tasks")).unwrap();
        tmp
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    fn epic_md(id: &str, title: &str) -> String {
        format!(
            r#"---
schema_version: 1
id: {id}
title: {title}
status: open
plan_review: unknown
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
---
## Description
Test epic.
"#
        )
    }

    fn task_md(id: &str, epic: &str, title: &str, deps: &[&str], files: &[&str]) -> String {
        let deps_yaml = if deps.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = deps.iter().map(|d| format!("  - {d}")).collect();
            format!("depends_on:\n{}\n", items.join("\n"))
        };
        let files_yaml = if files.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = files.iter().map(|f| format!("  - {f}")).collect();
            format!("files:\n{}\n", items.join("\n"))
        };
        format!(
            r#"---
schema_version: 1
id: {id}
epic: {epic}
title: {title}
status: todo
domain: general
{deps_yaml}{files_yaml}created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
---
## Description
Test task.
"#
        )
    }

    #[test]
    fn test_reindex_basic() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Test Epic"));
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "Task One", &[], &["src/main.rs"]),
        );
        write_file(
            &flow.join("tasks"),
            "fn-1-test.2.md",
            &task_md("fn-1-test.2", "fn-1-test", "Task Two", &["fn-1-test.1"], &[]),
        );

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.epics_indexed, 1);
        assert_eq!(result.tasks_indexed, 2);
        assert_eq!(result.files_skipped, 0);

        // Verify data in DB.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM epics", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM task_deps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_ownership", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_reindex_idempotent() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Test"));
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "Task", &[], &[]),
        );

        let r1 = reindex(&conn, flow, None).unwrap();
        let r2 = reindex(&conn, flow, None).unwrap();

        assert_eq!(r1.epics_indexed, r2.epics_indexed);
        assert_eq!(r1.tasks_indexed, r2.tasks_indexed);

        // Should still have exactly 1 epic and 1 task.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM epics", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_reindex_invalid_frontmatter_skipped() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Good Epic"));
        write_file(&flow.join("epics"), "fn-2-bad.md", "not valid frontmatter at all");

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.epics_indexed, 1);
        assert_eq!(result.files_skipped, 1);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_reindex_non_task_files_skipped() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Epic"));
        // A .md file with a non-task filename in tasks dir.
        write_file(&flow.join("tasks"), "notes.md", "just some notes");

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.epics_indexed, 1);
        assert_eq!(result.tasks_indexed, 0);
        assert_eq!(result.files_skipped, 1);
    }

    #[test]
    fn test_reindex_orphan_task_warns() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        // No epic file, but a task referencing it.
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "Orphan Task", &[], &[]),
        );

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.tasks_indexed, 1);
        assert!(result.warnings.iter().any(|w| w.contains("orphan")));

        // Placeholder epic should exist.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM epics WHERE id = 'fn-1-test'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_reindex_duplicate_epic_errors() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        // Two files with the same epic ID.
        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "First"));
        write_file(&flow.join("epics"), "fn-1-test-copy.md", &epic_md("fn-1-test", "Second"));

        let result = reindex(&conn, flow, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate epic ID"), "Got: {err}");
    }

    #[test]
    fn test_reindex_duplicate_task_errors() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Epic"));
        write_file(&flow.join("epics"), "fn-2-other.md", &epic_md("fn-2-other", "Other"));
        // Two files with different valid task-ID filenames but same ID in frontmatter.
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "First", &[], &[]),
        );
        write_file(
            &flow.join("tasks"),
            "fn-2-other.1.md",
            &task_md("fn-1-test.1", "fn-2-other", "Second (dup ID)", &[], &[]),
        );

        let result = reindex(&conn, flow, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate task ID"), "Got: {err}");
    }

    #[test]
    fn test_reindex_file_ownership() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Epic"));
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "Task", &[], &["src/a.rs", "src/b.rs"]),
        );

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.tasks_indexed, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_ownership", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_reindex_empty_dirs() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.epics_indexed, 0);
        assert_eq!(result.tasks_indexed, 0);
        assert_eq!(result.files_skipped, 0);
    }

    #[test]
    fn test_reindex_missing_dirs() {
        let conn = open_memory().unwrap();
        let tmp = TempDir::new().unwrap();
        // No epics/ or tasks/ subdirectories.
        let result = reindex(&conn, tmp.path(), None).unwrap();
        assert_eq!(result.epics_indexed, 0);
        assert_eq!(result.tasks_indexed, 0);
    }

    #[test]
    fn test_reindex_triggers_restored() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Epic"));
        reindex(&conn, flow, None).unwrap();

        // Verify trigger exists after reindex.
        let trigger_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name='trg_daily_rollup'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(trigger_count, 1);
    }

    #[test]
    fn test_migrate_runtime_state() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();
        let state_dir = TempDir::new().unwrap();

        // Create epic and task first.
        write_file(&flow.join("epics"), "fn-1-test.md", &epic_md("fn-1-test", "Epic"));
        write_file(
            &flow.join("tasks"),
            "fn-1-test.1.md",
            &task_md("fn-1-test.1", "fn-1-test", "Task", &[], &[]),
        );

        // Create runtime state file.
        let tasks_state = state_dir.path().join("tasks");
        fs::create_dir_all(&tasks_state).unwrap();
        write_file(
            &tasks_state,
            "fn-1-test.1.state.json",
            r#"{"assignee": "worker-1", "claimed_at": "2026-01-01T00:00:00Z", "duration_seconds": 120}"#,
        );

        let result = reindex(&conn, flow, Some(state_dir.path())).unwrap();
        assert_eq!(result.runtime_states_migrated, 1);

        let assignee: String = conn
            .query_row(
                "SELECT assignee FROM runtime_state WHERE task_id = 'fn-1-test.1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assignee, "worker-1");
    }

    #[test]
    fn test_reindex_epic_deps() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();
        let flow = tmp.path();

        write_file(&flow.join("epics"), "fn-1-base.md", &epic_md("fn-1-base", "Base"));
        write_file(
            &flow.join("epics"),
            "fn-2-next.md",
            &format!(
                r#"---
schema_version: 1
id: fn-2-next
title: Next
status: open
plan_review: unknown
depends_on_epics:
  - fn-1-base
created_at: "2026-01-01T00:00:00Z"
updated_at: "2026-01-01T00:00:00Z"
---
## Description
Depends on base.
"#
            ),
        );

        let result = reindex(&conn, flow, None).unwrap();
        assert_eq!(result.epics_indexed, 2);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM epic_deps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
