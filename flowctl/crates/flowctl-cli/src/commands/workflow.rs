//! Workflow commands: ready, next, start, done, block, restart, queue,
//! worker-phase next/done.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::Subcommand;
use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_epic_id, is_task_id, parse_id};
use flowctl_core::state_machine::Status;
use flowctl_core::types::{
    Epic, EpicStatus, Evidence, RuntimeState, Task, EPICS_DIR, FLOW_DIR, REVIEWS_DIR, TASKS_DIR,
};

/// Worker-phase subcommands.
#[derive(Subcommand, Debug)]
pub enum WorkerPhaseCmd {
    /// Return the next uncompleted phase.
    Next {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Include TDD phases.
        #[arg(long)]
        tdd: bool,
        /// Include review phase.
        #[arg(long, value_parser = ["rp", "codex"])]
        review: Option<String>,
    },
    /// Mark a phase as completed.
    Done {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Phase ID to mark done.
        #[arg(long)]
        phase: String,
        /// Include TDD phases.
        #[arg(long)]
        tdd: bool,
        /// Include review phase.
        #[arg(long, value_parser = ["rp", "codex"])]
        review: Option<String>,
    },
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Get the .flow/ directory path.
fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
}

/// Ensure .flow/ exists, error_exit if not.
fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Try to open a DB connection.
fn try_open_db() -> Option<rusqlite::Connection> {
    let cwd = env::current_dir().ok()?;
    flowctl_db::open(&cwd).ok()
}

/// Resolve current actor: FLOW_ACTOR env > git config user.email > git config user.name > $USER > "unknown"
fn resolve_actor() -> String {
    if let Ok(actor) = env::var("FLOW_ACTOR") {
        let trimmed = actor.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "user.email"])
        .output()
    {
        if output.status.success() {
            let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !email.is_empty() {
                return email;
            }
        }
    }
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "user.name"])
        .output()
    {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }
    if let Ok(user) = env::var("USER") {
        if !user.is_empty() {
            return user;
        }
    }
    "unknown".to_string()
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
fn load_tasks_for_epic(flow_dir: &Path, epic_id: &str) -> HashMap<String, Task> {
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
fn load_epic(flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(epic_id) {
            return Some(epic);
        }
    }
    load_epic_md(flow_dir, epic_id)
}

/// Load a task, trying DB first then Markdown.
fn load_task(flow_dir: &Path, task_id: &str) -> Option<Task> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::TaskRepo::new(&conn);
        if let Ok(task) = repo.get(task_id) {
            return Some(task);
        }
    }
    load_task_md(flow_dir, task_id)
}

/// Get runtime state for a task.
fn get_runtime(task_id: &str) -> Option<RuntimeState> {
    let conn = try_open_db()?;
    let repo = flowctl_db::RuntimeRepo::new(&conn);
    repo.get(task_id).ok().flatten()
}

/// Sort key for tasks: (priority, task_num, title).
fn task_sort_key(task: &Task) -> (u32, u32, String) {
    let parsed = parse_id(&task.id).ok();
    (
        task.sort_priority(),
        parsed.and_then(|p| p.task).unwrap_or(0),
        task.title.clone(),
    )
}

