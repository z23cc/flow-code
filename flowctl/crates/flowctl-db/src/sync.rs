//! Bidirectional Markdown-SQLite sync.
//!
//! **Invariant**: SQLite is updated first (in a transaction), then Markdown
//! frontmatter is written. If the Markdown write fails after SQLite commit,
//! the row is marked `pending_sync` and retried on the next operation.
//!
//! Staleness detection compares the Markdown file's mtime against the
//! SQLite `updated_at` timestamp. If Markdown is newer, the frontmatter
//! is re-parsed and SQLite is refreshed.
//!
//! Concurrent modification is guarded by checking mtime before overwrite.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use tracing::{error, info, warn};

use flowctl_core::frontmatter;
use flowctl_core::types::{Epic, Task};

use crate::error::DbError;
use crate::repo::{EpicRepo, TaskRepo};

/// Sync status for a row whose Markdown write failed after SQLite commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// Both SQLite and Markdown are in sync.
    Synced,
    /// SQLite was updated but Markdown write failed; needs retry.
    PendingSyncMd,
    /// Markdown is newer than SQLite; needs re-read.
    StaleSqlite,
}

/// Write an epic to both SQLite (first) and Markdown (second).
///
/// If the Markdown write fails, the epic is marked `pending_sync` in
/// the `sync_state` table and the error is logged (not propagated).
pub fn write_epic(
    conn: &Connection,
    flow_dir: &Path,
    epic: &Epic,
    body: &str,
) -> Result<SyncStatus, DbError> {
    // Capture mtime BEFORE SQLite transaction for concurrent modification check.
    let file_path = epic_md_path(flow_dir, &epic.id);
    let pre_mtime = file_mtime(&file_path);

    // Step 1: Update SQLite in a transaction.
    let repo = EpicRepo::new(conn);
    repo.upsert(epic)?;

    // Step 2: Write Markdown frontmatter.
    let doc = frontmatter::Document {
        frontmatter: epic.clone(),
        body: body.to_string(),
    };

    match frontmatter::write(&doc) {
        Ok(content) => match write_md_safe(&file_path, &content, pre_mtime) {
            Ok(()) => {
                clear_pending_sync(conn, &epic.id);
                Ok(SyncStatus::Synced)
            }
            Err(e) => {
                error!(id = %epic.id, err = %e, "markdown write failed after sqlite commit");
                mark_pending_sync(conn, &epic.id, "epic");
                Ok(SyncStatus::PendingSyncMd)
            }
        },
        Err(e) => {
            error!(id = %epic.id, err = %e, "frontmatter serialize failed");
            mark_pending_sync(conn, &epic.id, "epic");
            Ok(SyncStatus::PendingSyncMd)
        }
    }
}

/// Write a task to both SQLite (first) and Markdown (second).
///
/// Same guarantees as [`write_epic`].
pub fn write_task(
    conn: &Connection,
    flow_dir: &Path,
    task: &Task,
    body: &str,
) -> Result<SyncStatus, DbError> {
    // Capture mtime BEFORE SQLite transaction for concurrent modification check.
    let file_path = task_md_path(flow_dir, &task.id);
    let pre_mtime = file_mtime(&file_path);

    // Step 1: Update SQLite.
    let repo = TaskRepo::new(conn);
    repo.upsert(task)?;

    // Step 2: Write Markdown.
    let doc = frontmatter::Document {
        frontmatter: task.clone(),
        body: body.to_string(),
    };

    match frontmatter::write(&doc) {
        Ok(content) => match write_md_safe(&file_path, &content, pre_mtime) {
            Ok(()) => {
                clear_pending_sync(conn, &task.id);
                Ok(SyncStatus::Synced)
            }
            Err(e) => {
                error!(id = %task.id, err = %e, "markdown write failed after sqlite commit");
                mark_pending_sync(conn, &task.id, "task");
                Ok(SyncStatus::PendingSyncMd)
            }
        },
        Err(e) => {
            error!(id = %task.id, err = %e, "frontmatter serialize failed");
            mark_pending_sync(conn, &task.id, "task");
            Ok(SyncStatus::PendingSyncMd)
        }
    }
}

