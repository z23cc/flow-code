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
use std::path::{Path, PathBuf};

use crate::output::error_exit;

use flowctl_core::id::parse_id;
use flowctl_core::types::{
    Epic, RuntimeState, Task,
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
        let db = flowctl_db::open_async(&cwd).await.ok()?;
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

/// Load all tasks for an epic from DB (sole source of truth).
pub(crate) fn load_tasks_for_epic(_flow_dir: &Path, epic_id: &str) -> HashMap<String, Task> {
    if let Some(conn) = try_open_db() {
        let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Ok(tasks) = task_repo.list_by_epic(epic_id) {
            let mut map = HashMap::new();
            for task in tasks {
                map.insert(task.id.clone(), task);
            }
            return map;
        }
    }
    HashMap::new()
}

/// Load an epic from DB (sole source of truth).
pub(crate) fn load_epic(_flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    let conn = try_open_db()?;
    let repo = crate::commands::db_shim::EpicRepo::new(&conn);
    repo.get(epic_id).ok()
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

/// Get all epic IDs from DB, sorted by epic number.
pub(crate) fn scan_epic_ids(_flow_dir: &Path) -> Vec<String> {
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(epics) = repo.list(None) {
            let mut ids: Vec<String> = epics.into_iter().map(|e| e.id).collect();
            ids.sort_by_key(|id| parse_id(id).map(|p| p.epic).unwrap_or(0));
            return ids;
        }
    }
    Vec::new()
}