/// Scan all epic .md files in the epics directory, return their IDs sorted.
fn scan_epic_ids(flow_dir: &Path) -> Vec<String> {
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
fn patch_md_section(doc: &str, heading: &str, new_content: &str) -> Option<String> {
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
fn get_md_section(doc: &str, heading: &str) -> String {
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
fn find_dependents(flow_dir: &Path, task_id: &str) -> Vec<String> {
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
fn get_max_retries() -> u32 {
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
fn propagate_upstream_failure(flow_dir: &Path, failed_id: &str) -> Vec<String> {
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
fn handle_task_failure(
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

// ── Commands ────────────────────────────────────────────────────────

pub fn cmd_ready(json_mode: bool, epic: String) {
    let flow_dir = ensure_flow_exists();

    if !is_epic_id(&epic) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic
        ));
    }

    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic));
    if !epic_path.exists() {
        error_exit(&format!("Epic {} not found", epic));
    }

    let current_actor = resolve_actor();
    let tasks = load_tasks_for_epic(&flow_dir, &epic);

    let mut ready = Vec::new();
    let mut in_progress = Vec::new();
    let mut blocked: Vec<(Task, Vec<String>)> = Vec::new();

    for task in tasks.values() {
        match task.status {
            Status::InProgress => {
                in_progress.push(task.clone());
                continue;
            }
            Status::Done | Status::Skipped => continue,
            Status::Blocked => {
                blocked.push((task.clone(), vec!["status=blocked".to_string()]));
                continue;
            }
            Status::Todo => {}
            _ => continue,
        }

        // Check all deps are done/skipped
        let mut deps_done = true;
        let mut blocking_deps = Vec::new();
        for dep in &task.depends_on {
            match tasks.get(dep) {
                Some(dep_task) if dep_task.status.is_satisfied() => {}
                _ => {
                    deps_done = false;
                    blocking_deps.push(dep.clone());
                }
            }
        }

        if deps_done {
            ready.push(task.clone());
        } else {
            blocked.push((task.clone(), blocking_deps));
        }
    }

    ready.sort_by_key(|t| task_sort_key(t));
    in_progress.sort_by_key(|t| task_sort_key(t));
    blocked.sort_by_key(|(t, _)| task_sort_key(t));

    if json_mode {
        json_output(json!({
            "epic": epic,
            "actor": current_actor,
            "ready": ready.iter().map(|t| json!({
                "id": t.id,
                "title": t.title,
                "depends_on": t.depends_on,
            })).collect::<Vec<_>>(),
            "in_progress": in_progress.iter().map(|t| {
                let assignee = get_runtime(&t.id)
                    .and_then(|rt| rt.assignee)
                    .unwrap_or_default();
                json!({
                    "id": t.id,
                    "title": t.title,
                    "assignee": assignee,
                })
            }).collect::<Vec<_>>(),
            "blocked": blocked.iter().map(|(t, deps)| json!({
                "id": t.id,
                "title": t.title,
                "blocked_by": deps,
            })).collect::<Vec<_>>(),
        }));
    } else {
        println!("Ready tasks for {} (actor: {}):", epic, current_actor);
        if ready.is_empty() {
            println!("  (none)");
        } else {
            for t in &ready {
                println!("  {}: {}", t.id, t.title);
            }
        }
        if !in_progress.is_empty() {
            println!("\nIn progress:");
            for t in &in_progress {
                let assignee = get_runtime(&t.id)
                    .and_then(|rt| rt.assignee)
                    .unwrap_or_else(|| "unknown".to_string());
                let marker = if assignee == current_actor {
                    " (you)"
                } else {
                    ""
                };
                println!("  {}: {} [{}]{}", t.id, t.title, assignee, marker);
            }
        }
        if !blocked.is_empty() {
            println!("\nBlocked:");
            for (t, deps) in &blocked {
                println!("  {}: {} (by: {})", t.id, t.title, deps.join(", "));
            }
        }
    }
}

pub fn cmd_next(
    json_mode: bool,
    epics_file: Option<String>,
    require_plan_review: bool,
    require_completion_review: bool,
) {
    let flow_dir = ensure_flow_exists();
    let current_actor = resolve_actor();

    // Resolve epics list
    let epic_ids: Vec<String> = if let Some(ref file) = epics_file {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => error_exit(&format!("Cannot read epics file: {}", e)),
        };
        let data: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => error_exit(&format!("Epics file invalid JSON: {}", e)),
        };
        match data.get("epics").and_then(|v| v.as_array()) {
            Some(arr) => {
                let mut ids = Vec::new();
                for e in arr {
                    match e.as_str() {
                        Some(s) if is_epic_id(s) => ids.push(s.to_string()),
                        _ => error_exit(&format!("Invalid epic ID in epics file: {}", e)),
                    }
                }
                ids
            }
            None => error_exit("Epics file must be JSON with key 'epics' as a list"),
        }
    } else {
        scan_epic_ids(&flow_dir)
    };

    let mut blocked_epics: HashMap<String, Vec<String>> = HashMap::new();

    for epic_id in &epic_ids {
        let epic = match load_epic(&flow_dir, epic_id) {
            Some(e) => e,
            None => {
                if epics_file.is_some() {
                    error_exit(&format!("Epic {} not found", epic_id));
                }
                continue;
            }
        };

        if epic.status == EpicStatus::Done {
            continue;
        }

        // Check epic-level deps
        let mut epic_blocked_by = Vec::new();
        for dep in &epic.depends_on_epics {
            if dep == epic_id {
                continue;
            }
            match load_epic(&flow_dir, dep) {
                Some(dep_epic) if dep_epic.status == EpicStatus::Done => {}
                _ => epic_blocked_by.push(dep.clone()),
            }
        }
        if !epic_blocked_by.is_empty() {
            blocked_epics.insert(epic_id.clone(), epic_blocked_by);
            continue;
        }

        // Check plan review gate
        if require_plan_review
            && epic.plan_review != flowctl_core::types::ReviewStatus::Passed
        {
            if json_mode {
                json_output(json!({
                    "status": "plan",
                    "epic": epic_id,
                    "task": null,
                    "reason": "needs_plan_review",
                }));
            } else {
                println!("plan {} needs_plan_review", epic_id);
            }
            return;
        }

        let tasks = load_tasks_for_epic(&flow_dir, epic_id);

        // Resume in_progress tasks owned by current actor
        let mut my_in_progress: Vec<&Task> = tasks
            .values()
            .filter(|t| t.status == Status::InProgress)
            .filter(|t| {
                get_runtime(&t.id)
                    .and_then(|rt| rt.assignee)
                    .map(|a| a == current_actor)
                    .unwrap_or(false)
            })
            .collect();
        my_in_progress.sort_by_key(|t| task_sort_key(t));

        if let Some(task) = my_in_progress.first() {
            if json_mode {
                json_output(json!({
                    "status": "work",
                    "epic": epic_id,
                    "task": task.id,
                    "reason": "resume_in_progress",
                }));
            } else {
                println!("work {} resume_in_progress", task.id);
            }
            return;
        }

        // Find ready tasks
        let mut ready: Vec<&Task> = tasks
            .values()
            .filter(|t| t.status == Status::Todo)
            .filter(|t| {
                t.depends_on.iter().all(|dep| {
                    tasks
                        .get(dep)
                        .map(|dt| dt.status.is_satisfied())
                        .unwrap_or(false)
                })
            })
            .collect();
        ready.sort_by_key(|t| task_sort_key(t));

        if let Some(task) = ready.first() {
            if json_mode {
                json_output(json!({
                    "status": "work",
                    "epic": epic_id,
                    "task": task.id,
                    "reason": "ready_task",
                }));
            } else {
                println!("work {} ready_task", task.id);
            }
            return;
        }

        // Check completion review
        if require_completion_review
            && !tasks.is_empty()
            && tasks.values().all(|t| t.status == Status::Done)
            && epic.completion_review != flowctl_core::types::ReviewStatus::Passed
        {
            if json_mode {
                json_output(json!({
                    "status": "completion_review",
                    "epic": epic_id,
                    "task": null,
                    "reason": "needs_completion_review",
                }));
            } else {
                println!("completion_review {} needs_completion_review", epic_id);
            }
            return;
        }
    }

    // No work found
    if json_mode {
        let mut payload = json!({
            "status": "none",
            "epic": null,
            "task": null,
            "reason": "none",
        });
        if !blocked_epics.is_empty() {
            payload["reason"] = json!("blocked_by_epic_deps");
            payload["blocked_epics"] = json!(blocked_epics);
        }
        json_output(payload);
    } else if !blocked_epics.is_empty() {
        println!("none blocked_by_epic_deps");
        for (eid, deps) in &blocked_epics {
            println!("  {}: {}", eid, deps.join(", "));
        }
    } else {
        println!("none");
    }
}