/// Write a task to SQLite and optionally write a legacy JSON state file.
///
/// When `--legacy-json` is active, also writes a Python-compatible
/// `.state.json` file alongside the Markdown+SQLite sync.
pub fn write_task_with_legacy(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: &Path,
    task: &Task,
    body: &str,
) -> Result<SyncStatus, DbError> {
    let status = write_task(conn, flow_dir, task, body)?;

    // Write legacy JSON for Python compatibility.
    if let Err(e) = write_legacy_json(state_dir, task) {
        warn!(id = %task.id, err = %e, "legacy JSON write failed (non-fatal)");
    }

    Ok(status)
}

/// Check staleness: compare Markdown mtime against SQLite `updated_at`.
///
/// Returns `StaleSqlite` if Markdown was modified after the SQLite row,
/// `PendingSyncMd` if there is a pending sync entry, or `Synced`.
pub fn check_staleness(
    conn: &Connection,
    flow_dir: &Path,
    id: &str,
    entity: &str,
) -> SyncStatus {
    // Check pending_sync table first.
    if is_pending_sync(conn, id) {
        return SyncStatus::PendingSyncMd;
    }

    let (md_path, db_updated_at) = match entity {
        "epic" => {
            let path = epic_md_path(flow_dir, id);
            let updated = get_db_updated_at(conn, "epics", id);
            (path, updated)
        }
        "task" => {
            let path = task_md_path(flow_dir, id);
            let updated = get_db_updated_at(conn, "tasks", id);
            (path, updated)
        }
        _ => return SyncStatus::Synced,
    };

    let db_updated = match db_updated_at {
        Some(dt) => dt,
        None => return SyncStatus::Synced, // not in DB yet
    };

    match file_mtime(&md_path) {
        Some(mtime) => {
            // Allow 2-second tolerance: file writes naturally have a
            // slightly newer mtime than the `updated_at` stored in SQLite,
            // because the struct is created before the file is written.
            let tolerance = chrono::Duration::seconds(2);
            if mtime > db_updated + tolerance {
                SyncStatus::StaleSqlite
            } else {
                SyncStatus::Synced
            }
        }
        None => SyncStatus::Synced, // file doesn't exist
    }
}

/// Refresh SQLite from Markdown if stale.
///
/// Reads the Markdown file, parses frontmatter, and upserts into SQLite.
pub fn refresh_if_stale(
    conn: &Connection,
    flow_dir: &Path,
    id: &str,
    entity: &str,
) -> Result<bool, DbError> {
    let status = check_staleness(conn, flow_dir, id, entity);

    match status {
        SyncStatus::StaleSqlite => {
            info!(id, entity, "refreshing stale sqlite from markdown");
            match entity {
                "epic" => {
                    let path = epic_md_path(flow_dir, id);
                    let content = fs::read_to_string(&path).map_err(|e| {
                        DbError::StateDir(format!("failed to read {}: {e}", path.display()))
                    })?;
                    let epic: Epic = flowctl_core::frontmatter::parse_frontmatter(&content)
                        .map_err(|e| DbError::Migration(format!("parse error: {e}")))?;
                    EpicRepo::new(conn).upsert(&epic)?;
                }
                "task" => {
                    let path = task_md_path(flow_dir, id);
                    let content = fs::read_to_string(&path).map_err(|e| {
                        DbError::StateDir(format!("failed to read {}: {e}", path.display()))
                    })?;
                    let task: Task = flowctl_core::frontmatter::parse_frontmatter(&content)
                        .map_err(|e| DbError::Migration(format!("parse error: {e}")))?;
                    TaskRepo::new(conn).upsert(&task)?;
                }
                _ => {}
            }
            Ok(true)
        }
        SyncStatus::PendingSyncMd => {
            // Try to retry the pending Markdown write.
            // We'd need the body for this, so just log and skip.
            warn!(id, "pending_sync entry exists; re-run write to resolve");
            Ok(false)
        }
        SyncStatus::Synced => Ok(false),
    }
}

