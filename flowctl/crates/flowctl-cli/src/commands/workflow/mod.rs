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

use chrono::Utc;
use regex::Regex;

use crate::output::error_exit;

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_task_id, parse_id};
use flowctl_core::state_machine::Status;
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
pub(crate) fn try_open_db() -> Option<rusqlite::Connection> {
    let cwd = env::current_dir().ok()?;
    flowctl_db::open(&cwd).ok()
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

/// Load a single task from Markdown frontmatter.
fn load_task_md(flow_dir: &Path, task_id: &str) -> Option<Task> {
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if !task_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&task_path).ok()?;
    frontmatter::parse_frontmatter::<Task>(&content).ok()
}

/// Load all tasks for an epic, trying DB first then Markdown.
pub(crate) fn load_tasks_for_epic(flow_dir: &Path, epic_id: &str) -> HashMap<String, Task> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
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
        let repo = flowctl_db::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(epic_id) {
            return Some(epic);
        }
    }
    load_epic_md(flow_dir, epic_id)
}

/// Load a task, trying DB first then Markdown.
pub(crate) fn load_task(flow_dir: &Path, task_id: &str) -> Option<Task> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        if let Ok(task) = repo.get(task_id) {
            return Some(task);
        }
    }
    load_task_md(flow_dir, task_id)
}

/// Get runtime state for a task.
pub(crate) fn get_runtime(task_id: &str) -> Option<RuntimeState> {
    let conn = try_open_db()?;
    let repo = flowctl_db::RuntimeRepo::new(&conn);
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

/// Patch a Markdown section (## heading) with new content.
pub(crate) fn patch_md_section(doc: &str, heading: &str, new_content: &str) -> Option<String> {
    let heading_prefix = format!("{}\n", heading);
    let pos = doc.find(&heading_prefix)?;
    let after_heading = pos + heading_prefix.len();

    // Find the next ## heading or end of document
    let rest = &doc[after_heading..];
    let next_heading = rest.find("\n## ").map(|p| after_heading + p + 1);

    let mut result = String::with_capacity(doc.len());
    result.push_str(&doc[..after_heading]);
    result.push_str(new_content.trim_end());
    result.push('\n');
    if let Some(nh) = next_heading {
        result.push('\n');
        result.push_str(&doc[nh..]);
    }
    Some(result)
}

/// Get a Markdown section content (between ## heading and next ## or EOF).
pub(crate) fn get_md_section(doc: &str, heading: &str) -> String {
    let heading_prefix = format!("{}\n", heading);
    let Some(pos) = doc.find(&heading_prefix) else {
        return String::new();
    };
    let after_heading = pos + heading_prefix.len();
    let rest = &doc[after_heading..];
    let section_end = rest.find("\n## ").unwrap_or(rest.len());
    rest[..section_end].trim().to_string()
}

/// Find all downstream dependents of a task within the same epic.
pub(crate) fn find_dependents(flow_dir: &Path, task_id: &str) -> Vec<String> {
    let epic_id = match epic_id_from_task(task_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(flow_dir, &epic_id);
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![task_id.to_string()];

    while let Some(current) = queue.pop() {
        for (tid, task) in &tasks {
            if visited.contains(tid.as_str()) {
                continue;
            }
            if task.depends_on.contains(&current) {
                visited.insert(tid.clone());
                dependents.push(tid.clone());
                queue.push(tid.clone());
            }
        }
    }

    dependents.sort();
    dependents
}

/// Read max_retries from .flow/config.json (defaults to 0 = no retries).
pub(crate) fn get_max_retries() -> u32 {
    let config_path = get_flow_dir().join("config.json");
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(max) = config.get("max_retries").and_then(|v| v.as_u64()) {
                return max as u32;
            }
        }
    }
    0
}

/// Propagate upstream_failed to all transitive downstream tasks of `failed_id`.
///
/// Updates both SQLite and Markdown for each affected task. Returns the list
/// of task IDs that were marked upstream_failed.
pub(crate) fn propagate_upstream_failure(flow_dir: &Path, failed_id: &str) -> Vec<String> {
    let epic_id = match epic_id_from_task(failed_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(flow_dir, &epic_id);
    let task_list: Vec<Task> = tasks.values().cloned().collect();

    let dag = match flowctl_core::TaskDag::from_tasks(&task_list) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let downstream = dag.propagate_failure(failed_id);
    let mut affected = Vec::new();

    for tid in &downstream {
        let task = match tasks.get(tid) {
            Some(t) => t,
            None => continue,
        };

        // Only propagate to tasks that aren't already in a terminal or failure state.
        if task.status.is_satisfied() || task.status.is_failed() {
            continue;
        }

        // Update SQLite
        if let Some(conn) = try_open_db() {
            let task_repo = flowctl_db::TaskRepo::new(&conn);
            let _ = task_repo.update_status(tid, Status::UpstreamFailed);
        }

        // Update Markdown frontmatter
        let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", tid));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                if let Ok(mut doc) = frontmatter::parse::<Task>(&content) {
                    doc.frontmatter.status = Status::UpstreamFailed;
                    doc.frontmatter.updated_at = Utc::now();
                    if let Ok(new_content) = frontmatter::write(&doc) {
                        let _ = fs::write(&task_path, new_content);
                    }
                }
            }
        }

        affected.push(tid.clone());
    }

    affected
}

/// Handle task failure: check retries, set up_for_retry or failed + propagate.
///
/// Returns `(final_status, upstream_failed_ids)`.
pub(crate) fn handle_task_failure(
    flow_dir: &Path,
    task_id: &str,
    runtime: &Option<RuntimeState>,
) -> (Status, Vec<String>) {
    let max_retries = get_max_retries();
    let current_retry_count = runtime.as_ref().map(|r| r.retry_count).unwrap_or(0);

    if max_retries > 0 && current_retry_count < max_retries {
        // Task has retries remaining — set up_for_retry
        let new_retry_count = current_retry_count + 1;

        if let Some(conn) = try_open_db() {
            let task_repo = flowctl_db::TaskRepo::new(&conn);
            let _ = task_repo.update_status(task_id, Status::UpForRetry);

            let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
            let rt = RuntimeState {
                task_id: task_id.to_string(),
                assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
                claimed_at: None,
                completed_at: None,
                duration_secs: None,
                blocked_reason: None,
                baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
                final_rev: None,
                retry_count: new_retry_count,
            };
            let _ = runtime_repo.upsert(&rt);
        }

        // Update Markdown
        let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                if let Ok(mut doc) = frontmatter::parse::<Task>(&content) {
                    doc.frontmatter.status = Status::UpForRetry;
                    doc.frontmatter.updated_at = Utc::now();
                    if let Ok(new_content) = frontmatter::write(&doc) {
                        let _ = fs::write(&task_path, new_content);
                    }
                }
            }
        }

        (Status::UpForRetry, Vec::new())
    } else {
        // No retries remaining — mark failed and propagate
        if let Some(conn) = try_open_db() {
            let task_repo = flowctl_db::TaskRepo::new(&conn);
            let _ = task_repo.update_status(task_id, Status::Failed);
        }

        // Update Markdown
        let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                if let Ok(mut doc) = frontmatter::parse::<Task>(&content) {
                    doc.frontmatter.status = Status::Failed;
                    doc.frontmatter.updated_at = Utc::now();
                    if let Ok(new_content) = frontmatter::write(&doc) {
                        let _ = fs::write(&task_path, new_content);
                    }
                }
            }
        }

        let affected = propagate_upstream_failure(flow_dir, task_id);
        (Status::Failed, affected)
    }
}