pub fn cmd_queue(json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    let current_actor = resolve_actor();

    let epic_ids = scan_epic_ids(&flow_dir);
    let mut epics_data: Vec<serde_json::Value> = Vec::new();

    for epic_id in &epic_ids {
        let epic = match load_epic(&flow_dir, epic_id) {
            Some(e) => e,
            None => continue,
        };

        let tasks = load_tasks_for_epic(&flow_dir, epic_id);

        // Count tasks by status
        let mut todo = 0u64;
        let mut in_progress = 0u64;
        let mut done = 0u64;
        let mut blocked = 0u64;
        let mut ready = 0u64;

        for task in tasks.values() {
            match task.status {
                Status::Todo => {
                    todo += 1;
                    // Check if ready
                    let deps_done = task.depends_on.iter().all(|dep| {
                        tasks
                            .get(dep)
                            .map(|dt| dt.status.is_satisfied())
                            .unwrap_or(false)
                    });
                    if deps_done {
                        ready += 1;
                    }
                }
                Status::InProgress => in_progress += 1,
                Status::Done | Status::Skipped => done += 1,
                Status::Blocked => blocked += 1,
                _ => {}
            }
        }

        // Check epic-level deps
        let mut epic_blocked_by = Vec::new();
        for dep in &epic.depends_on_epics {
            if dep == epic_id {
                continue;
            }
            match load_epic(&flow_dir, dep) {
                Some(dep_epic) if dep_epic.status == EpicStatus::Done => {}
                _ => epic_blocked_by.push(dep.clone()),
            }
        }

        let total = todo + in_progress + done + blocked;
        let progress = if total > 0 {
            ((done as f64 / total as f64) * 100.0).round() as u64
        } else {
            0
        };

        epics_data.push(json!({
            "id": epic.id,
            "title": epic.title,
            "status": epic.status.to_string(),
            "plan_review_status": epic.plan_review.to_string(),
            "completion_review_status": epic.completion_review.to_string(),
            "depends_on_epics": epic.depends_on_epics,
            "blocked_by": epic_blocked_by,
            "tasks": {
                "todo": todo,
                "in_progress": in_progress,
                "done": done,
                "blocked": blocked,
                "ready": ready,
            },
            "total_tasks": total,
            "progress": progress,
        }));
    }

    // Sort: open (unblocked) first, then blocked, then done
    epics_data.sort_by(|a, b| {
        let a_status = if a["status"].as_str() == Some("done") {
            2
        } else if !a["blocked_by"]
            .as_array()
            .map_or(true, |v| v.is_empty())
        {
            1
        } else {
            0
        };
        let b_status = if b["status"].as_str() == Some("done") {
            2
        } else if !b["blocked_by"]
            .as_array()
            .map_or(true, |v| v.is_empty())
        {
            1
        } else {
            0
        };
        a_status.cmp(&b_status).then_with(|| {
            let a_num = parse_id(a["id"].as_str().unwrap_or(""))
                .map(|p| p.epic)
                .unwrap_or(0);
            let b_num = parse_id(b["id"].as_str().unwrap_or(""))
                .map(|p| p.epic)
                .unwrap_or(0);
            a_num.cmp(&b_num)
        })
    });

    if json_mode {
        json_output(json!({
            "actor": current_actor,
            "epics": epics_data,
            "total": epics_data.len(),
        }));
    } else {
        let open_count = epics_data
            .iter()
            .filter(|e| e["status"].as_str() != Some("done"))
            .count();
        let done_count = epics_data.len() - open_count;
        println!("Queue ({} open, {} done):\n", open_count, done_count);

        for e in &epics_data {
            let status_icon = if e["status"].as_str() == Some("done") {
                "\u{2713}"
            } else if !e["blocked_by"]
                .as_array()
                .map_or(true, |v| v.is_empty())
            {
                "\u{2298}"
            } else if e["tasks"]["ready"].as_u64().unwrap_or(0) > 0 {
                "\u{25b6}"
            } else {
                "\u{25cb}"
            };

            let tc = &e["tasks"];
            let progress = e["progress"].as_u64().unwrap_or(0);
            let bar_len = 20usize;
            let total = e["total_tasks"].as_u64().unwrap_or(0);
            let done_bars = if total > 0 {
                (progress as usize * bar_len / 100).min(bar_len)
            } else {
                0
            };
            let bar = format!(
                "{}{}",
                "\u{2588}".repeat(done_bars),
                "\u{2591}".repeat(bar_len - done_bars)
            );

            println!(
                "  {} {}: {}",
                status_icon,
                e["id"].as_str().unwrap_or(""),
                e["title"].as_str().unwrap_or("")
            );
            println!(
                "    [{}] {}%  done={} ready={} todo={} in_progress={} blocked={}",
                bar,
                progress,
                tc["done"].as_u64().unwrap_or(0),
                tc["ready"].as_u64().unwrap_or(0),
                tc["todo"].as_u64().unwrap_or(0),
                tc["in_progress"].as_u64().unwrap_or(0),
                tc["blocked"].as_u64().unwrap_or(0)
            );

            if let Some(blocked_by) = e["blocked_by"].as_array() {
                if !blocked_by.is_empty() {
                    let names: Vec<&str> =
                        blocked_by.iter().filter_map(|v| v.as_str()).collect();
                    println!("    \u{2298} blocked by: {}", names.join(", "));
                }
            }

            if let Some(deps) = e["depends_on_epics"].as_array() {
                let blocked_by = e["blocked_by"].as_array();
                if !deps.is_empty() && blocked_by.map_or(true, |v| v.is_empty()) {
                    let names: Vec<&str> = deps.iter().filter_map(|v| v.as_str()).collect();
                    println!("    \u{2192} deps (resolved): {}", names.join(", "));
                }
            }

            println!();
        }
    }
}