/// Retry all pending sync entries.
///
/// For each pending entry, re-read from SQLite and re-write the Markdown.
/// Returns the number of entries successfully resolved.
pub fn retry_pending(conn: &Connection, flow_dir: &Path) -> Result<usize, DbError> {
    let pending = list_pending_sync(conn)?;
    let mut resolved = 0;

    for (id, entity) in &pending {
        let result = match entity.as_str() {
            "epic" => {
                let repo = EpicRepo::new(conn);
                match repo.get(id) {
                    Ok(epic) => {
                        let path = epic_md_path(flow_dir, id);
                        let body = read_body(&path);
                        let doc = frontmatter::Document {
                            frontmatter: epic,
                            body,
                        };
                        match frontmatter::write(&doc) {
                            Ok(content) => write_md_safe(&path, &content, None),
                            Err(e) => Err(std::io::Error::other(
                                e.to_string(),
                            )),
                        }
                    }
                    Err(_) => continue,
                }
            }
            "task" => {
                let repo = TaskRepo::new(conn);
                match repo.get(id) {
                    Ok(task) => {
                        let path = task_md_path(flow_dir, id);
                        let body = read_body(&path);
                        let doc = frontmatter::Document {
                            frontmatter: task,
                            body,
                        };
                        match frontmatter::write(&doc) {
                            Ok(content) => write_md_safe(&path, &content, None),
                            Err(e) => Err(std::io::Error::other(
                                e.to_string(),
                            )),
                        }
                    }
                    Err(_) => continue,
                }
            }
            _ => continue,
        };

        match result {
            Ok(()) => {
                clear_pending_sync(conn, id);
                resolved += 1;
                info!(id, entity, "resolved pending sync");
            }
            Err(e) => {
                warn!(id, entity, err = %e, "pending sync retry still failing");
            }
        }
    }

    Ok(resolved)
}

// ── Internal helpers ───────────────────────────────────────────────

/// Construct the Markdown path for an epic.
fn epic_md_path(flow_dir: &Path, id: &str) -> std::path::PathBuf {
    flow_dir.join("epics").join(format!("{id}.md"))
}

/// Construct the Markdown path for a task.
fn task_md_path(flow_dir: &Path, id: &str) -> std::path::PathBuf {
    flow_dir.join("tasks").join(format!("{id}.md"))
}

/// Write Markdown content to a file with concurrent modification check.
///
/// Reads the file's current mtime before writing. If another process
/// modified the file between our read and write, we detect it.
fn write_md_safe(
    path: &Path,
    content: &str,
    pre_mtime: Option<DateTime<Utc>>,
) -> Result<(), std::io::Error> {
    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Check for concurrent modification: if mtime changed since we
    // captured it (before the SQLite transaction), another process
    // may have modified the file.
    if let Some(pre) = pre_mtime {
        if let Some(current) = file_mtime(path) {
            if current != pre {
                return Err(std::io::Error::other(
                    format!(
                        "concurrent modification detected on {}",
                        path.display()
                    ),
                ));
            }
        }
    }

    // Write to temp file then rename for atomicity.
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

/// Get file modification time as DateTime<Utc>.
fn file_mtime(path: &Path) -> Option<DateTime<Utc>> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(system_time_to_utc)
}

/// Convert SystemTime to DateTime<Utc>.
fn system_time_to_utc(st: SystemTime) -> DateTime<Utc> {
    let duration = st
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
        .unwrap_or_else(Utc::now)
}

