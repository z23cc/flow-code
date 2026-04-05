//! Workflow commands: ready, next, start, done, block, restart, queue,
//! worker-phase next/done.

mod lifecycle;
mod phase;
mod scheduling;

// Re-export all public items so callers see the same API.
pub use lifecycle::{cmd_block, cmd_done, cmd_fail, cmd_restart, cmd_start};
pub use phase::{dispatch_worker_phase, WorkerPhaseCmd};
pub use scheduling::{cmd_next, cmd_queue, cmd_ready};

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::output::error_exit;

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_task_id, parse_id};
use flowctl_core::types::{
    Epic, RuntimeState, Task, EPICS_DIR, TASKS_DIR,
};

use super::helpers::{get_flow_dir, resolve_actor};

// ── Helpers ─────────────────────────────────────────────────────────

/// Ensure .flow/ exists, error_exit if not.
pub(crate) fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Try to open a DB connection.
pub(crate) fn try_open_db() -> Option<crate::commands::db_shim::Connection> {
    let cwd = env::current_dir().ok()?;
    crate::commands::db_shim::open(&cwd).ok()
}

/// Try to open a libSQL async DB connection (for service-layer calls).
pub(crate) fn try_open_lsql_conn() -> Option<libsql::Connection> {
    let cwd = env::current_dir().ok()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    rt.block_on(async {
        let db = flowctl_db_lsql::open_async(&cwd).await.ok()?;
        db.connect().ok()
    })
}

/// Block the current thread on a future (for invoking async service calls
/// from sync CLI code).
pub(crate) fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");
    rt.block_on(fut)
}

/// Load a single epic from Markdown frontmatter.
fn load_epic_md(flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !epic_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&epic_path).ok()?;
    frontmatter::parse_frontmatter::<Epic>(&content).ok()
}

/// Load all tasks for an epic, trying DB first then Markdown.
pub(crate) fn load_tasks_for_epic(flow_dir: &Path, epic_id: &str) -> HashMap<String, Task> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Ok(tasks) = task_repo.list_by_epic(epic_id) {
            if !tasks.is_empty() {
                let mut map = HashMap::new();
                for task in tasks {
                    map.insert(task.id.clone(), task);
                }
                return map;
            }
        }
    }

    // Fall back to Markdown scanning
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if !tasks_dir.is_dir() {
        return HashMap::new();
    }

    let mut map = HashMap::new();
    if let Ok(entries) = fs::read_dir(&tasks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !is_task_id(stem) {
                continue;
            }
            if let Ok(eid) = epic_id_from_task(stem) {
                if eid != epic_id {
                    continue;
                }
            } else {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(task) = frontmatter::parse_frontmatter::<Task>(&content) {
                    map.insert(task.id.clone(), task);
                }
            }
        }
    }
    map
}

/// Load an epic, trying DB first then Markdown.
pub(crate) fn load_epic(flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(epic_id) {
            return Some(epic);
        }
    }
    load_epic_md(flow_dir, epic_id)
}

/// Get runtime state for a task.
pub(crate) fn get_runtime(task_id: &str) -> Option<RuntimeState> {
    let conn = try_open_db()?;
    let repo = crate::commands::db_shim::RuntimeRepo::new(&conn);
    repo.get(task_id).ok().flatten()
}

/// Sort key for tasks: (priority, task_num, title).
pub(crate) fn task_sort_key(task: &Task) -> (u32, u32, String) {
    let parsed = parse_id(&task.id).ok();
    (
        task.sort_priority(),
        parsed.and_then(|p| p.task).unwrap_or(0),
        task.title.clone(),
    )
}

/// Scan all epic .md files in the epics directory, return their IDs sorted.
pub(crate) fn scan_epic_ids(flow_dir: &Path) -> Vec<String> {
    let epics_dir = flow_dir.join(EPICS_DIR);
    if !epics_dir.is_dir() {
        return Vec::new();
    }

    let epic_re = Regex::new(
        r"^fn-(\d+)(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?\.md$",
    )
    .unwrap();

    let mut ids = Vec::new();
    if let Ok(entries) = fs::read_dir(&epics_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name();
            let name = fname.to_string_lossy();
            if epic_re.is_match(&name) {
                let stem = name.trim_end_matches(".md");
                ids.push(stem.to_string());
            }
        }
    }
    ids.sort_by_key(|id| parse_id(id).map(|p| p.epic).unwrap_or(0));
    ids
}