pub fn cmd_start(json_mode: bool, id: String, force: bool, _note: Option<String>) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Validate dependencies unless --force
    if !force {
        for dep in &task.depends_on {
            let dep_task = match load_task(&flow_dir, dep) {
                Some(t) => t,
                None => error_exit(&format!(
                    "Cannot start task {}: dependency {} not found",
                    id, dep
                )),
            };
            if !dep_task.status.is_satisfied() {
                error_exit(&format!(
                    "Cannot start task {}: dependency {} is '{}', not 'done'. \
                     Complete dependencies first or use --force to override.",
                    id, dep, dep_task.status
                ));
            }
        }
    }

    let current_actor = resolve_actor();
    let existing_rt = get_runtime(&id);
    let existing_assignee = existing_rt.as_ref().and_then(|rt| rt.assignee.clone());

    // Cannot start done task
    if task.status == Status::Done {
        error_exit(&format!("Cannot start task {}: status is 'done'.", id));
    }

    // Blocked requires --force
    if task.status == Status::Blocked && !force {
        error_exit(&format!(
            "Cannot start task {}: status is 'blocked'. Use --force to override.",
            id
        ));
    }

    // Check if claimed by someone else
    if !force {
        if let Some(ref assignee) = existing_assignee {
            if assignee != &current_actor {
                error_exit(&format!(
                    "Cannot start task {}: claimed by '{}'. Use --force to override.",
                    id, assignee
                ));
            }
        }
    }

    // Validate task is in todo status (unless --force or resuming own task)
    if !force && task.status != Status::Todo {
        let can_resume = task.status == Status::InProgress
            && existing_assignee
                .as_ref()
                .map(|a| a == &current_actor)
                .unwrap_or(false);
        if !can_resume {
            error_exit(&format!(
                "Cannot start task {}: status is '{}', expected 'todo'. Use --force to override.",
                id, task.status
            ));
        }
    }

    // Build runtime state
    let now = Utc::now();
    let force_takeover = force
        && existing_assignee
            .as_ref()
            .map(|a| a != &current_actor)
            .unwrap_or(false);
    let new_assignee = if existing_assignee.is_none() || force_takeover {
        current_actor.clone()
    } else {
        existing_assignee.clone().unwrap_or_else(|| current_actor.clone())
    };

    let claimed_at = if existing_rt
        .as_ref()
        .and_then(|rt| rt.claimed_at)
        .is_some()
        && !force_takeover
    {
        existing_rt.as_ref().unwrap().claimed_at
    } else {
        Some(now)
    };

    let runtime_state = RuntimeState {
        task_id: id.clone(),
        assignee: Some(new_assignee),
        claimed_at,
        completed_at: None,
        duration_secs: None,
        blocked_reason: None,
        baseline_rev: existing_rt.as_ref().and_then(|rt| rt.baseline_rev.clone()),
        final_rev: None,
        retry_count: existing_rt.as_ref().map(|rt| rt.retry_count).unwrap_or(0),
    };

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        if let Err(e) = task_repo.update_status(&id, Status::InProgress) {
            error_exit(&format!("Failed to update task status: {}", e));
        }
        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        if let Err(e) = runtime_repo.upsert(&runtime_state) {
            error_exit(&format!("Failed to update runtime state: {}", e));
        }
    }

    // Update Markdown frontmatter
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_path.exists() {
        if let Ok(content) = fs::read_to_string(&task_path) {
            if let Ok(mut doc) = frontmatter::parse::<Task>(&content) {
                doc.frontmatter.status = Status::InProgress;
                doc.frontmatter.updated_at = now;
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_path, new_content);
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "in_progress",
            "message": format!("Task {} started", id),
        }));
    } else {
        println!("Task {} started", id);
    }
}