/// Get the `updated_at` value from a SQLite table.
fn get_db_updated_at(conn: &Connection, table: &str, id: &str) -> Option<DateTime<Utc>> {
    let sql = format!("SELECT updated_at FROM {table} WHERE id = ?1");
    conn.query_row(&sql, params![id], |row| row.get::<_, String>(0))
        .ok()
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Read the body (everything after frontmatter) from an existing Markdown file.
fn read_body(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(content) => {
            match frontmatter::parse::<serde_json::Value>(&content) {
                Ok(doc) => doc.body,
                Err(_) => String::new(),
            }
        }
        Err(_) => String::new(),
    }
}

/// Write a Python-compatible `.state.json` file for legacy support.
fn write_legacy_json(state_dir: &Path, task: &Task) -> Result<(), std::io::Error> {
    let tasks_dir = state_dir.join("tasks");
    fs::create_dir_all(&tasks_dir)?;

    let json = serde_json::json!({
        "status": task.status.to_string(),
        "updated_at": task.updated_at.to_rfc3339(),
    });

    let path = tasks_dir.join(format!("{}.state.json", task.id));
    fs::write(path, serde_json::to_string_pretty(&json).unwrap_or_default())
}

// ── Pending sync table helpers ─────────────────────────────────────

/// Ensure the `sync_state` table exists (created alongside migrations).
fn ensure_sync_table(conn: &Connection) {
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sync_state (
            id     TEXT PRIMARY KEY,
            entity TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending_sync_md',
            failed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        )"
    );
}

/// Mark an entity as pending sync.
fn mark_pending_sync(conn: &Connection, id: &str, entity: &str) {
    ensure_sync_table(conn);
    let _ = conn.execute(
        "INSERT INTO sync_state (id, entity) VALUES (?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
             failed_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        params![id, entity],
    );
}

/// Clear a pending sync entry (after successful Markdown write).
fn clear_pending_sync(conn: &Connection, id: &str) {
    ensure_sync_table(conn);
    let _ = conn.execute("DELETE FROM sync_state WHERE id = ?1", params![id]);
}