pub fn cmd_done(
    json_mode: bool,
    id: String,
    summary_file: Option<String>,
    summary: Option<String>,
    evidence_json: Option<String>,
    evidence: Option<String>,
    force: bool,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Require in_progress status (unless --force)
    if !force {
        match task.status {
            Status::InProgress => {}
            Status::Done => error_exit(&format!("Task {} is already done.", id)),
            other => error_exit(&format!(
                "Task {} is '{}', not 'in_progress'. Use --force to override.",
                id, other
            )),
        }
    }

    // Prevent cross-actor completion (unless --force)
    let current_actor = resolve_actor();
    let runtime = get_runtime(&id);
    if !force {
        if let Some(ref rt) = runtime {
            if let Some(ref assignee) = rt.assignee {
                if assignee != &current_actor {
                    error_exit(&format!(
                        "Cannot complete task {}: claimed by '{}'. Use --force to override.",
                        id, assignee
                    ));
                }
            }
        }
    }

    // Get summary
    let summary_text = if let Some(ref file) = summary_file {
        match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => error_exit(&format!("Cannot read summary file: {}", e)),
        }
    } else if let Some(ref s) = summary {
        s.clone()
    } else {
        "- Task completed".to_string()
    };

    // Get evidence
    let evidence_obj: serde_json::Value = if let Some(ref ev) = evidence_json {
        let raw = if ev.trim().starts_with('{') {
            ev.clone()
        } else {
            match fs::read_to_string(ev) {
                Ok(s) => s,
                Err(e) => error_exit(&format!("Cannot read evidence file: {}", e)),
            }
        };
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => error_exit(&format!("Evidence JSON invalid: {}", e)),
        }
    } else if let Some(ref ev) = evidence {
        match serde_json::from_str(ev) {
            Ok(v) => v,
            Err(e) => error_exit(&format!("Evidence invalid JSON: {}", e)),
        }
    } else {
        json!({"commits": [], "tests": [], "prs": []})
    };

    if !evidence_obj.is_object() {
        error_exit("Evidence JSON must be an object with keys: commits/tests/prs");
    }

    // Calculate duration from claimed_at
    let duration_seconds: Option<u64> = runtime
        .as_ref()
        .and_then(|rt| rt.claimed_at)
        .map(|start| {
            let dur = Utc::now() - start;
            dur.num_seconds().max(0) as u64
        });

    // Validate workspace_changes if present
    let ws_changes = evidence_obj.get("workspace_changes");
    let mut ws_warning: Option<String> = None;
    if let Some(wc) = ws_changes {
        if !wc.is_object() {
            ws_warning = Some("workspace_changes must be an object".to_string());
        } else {
            let required = [
                "baseline_rev",
                "final_rev",
                "files_changed",
                "insertions",
                "deletions",
            ];
            let missing: Vec<&str> = required
                .iter()
                .filter(|k| !wc.as_object().unwrap().contains_key(**k))
                .copied()
                .collect();
            if !missing.is_empty() {
                ws_warning = Some(format!(
                    "workspace_changes missing keys: {}",
                    missing.join(", ")
                ));
            }
        }
    }

    // Format evidence as markdown
    let to_list = |val: Option<&serde_json::Value>| -> Vec<String> {
        match val {
            None => Vec::new(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Some(serde_json::Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    };

    let commits = to_list(evidence_obj.get("commits"));
    let tests = to_list(evidence_obj.get("tests"));
    let prs = to_list(evidence_obj.get("prs"));

    let mut evidence_md = Vec::new();
    if commits.is_empty() {
        evidence_md.push("- Commits:".to_string());
    } else {
        evidence_md.push(format!("- Commits: {}", commits.join(", ")));
    }
    if tests.is_empty() {
        evidence_md.push("- Tests:".to_string());
    } else {
        evidence_md.push(format!("- Tests: {}", tests.join(", ")));
    }
    if prs.is_empty() {
        evidence_md.push("- PRs:".to_string());
    } else {
        evidence_md.push(format!("- PRs: {}", prs.join(", ")));
    }

    if ws_warning.is_none() {
        if let Some(wc) = ws_changes {
            if wc.is_object() {
                let fc = wc.get("files_changed").and_then(|v| v.as_u64()).unwrap_or(0);
                let ins = wc.get("insertions").and_then(|v| v.as_u64()).unwrap_or(0);
                let del = wc.get("deletions").and_then(|v| v.as_u64()).unwrap_or(0);
                let br = wc
                    .get("baseline_rev")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let fr = wc
                    .get("final_rev")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                evidence_md.push(format!(
                    "- Workspace: {} files changed, +{} -{} ({}..{})",
                    fc,
                    ins,
                    del,
                    &br[..br.len().min(7)],
                    &fr[..fr.len().min(7)]
                ));
            }
        }
    }

    if let Some(dur) = duration_seconds {
        let mins = dur / 60;
        let secs = dur % 60;
        let dur_str = if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        };
        evidence_md.push(format!("- Duration: {}", dur_str));
    }
    let evidence_content = evidence_md.join("\n");

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        let _ = task_repo.update_status(&id, Status::Done);

        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        let now = Utc::now();
        let rt = RuntimeState {
            task_id: id.clone(),
            assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: runtime.as_ref().and_then(|r| r.claimed_at),
            completed_at: Some(now),
            duration_secs: duration_seconds,
            blocked_reason: None,
            baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: runtime.as_ref().and_then(|r| r.final_rev.clone()),
            retry_count: runtime.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt);

        // Store evidence
        let ev = Evidence {
            commits: commits.clone(),
            tests: tests.clone(),
            prs: prs.clone(),
            ..Evidence::default()
        };
        let evidence_repo = flowctl_db::EvidenceRepo::new(&conn);
        let _ = evidence_repo.upsert(&id, &ev);
    }

    // Update Markdown spec
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(current_spec) = fs::read_to_string(&task_spec_path) {
            let mut updated = current_spec;
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &summary_text) {
                updated = patched;
            }
            if let Some(patched) = patch_md_section(&updated, "## Evidence", &evidence_content) {
                updated = patched;
            }

            // Update frontmatter status
            if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                doc.frontmatter.status = Status::Done;
                doc.frontmatter.updated_at = Utc::now();
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_spec_path, new_content);
                }
            } else {
                let _ = fs::write(&task_spec_path, updated);
            }
        }
    }

    // Archive review receipt if present
    if let Some(receipt) = evidence_obj.get("review_receipt") {
        if receipt.is_object() {
            let reviews_dir = flow_dir.join(REVIEWS_DIR);
            let _ = fs::create_dir_all(&reviews_dir);
            let mode = receipt
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let rtype = receipt
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("review");
            let filename = format!("{}-{}-{}.json", rtype, id, mode);
            if let Ok(content) = serde_json::to_string_pretty(receipt) {
                let _ = fs::write(reviews_dir.join(filename), content);
            }
        }
    }

    if json_mode {
        let mut result = json!({
            "id": id,
            "status": "done",
            "message": format!("Task {} completed", id),
        });
        if let Some(dur) = duration_seconds {
            result["duration_seconds"] = json!(dur);
        }
        if let Some(ref warn) = ws_warning {
            result["warning"] = json!(warn);
        }
        json_output(result);
    } else {
        let dur_str = duration_seconds.map(|dur| {
            let mins = dur / 60;
            let secs = dur % 60;
            if mins > 0 {
                format!(" ({}m {}s)", mins, secs)
            } else {
                format!(" ({}s)", secs)
            }
        });
        println!("Task {} completed{}", id, dur_str.unwrap_or_default());
        if let Some(warn) = ws_warning {
            println!("  warning: {}", warn);
        }
    }
}

pub fn cmd_block(json_mode: bool, id: String, reason_file: String) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    if task.status == Status::Done {
        error_exit(&format!("Cannot block task {}: status is 'done'.", id));
    }

    let reason = match fs::read_to_string(&reason_file) {
        Ok(s) => s.trim().to_string(),
        Err(e) => error_exit(&format!("Cannot read reason file: {}", e)),
    };

    if reason.is_empty() {
        error_exit("Reason file is empty");
    }

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        let _ = task_repo.update_status(&id, Status::Blocked);

        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        let existing = runtime_repo.get(&id).ok().flatten();
        let rt = RuntimeState {
            task_id: id.clone(),
            assignee: existing.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: existing.as_ref().and_then(|r| r.claimed_at),
            completed_at: None,
            duration_secs: None,
            blocked_reason: Some(reason.clone()),
            baseline_rev: existing.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: None,
            retry_count: existing.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt);
    }

    // Update Markdown spec
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(current_spec) = fs::read_to_string(&task_spec_path) {
            let existing_summary = get_md_section(&current_spec, "## Done summary");
            let new_summary = if existing_summary.is_empty()
                || existing_summary.to_lowercase() == "tbd"
            {
                format!("Blocked:\n{}", reason)
            } else {
                format!("{}\n\nBlocked:\n{}", existing_summary, reason)
            };

            let mut updated = current_spec;
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &new_summary) {
                updated = patched;
            }

            // Update frontmatter
            if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                doc.frontmatter.status = Status::Blocked;
                doc.frontmatter.updated_at = Utc::now();
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_spec_path, new_content);
                }
            } else {
                let _ = fs::write(&task_spec_path, updated);
            }
        }
    }

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "blocked",
            "message": format!("Task {} blocked", id),
        }));
    } else {
        println!("Task {} blocked", id);
    }
}

pub fn cmd_fail(json_mode: bool, id: String, reason: Option<String>, force: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    if !force && task.status != Status::InProgress {
        error_exit(&format!(
            "Task {} is '{}', not 'in_progress'. Use --force to override.",
            id, task.status
        ));
    }

    let runtime = get_runtime(&id);
    let reason_text = reason.unwrap_or_else(|| "Task failed".to_string());

    let (final_status, upstream_failed_ids) = handle_task_failure(&flow_dir, &id, &runtime);

    // Update Done summary with failure reason
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(content) = fs::read_to_string(&task_spec_path) {
            let mut updated = content;
            let summary = format!("Failed:\n{}", reason_text);
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &summary) {
                updated = patched;
            }
            // Frontmatter was already updated by handle_task_failure, just write body changes
            let _ = fs::write(&task_spec_path, updated);
        }
    }

    if json_mode {
        let mut result = json!({
            "id": id,
            "status": final_status.to_string(),
            "message": format!("Task {} {}", id, final_status),
            "reason": reason_text,
        });
        if !upstream_failed_ids.is_empty() {
            result["upstream_failed"] = json!(upstream_failed_ids);
        }
        json_output(result);
    } else {
        println!("Task {} {}", id, final_status);
        if final_status == Status::UpForRetry {
            let max = get_max_retries();
            let count = runtime.as_ref().map(|r| r.retry_count).unwrap_or(0) + 1;
            println!("  retry {}/{} — will be retried by scheduler", count, max);
        }
        if !upstream_failed_ids.is_empty() {
            println!(
                "  upstream_failed propagated to {} downstream task(s):",
                upstream_failed_ids.len()
            );
            for tid in &upstream_failed_ids {
                println!("    {}", tid);
            }
        }
    }
}