/// Check if an entity has a pending sync entry.
fn is_pending_sync(conn: &Connection, id: &str) -> bool {
    ensure_sync_table(conn);
    conn.query_row(
        "SELECT COUNT(*) FROM sync_state WHERE id = ?1",
        params![id],
        |row| row.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// List all pending sync entries.
fn list_pending_sync(conn: &Connection) -> Result<Vec<(String, String)>, DbError> {
    ensure_sync_table(conn);
    let mut stmt = conn.prepare("SELECT id, entity FROM sync_state")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;
    use flowctl_core::state_machine::Status;
    use flowctl_core::types::{Domain, EpicStatus, ReviewStatus};
    use tempfile::TempDir;

    fn test_epic(id: &str) -> Epic {
        Epic {
            schema_version: 1,
            id: id.to_string(),
            title: "Test Epic".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some(format!("epics/{id}.md")),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn test_task(id: &str, epic: &str) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: epic.to_string(),
            title: "Test Task".to_string(),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some(format!("tasks/{id}.md")),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn setup_flow_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("epics")).unwrap();
        fs::create_dir_all(tmp.path().join("tasks")).unwrap();
        tmp
    }

    #[test]
    fn test_write_epic_syncs_both() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        let epic = test_epic("fn-1-test");
        let status = write_epic(&conn, tmp.path(), &epic, "## Description\nTest.\n").unwrap();
        assert_eq!(status, SyncStatus::Synced);

        // Verify SQLite.
        let repo = EpicRepo::new(&conn);
        let loaded = repo.get("fn-1-test").unwrap();
        assert_eq!(loaded.title, "Test Epic");

        // Verify Markdown file exists.
        let md_path = tmp.path().join("epics/fn-1-test.md");
        assert!(md_path.exists());
        let content = fs::read_to_string(&md_path).unwrap();
        assert!(content.contains("fn-1-test"));
    }

    #[test]
    fn test_write_task_syncs_both() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        // Need epic first (FK).
        let epic = test_epic("fn-1-test");
        write_epic(&conn, tmp.path(), &epic, "").unwrap();

        let task = test_task("fn-1-test.1", "fn-1-test");
        let status = write_task(&conn, tmp.path(), &task, "## Description\nDo thing.\n").unwrap();
        assert_eq!(status, SyncStatus::Synced);

        // Verify SQLite.
        let repo = TaskRepo::new(&conn);
        let loaded = repo.get("fn-1-test.1").unwrap();
        assert_eq!(loaded.title, "Test Task");

        // Verify Markdown.
        let md_path = tmp.path().join("tasks/fn-1-test.1.md");
        assert!(md_path.exists());
    }

    #[test]
    fn test_staleness_detection_synced() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        let epic = test_epic("fn-1-test");
        write_epic(&conn, tmp.path(), &epic, "").unwrap();

        let status = check_staleness(&conn, tmp.path(), "fn-1-test", "epic");
        assert_eq!(status, SyncStatus::Synced);
    }

    #[test]
    fn test_staleness_detection_stale() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        let epic = test_epic("fn-1-test");
        write_epic(&conn, tmp.path(), &epic, "").unwrap();

        // Manually update the Markdown file to make it "newer".
        // We need to set SQLite updated_at to an older time.
        conn.execute(
            "UPDATE epics SET updated_at = '2020-01-01T00:00:00Z' WHERE id = 'fn-1-test'",
            [],
        )
        .unwrap();

        // Touch the file to update its mtime.
        let md_path = tmp.path().join("epics/fn-1-test.md");
        let content = fs::read_to_string(&md_path).unwrap();
        fs::write(&md_path, content).unwrap();

        let status = check_staleness(&conn, tmp.path(), "fn-1-test", "epic");
        assert_eq!(status, SyncStatus::StaleSqlite);
    }

    #[test]
    fn test_refresh_if_stale() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        let epic = test_epic("fn-1-test");
        write_epic(&conn, tmp.path(), &epic, "## Body\n").unwrap();

        // Make SQLite stale by backdating updated_at.
        conn.execute(
            "UPDATE epics SET updated_at = '2020-01-01T00:00:00Z' WHERE id = 'fn-1-test'",
            [],
        )
        .unwrap();
        // Touch file.
        let md_path = tmp.path().join("epics/fn-1-test.md");
        let content = fs::read_to_string(&md_path).unwrap();
        fs::write(&md_path, content).unwrap();

        let refreshed = refresh_if_stale(&conn, tmp.path(), "fn-1-test", "epic").unwrap();
        assert!(refreshed);
    }

    #[test]
    fn test_pending_sync_lifecycle() {
        let conn = open_memory().unwrap();

        assert!(!is_pending_sync(&conn, "fn-1-test"));

        mark_pending_sync(&conn, "fn-1-test", "epic");
        assert!(is_pending_sync(&conn, "fn-1-test"));

        clear_pending_sync(&conn, "fn-1-test");
        assert!(!is_pending_sync(&conn, "fn-1-test"));
    }

    #[test]
    fn test_legacy_json_write() {
        let tmp = TempDir::new().unwrap();
        let task = test_task("fn-1-test.1", "fn-1-test");

        write_legacy_json(tmp.path(), &task).unwrap();

        let json_path = tmp.path().join("tasks/fn-1-test.1.state.json");
        assert!(json_path.exists());

        let content = fs::read_to_string(&json_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["status"], "todo");
    }

    #[test]
    fn test_retry_pending_resolves() {
        let conn = open_memory().unwrap();
        let tmp = setup_flow_dir();

        // Write epic to SQLite only (simulate failed MD write).
        let epic = test_epic("fn-1-test");
        EpicRepo::new(&conn).upsert(&epic).unwrap();
        mark_pending_sync(&conn, "fn-1-test", "epic");

        // Retry should write the MD file.
        let resolved = retry_pending(&conn, tmp.path()).unwrap();
        assert_eq!(resolved, 1);
        assert!(!is_pending_sync(&conn, "fn-1-test"));

        // MD file should now exist.
        let md_path = tmp.path().join("epics/fn-1-test.md");
        assert!(md_path.exists());
    }
}