pub fn cmd_restart(json_mode: bool, id: String, dry_run: bool, force: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Check epic not closed
    if let Ok(epic_id) = epic_id_from_task(&id) {
        if let Some(epic) = load_epic(&flow_dir, &epic_id) {
            if epic.status == EpicStatus::Done {
                error_exit(&format!("Cannot restart task in closed epic {}", epic_id));
            }
        }
    }

    // Find all downstream dependents
    let dependents = find_dependents(&flow_dir, &id);

    // Check for in_progress tasks
    let mut in_progress_ids = Vec::new();
    if task.status == Status::InProgress {
        in_progress_ids.push(id.clone());
    }
    for dep_id in &dependents {
        if let Some(dep_task) = load_task(&flow_dir, dep_id) {
            if dep_task.status == Status::InProgress {
                in_progress_ids.push(dep_id.clone());
            }
        }
    }

    if !in_progress_ids.is_empty() && !force {
        error_exit(&format!(
            "Cannot restart: tasks in progress: {}. Use --force to override.",
            in_progress_ids.join(", ")
        ));
    }

    // Build full reset list
    let all_ids: Vec<String> = std::iter::once(id.clone())
        .chain(dependents.iter().cloned())
        .collect();
    let mut to_reset = Vec::new();
    let mut skipped = Vec::new();

    for tid in &all_ids {
        let t = match load_task(&flow_dir, tid) {
            Some(t) => t,
            None => continue,
        };
        if t.status == Status::Todo {
            skipped.push(tid.clone());
        } else {
            to_reset.push(tid.clone());
        }
    }

    // Dry-run mode
    if dry_run {
        if json_mode {
            json_output(json!({
                "dry_run": true,
                "would_reset": to_reset,
                "already_todo": skipped,
                "in_progress_overridden": if force { in_progress_ids.clone() } else { Vec::<String>::new() },
            }));
        } else {
            println!(
                "Dry run \u{2014} would restart {} task(s):",
                to_reset.len()
            );
            for tid in &to_reset {
                if let Some(t) = load_task(&flow_dir, tid) {
                    let marker = if in_progress_ids.contains(tid) {
                        " (force)"
                    } else {
                        ""
                    };
                    println!("  {}  {} -> todo{}", tid, t.status, marker);
                }
            }
            if !skipped.is_empty() {
                println!("Already todo: {}", skipped.join(", "));
            }
        }
        return;
    }

    // Execute reset
    let mut reset_ids = Vec::new();
    for tid in &to_reset {
        // Reset in SQLite
        if let Some(conn) = try_open_db() {
            let task_repo = flowctl_db::TaskRepo::new(&conn);
            let _ = task_repo.update_status(tid, Status::Todo);

            // Clear runtime state
            let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
            let rt = RuntimeState {
                task_id: tid.clone(),
                assignee: None,
                claimed_at: None,
                completed_at: None,
                duration_secs: None,
                blocked_reason: None,
                baseline_rev: None,
                final_rev: None,
                retry_count: 0,
            };
            let _ = runtime_repo.upsert(&rt);
        }

        // Update Markdown frontmatter + clear evidence
        let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", tid));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                let mut updated = content;

                // Clear sections
                if let Some(patched) = patch_md_section(&updated, "## Done summary", "TBD") {
                    updated = patched;
                }
                if let Some(patched) = patch_md_section(&updated, "## Evidence", "TBD") {
                    updated = patched;
                }

                // Update frontmatter status
                if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                    doc.frontmatter.status = Status::Todo;
                    doc.frontmatter.updated_at = Utc::now();
                    if let Ok(new_content) = frontmatter::write(&doc) {
                        updated = new_content;
                    }
                }

                let _ = fs::write(&task_path, updated);
            }
        }

        reset_ids.push(tid.clone());
    }

    if json_mode {
        json_output(json!({
            "reset": reset_ids,
            "skipped": skipped,
            "cascade_from": id,
        }));
    } else if reset_ids.is_empty() {
        println!(
            "Nothing to restart \u{2014} {} and dependents already todo.",
            id
        );
    } else {
        let downstream_count =
            reset_ids.len() - if reset_ids.contains(&id) { 1 } else { 0 };
        println!(
            "Restarted from {} (cascade: {} downstream):\n",
            id, downstream_count
        );
        for tid in &reset_ids {
            let marker = if *tid == id { " (target)" } else { "" };
            println!("  {}  -> todo{}", tid, marker);
        }
    }
}

// ── Phase definitions ──────────────────────────────────────────────

/// Phase definition: (id, title, done_condition).
struct PhaseDef {
    id: &'static str,
    title: &'static str,
    done_condition: &'static str,
}

const PHASE_DEFS: &[PhaseDef] = &[
    PhaseDef { id: "0",   title: "Verify Configuration",  done_condition: "OWNED_FILES verified and configuration validated" },
    PhaseDef { id: "1",   title: "Re-anchor",             done_condition: "Run flowctl show <task> and verify spec was read" },
    PhaseDef { id: "2a",  title: "TDD Red-Green",         done_condition: "Failing tests written and confirmed to fail" },
    PhaseDef { id: "2",   title: "Implement",             done_condition: "Feature implemented and code compiles" },
    PhaseDef { id: "2.5", title: "Verify & Fix",          done_condition: "flowctl guard passes and diff reviewed" },
    PhaseDef { id: "3",   title: "Commit",                done_condition: "Changes committed with conventional commit message" },
    PhaseDef { id: "4",   title: "Review",                done_condition: "SHIP verdict received from reviewer" },
    PhaseDef { id: "5",   title: "Complete",              done_condition: "flowctl done called and task status is done" },
    PhaseDef { id: "5b",  title: "Memory Auto-Save",      done_condition: "Non-obvious lessons saved to memory (if any)" },
    PhaseDef { id: "6",   title: "Return",                done_condition: "Summary returned to main conversation" },
];

/// Canonical ordering of all phases — used to merge sequences.
const CANONICAL_ORDER: &[&str] = &["0", "1", "2a", "2", "2.5", "3", "4", "5", "5b", "6"];

/// Default phase sequence (Worktree + Teams, always includes Phase 0).
const PHASE_SEQ_DEFAULT: &[&str] = &["0", "1", "2", "2.5", "3", "5", "5b", "6"];
const PHASE_SEQ_TDD: &[&str]    = &["0", "1", "2a", "2", "2.5", "3", "5", "5b", "6"];
const PHASE_SEQ_REVIEW: &[&str] = &["0", "1", "2", "2.5", "3", "4", "5", "5b", "6"];

fn get_phase_def(phase_id: &str) -> Option<&'static PhaseDef> {
    PHASE_DEFS.iter().find(|p| p.id == phase_id)
}

/// Build the phase sequence based on mode flags.
fn build_phase_sequence(tdd: bool, review: bool) -> Vec<&'static str> {
    if !tdd && !review {
        return PHASE_SEQ_DEFAULT.to_vec();
    }

    let mut phases = std::collections::HashSet::new();
    for p in PHASE_SEQ_DEFAULT {
        phases.insert(*p);
    }
    if tdd {
        for p in PHASE_SEQ_TDD {
            phases.insert(*p);
        }
    }
    if review {
        for p in PHASE_SEQ_REVIEW {
            phases.insert(*p);
        }
    }
    CANONICAL_ORDER.iter().copied().filter(|p| phases.contains(p)).collect()
}

/// Load completed phases from SQLite.
fn load_completed_phases(task_id: &str) -> Vec<String> {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::PhaseProgressRepo::new(&conn);
        repo.get_completed(task_id).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Mark a phase as done in SQLite.
fn save_phase_done(task_id: &str, phase: &str) {
    if let Some(conn) = try_open_db() {
        let repo = flowctl_db::PhaseProgressRepo::new(&conn);
        if let Err(e) = repo.mark_done(task_id, phase) {
            eprintln!("Warning: failed to save phase progress: {}", e);
        }
    }
}

// ── Worker-phase dispatch ─────────────────────────────────────────

pub fn dispatch_worker_phase(cmd: &WorkerPhaseCmd, json_mode: bool) {
    match cmd {
        WorkerPhaseCmd::Next { task, tdd, review } => {
            cmd_worker_phase_next(json_mode, task, *tdd, review.as_deref());
        }
        WorkerPhaseCmd::Done { task, phase, tdd, review } => {
            cmd_worker_phase_done(json_mode, task, phase, *tdd, review.as_deref());
        }
    }
}

fn cmd_worker_phase_next(json_mode: bool, task_id: &str, tdd: bool, review: Option<&str>) {
    let _flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some());
    let completed = load_completed_phases(task_id);
    let completed_set: std::collections::HashSet<&str> =
        completed.iter().map(|s| s.as_str()).collect();

    // Find first uncompleted phase
    let next_phase = seq.iter().find(|p| !completed_set.contains(**p)).copied();

    match next_phase {
        None => {
            if json_mode {
                json_output(json!({
                    "phase": null,
                    "all_done": true,
                    "sequence": seq,
                }));
            } else {
                println!("All phases completed.");
            }
        }
        Some(phase_id) => {
            let def = get_phase_def(phase_id);
            let title = def.map(|d| d.title).unwrap_or("Unknown");
            let done_condition = def.map(|d| d.done_condition).unwrap_or("");

            let sorted_completed: Vec<&str> = seq.iter()
                .copied()
                .filter(|p| completed_set.contains(*p))
                .collect();

            if json_mode {
                json_output(json!({
                    "phase": phase_id,
                    "title": title,
                    "done_condition": done_condition,
                    "content": "",
                    "completed_phases": sorted_completed,
                    "sequence": seq,
                    "all_done": false,
                }));
            } else {
                println!("Next phase: {} - {}", phase_id, title);
                println!("Done when: {}", done_condition);
                if !sorted_completed.is_empty() {
                    println!("Completed: {}", sorted_completed.join(", "));
                }
            }
        }
    }
}

fn cmd_worker_phase_done(
    json_mode: bool,
    task_id: &str,
    phase: &str,
    tdd: bool,
    review: Option<&str>,
) {
    let _flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some());

    // Validate phase exists in sequence
    if !seq.contains(&phase) {
        error_exit(&format!(
            "Phase '{}' is not in the current sequence: {}. \
             Check your mode flags (--tdd, --review).",
            phase,
            seq.join(", ")
        ));
    }

    let completed = load_completed_phases(task_id);
    let completed_set: std::collections::HashSet<&str> =
        completed.iter().map(|s| s.as_str()).collect();

    // Find expected next phase (first uncompleted)
    let expected = seq.iter().find(|p| !completed_set.contains(**p)).copied();

    match expected {
        None => {
            error_exit("All phases are already completed. Nothing to mark done.");
        }
        Some(exp) if exp != phase => {
            error_exit(&format!(
                "Expected phase {}, got phase {}. Cannot skip phases.",
                exp, phase
            ));
        }
        _ => {}
    }

    // Mark phase done
    save_phase_done(task_id, phase);

    // Reload to get updated state
    let updated_completed = load_completed_phases(task_id);
    let updated_set: std::collections::HashSet<&str> =
        updated_completed.iter().map(|s| s.as_str()).collect();
    let next_phase = seq.iter().find(|p| !updated_set.contains(**p)).copied();
    let all_done = next_phase.is_none();

    if json_mode {
        let mut result = json!({
            "completed_phase": phase,
            "completed_phases": updated_completed,
            "all_done": all_done,
        });
        if let Some(np) = next_phase {
            let def = get_phase_def(np);
            result["next_phase"] = json!({
                "phase": np,
                "title": def.map(|d| d.title).unwrap_or("Unknown"),
                "done_condition": def.map(|d| d.done_condition).unwrap_or(""),
            });
        }
        json_output(result);
    } else {
        println!("Phase {} marked done.", phase);
        if let Some(np) = next_phase {
            let def = get_phase_def(np);
            let title = def.map(|d| d.title).unwrap_or("Unknown");
            println!("Next: {} - {}", np, title);
        } else {
            println!("All phases completed.");
        }
    }
}
