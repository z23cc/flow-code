//! Query commands: show, epics, tasks, list, cat, files, lock, unlock, lock-check.
//!
//! Reads from SQLite if available, falls back to scanning Markdown files.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{is_epic_id, is_task_id, parse_id};
use flowctl_core::types::{
    Epic, Task, EPICS_DIR, SPECS_DIR, TASKS_DIR,
};

use super::helpers::get_flow_dir;

// ── Helpers ─────────────────────────────────────────────────────────

/// Ensure .flow/ exists, error_exit if not.
fn ensure_flow_exists() -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

/// Try to open a DB connection. Returns None if DB doesn't exist or can't be opened.
fn try_open_db() -> Option<crate::commands::db_shim::Connection> {
    let cwd = env::current_dir().ok()?;
    crate::commands::db_shim::open(&cwd).ok()
}

/// Serialize an Epic to the JSON format matching Python output.
fn epic_to_json(epic: &Epic) -> serde_json::Value {
    let spec_path = format!(".flow/specs/{}.md", epic.id);
    json!({
        "id": epic.id,
        "title": epic.title,
        "status": epic.status.to_string(),
        "branch_name": epic.branch_name,
        "plan_review_status": epic.plan_review.to_string(),
        "plan_reviewed_at": null,
        "completion_review_status": epic.completion_review.to_string(),
        "completion_reviewed_at": null,
        "depends_on_epics": epic.depends_on_epics,
        "default_impl": epic.default_impl,
        "default_review": epic.default_review,
        "default_sync": epic.default_sync,
        "spec_path": spec_path,
        "created_at": epic.created_at.to_rfc3339(),
        "updated_at": epic.updated_at.to_rfc3339(),
    })
}

/// Serialize a Task to the JSON format matching Python output.
fn task_to_json(task: &Task) -> serde_json::Value {
    let spec_path = format!(".flow/tasks/{}.md", task.id);

    // Try to get runtime state from DB
    let mut assignee: serde_json::Value = json!(null);
    let mut claimed_at: serde_json::Value = json!(null);
    let claim_note: serde_json::Value = json!("");

    if let Some(conn) = try_open_db() {
        let runtime_repo = crate::commands::db_shim::RuntimeRepo::new(&conn);
        if let Ok(Some(state)) = runtime_repo.get(&task.id) {
            if let Some(a) = &state.assignee {
                assignee = json!(a);
            }
            if let Some(ca) = &state.claimed_at {
                claimed_at = json!(ca.to_rfc3339());
            }
        }
    }

    json!({
        "id": task.id,
        "epic": task.epic,
        "title": task.title,
        "status": task.status.to_string(),
        "priority": task.priority,
        "domain": task.domain.to_string(),
        "depends_on": task.depends_on,
        "files": task.files,
        "impl": task.r#impl,
        "review": task.review,
        "sync": task.sync,
        "assignee": assignee,
        "claimed_at": claimed_at,
        "claim_note": claim_note,
        "spec_path": spec_path,
        "created_at": task.created_at.to_rfc3339(),
        "updated_at": task.updated_at.to_rfc3339(),
    })
}

/// Task summary for list/show contexts (less detail than full task_to_json).
fn task_summary_json(task: &Task) -> serde_json::Value {
    json!({
        "id": task.id,
        "title": task.title,
        "status": task.status.to_string(),
        "priority": task.priority,
        "depends_on": task.depends_on,
    })
}

/// Task summary for tasks command (includes epic, domain).
fn task_list_json(task: &Task) -> serde_json::Value {
    json!({
        "id": task.id,
        "epic": task.epic,
        "title": task.title,
        "status": task.status.to_string(),
        "priority": task.priority,
        "domain": task.domain.to_string(),
        "depends_on": task.depends_on,
    })
}

// ── Markdown scanning fallback ──────────────────────────────────────

/// Scan .flow/epics/*.md and parse all epics from frontmatter.
fn scan_epics_md(flow_dir: &Path) -> Vec<Epic> {
    let epics_dir = flow_dir.join(EPICS_DIR);
    if !epics_dir.is_dir() {
        return Vec::new();
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(&epics_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    entries.sort();

    let mut epics = Vec::new();
    for path in entries {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_epic_id(stem) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut epic) = frontmatter::parse_frontmatter::<Epic>(&content) {
                epic.file_path = Some(format!("epics/{}", path.file_name().unwrap().to_string_lossy()));
                epics.push(epic);
            }
        }
    }

    // Sort by epic number
    epics.sort_by_key(|e| parse_id(&e.id).map(|p| p.epic).unwrap_or(0));
    epics
}

/// Scan .flow/tasks/*.md and parse all tasks from frontmatter.
fn scan_tasks_md(flow_dir: &Path, epic_filter: Option<&str>) -> Vec<Task> {
    let tasks_dir = flow_dir.join(TASKS_DIR);
    if !tasks_dir.is_dir() {
        return Vec::new();
    }

    let mut entries: Vec<PathBuf> = match fs::read_dir(&tasks_dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    entries.sort();

    let mut tasks = Vec::new();
    for path in entries {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_task_id(stem) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut task) = frontmatter::parse_frontmatter::<Task>(&content) {
                if let Some(filter) = epic_filter {
                    if task.epic != filter {
                        continue;
                    }
                }
                task.file_path = Some(format!("tasks/{}", path.file_name().unwrap().to_string_lossy()));
                tasks.push(task);
            }
        }
    }

    // Sort by (epic_num, task_num)
    tasks.sort_by_key(|t| {
        let parsed = parse_id(&t.id).ok();
        (
            parsed.as_ref().map(|p| p.epic).unwrap_or(0),
            parsed.as_ref().and_then(|p| p.task).unwrap_or(0),
        )
    });
    tasks
}

/// Get a single epic by ID, trying DB first then Markdown.
fn get_epic(flow_dir: &Path, id: &str) -> Option<Epic> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(epic) = repo.get(id) {
            return Some(epic);
        }
    }

    // Fall back to Markdown
    let epic_path = flow_dir.join(EPICS_DIR).join(format!("{}.md", id));
    if !epic_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&epic_path).ok()?;
    let mut epic = frontmatter::parse_frontmatter::<Epic>(&content).ok()?;
    epic.file_path = Some(format!("epics/{}.md", id));
    Some(epic)
}

/// Get a single task by ID, trying DB first then Markdown.
fn get_task(flow_dir: &Path, id: &str) -> Option<Task> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Ok(task) = repo.get(id) {
            return Some(task);
        }
    }

    // Fall back to Markdown
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if !task_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&task_path).ok()?;
    let mut task = frontmatter::parse_frontmatter::<Task>(&content).ok()?;
    task.file_path = Some(format!("tasks/{}.md", id));
    Some(task)
}

/// Get all tasks for an epic, trying DB first then Markdown.
fn get_epic_tasks(flow_dir: &Path, epic_id: &str) -> Vec<Task> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        if let Ok(tasks) = repo.list_by_epic(epic_id) {
            if !tasks.is_empty() {
                return tasks;
            }
        }
    }

    // Fall back to Markdown
    scan_tasks_md(flow_dir, Some(epic_id))
}

/// Get all epics, trying DB first then Markdown.
fn get_all_epics(flow_dir: &Path) -> Vec<Epic> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::EpicRepo::new(&conn);
        if let Ok(epics) = repo.list(None) {
            if !epics.is_empty() {
                return epics;
            }
        }
    }

    // Fall back to Markdown
    scan_epics_md(flow_dir)
}

/// Get all tasks, optionally filtered, trying DB first then Markdown.
fn get_all_tasks(
    flow_dir: &Path,
    epic_filter: Option<&str>,
    status_filter: Option<&str>,
    domain_filter: Option<&str>,
) -> Vec<Task> {
    // Try DB first
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        match epic_filter {
            Some(epic_id) => {
                if let Ok(mut tasks) = repo.list_by_epic(epic_id) {
                    // Apply status/domain filters
                    if let Some(status) = status_filter {
                        tasks.retain(|t| t.status.to_string() == status);
                    }
                    if let Some(domain) = domain_filter {
                        tasks.retain(|t| t.domain.to_string() == domain);
                    }
                    if !tasks.is_empty() {
                        return tasks;
                    }
                    // If empty, might be a new epic not yet in DB - fall through
                }
            }
            None => {
                if let Ok(tasks) = repo.list_all(status_filter, domain_filter) {
                    if !tasks.is_empty() {
                        return tasks;
                    }
                }
            }
        }
    }

    // Fall back to Markdown scan
    let mut tasks = scan_tasks_md(flow_dir, epic_filter);

    // Apply filters
    if let Some(status) = status_filter {
        tasks.retain(|t| t.status.to_string() == status);
    }
    if let Some(domain) = domain_filter {
        tasks.retain(|t| t.domain.to_string() == domain);
    }

    tasks
}

// ── Show command ────────────────────────────────────────────────────

pub fn cmd_show(json: bool, id: String) {
    let flow_dir = ensure_flow_exists();

    if is_epic_id(&id) {
        let epic = match get_epic(&flow_dir, &id) {
            Some(e) => e,
            None => {
                error_exit(&format!("Epic not found: {}", id));
            }
        };

        // Get tasks for this epic
        let tasks = get_epic_tasks(&flow_dir, &id);
        let task_summaries: Vec<serde_json::Value> = tasks
            .iter()
            .map(task_summary_json)
            .collect();

        if json {
            let mut result = epic_to_json(&epic);
            result["tasks"] = json!(task_summaries);
            json_output(result);
        } else {
            println!("Epic: {}", epic.id);
            println!("Title: {}", epic.title);
            println!("Status: {}", epic.status);
            println!("Spec: .flow/specs/{}.md", epic.id);
            println!("\nTasks ({}):", tasks.len());
            for t in &tasks {
                let deps = if t.depends_on.is_empty() {
                    String::new()
                } else {
                    format!(" (deps: {})", t.depends_on.join(", "))
                };
                println!("  [{}] {}: {}{}", t.status, t.id, t.title, deps);
            }
        }
    } else if is_task_id(&id) {
        let task = match get_task(&flow_dir, &id) {
            Some(t) => t,
            None => {
                error_exit(&format!("Task not found: {}", id));
            }
        };

        if json {
            json_output(task_to_json(&task));
        } else {
            println!("Task: {}", task.id);
            println!("Epic: {}", task.epic);
            println!("Title: {}", task.title);
            println!("Status: {}", task.status);
            if task.domain != flowctl_core::types::Domain::General {
                println!("Domain: {}", task.domain);
            }
            let deps_str = if task.depends_on.is_empty() {
                "none".to_string()
            } else {
                task.depends_on.join(", ")
            };
            println!("Depends on: {}", deps_str);
            println!("Spec: .flow/tasks/{}.md", task.id);
        }
    } else {
        error_exit(&format!(
            "Invalid ID: {}. Expected format: fn-N or fn-N-slug (epic), fn-N.M or fn-N-slug.M (task)",
            id
        ));
    }
}

// ── Epics command ───────────────────────────────────────────────────

pub fn cmd_epics(json: bool) {
    let flow_dir = ensure_flow_exists();
    let epics = get_all_epics(&flow_dir);

    let mut epics_out: Vec<serde_json::Value> = Vec::new();
    for epic in &epics {
        let tasks = get_epic_tasks(&flow_dir, &epic.id);
        let task_count = tasks.len();
        let done_count = tasks
            .iter()
            .filter(|t| t.status == flowctl_core::state_machine::Status::Done)
            .count();

        epics_out.push(json!({
            "id": epic.id,
            "title": epic.title,
            "status": epic.status.to_string(),
            "tasks": task_count,
            "done": done_count,
        }));
    }

    if json {
        json_output(json!({
            "epics": epics_out,
            "count": epics_out.len(),
        }));
    } else if epics_out.is_empty() {
        println!("No epics found.");
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "Epics ({}):\n", epics_out.len()).ok();
        for e in &epics_out {
            let tasks = e["tasks"].as_u64().unwrap_or(0);
            let done = e["done"].as_u64().unwrap_or(0);
            let progress = if tasks > 0 {
                format!("{}/{}", done, tasks)
            } else {
                "0/0".to_string()
            };
            writeln!(
                buf,
                "  [{}] {}: {} ({} tasks done)",
                e["status"].as_str().unwrap_or(""),
                e["id"].as_str().unwrap_or(""),
                e["title"].as_str().unwrap_or(""),
                progress
            )
            .ok();
        }
        pretty_output("epics", &buf);
    }
}

// ── Tasks command ───────────────────────────────────────────────────

pub fn cmd_tasks(
    json: bool,
    epic: Option<String>,
    status: Option<String>,
    domain: Option<String>,
) {
    let flow_dir = ensure_flow_exists();

    let tasks = get_all_tasks(
        &flow_dir,
        epic.as_deref(),
        status.as_deref(),
        domain.as_deref(),
    );

    let tasks_out: Vec<serde_json::Value> = tasks.iter().map(task_list_json).collect();

    if json {
        json_output(json!({
            "tasks": tasks_out,
            "count": tasks_out.len(),
        }));
    } else if tasks_out.is_empty() {
        let scope = epic.as_ref().map(|e| format!(" for epic {}", e)).unwrap_or_default();
        let status_filter = status.as_ref().map(|s| format!(" with status '{}'", s)).unwrap_or_default();
        println!("No tasks found{}{}.", scope, status_filter);
    } else {
        use std::fmt::Write as _;
        let scope = epic.as_ref().map(|e| format!(" for {}", e)).unwrap_or_default();
        let mut buf = String::new();
        writeln!(buf, "Tasks{} ({}):\n", scope, tasks_out.len()).ok();
        for t in &tasks {
            let deps = if t.depends_on.is_empty() {
                String::new()
            } else {
                format!(" (deps: {})", t.depends_on.join(", "))
            };
            let domain_tag = if t.domain != flowctl_core::types::Domain::General {
                format!(" [{}]", t.domain)
            } else {
                String::new()
            };
            writeln!(
                buf,
                "  [{}] {}: {}{}{}",
                t.status, t.id, t.title, domain_tag, deps
            )
            .ok();
        }
        pretty_output("tasks", &buf);
    }
}

// ── List command ────────────────────────────────────────────────────

pub fn cmd_list(json: bool) {
    let flow_dir = ensure_flow_exists();
    let epics = get_all_epics(&flow_dir);
    let all_tasks = get_all_tasks(&flow_dir, None, None, None);

    // Group tasks by epic
    let mut tasks_by_epic: std::collections::HashMap<String, Vec<&Task>> =
        std::collections::HashMap::new();
    for task in &all_tasks {
        tasks_by_epic
            .entry(task.epic.clone())
            .or_default()
            .push(task);
    }

    if json {
        let epics_out: Vec<serde_json::Value> = epics
            .iter()
            .map(|e| {
                let task_list = tasks_by_epic.get(&e.id).map(|t| t.len()).unwrap_or(0);
                let done_count = tasks_by_epic
                    .get(&e.id)
                    .map(|tasks| {
                        tasks
                            .iter()
                            .filter(|t| t.status == flowctl_core::state_machine::Status::Done)
                            .count()
                    })
                    .unwrap_or(0);
                json!({
                    "id": e.id,
                    "title": e.title,
                    "status": e.status.to_string(),
                    "tasks": task_list,
                    "done": done_count,
                })
            })
            .collect();

        let tasks_out: Vec<serde_json::Value> = all_tasks
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "epic": t.epic,
                    "title": t.title,
                    "status": t.status.to_string(),
                    "priority": t.priority,
                    "depends_on": t.depends_on,
                })
            })
            .collect();

        json_output(json!({
            "epics": epics_out,
            "tasks": tasks_out,
            "epic_count": epics_out.len(),
            "task_count": tasks_out.len(),
        }));
    } else if epics.is_empty() {
        println!("No epics or tasks found.");
    } else {
        let total_tasks = all_tasks.len();
        let total_done = all_tasks
            .iter()
            .filter(|t| t.status == flowctl_core::state_machine::Status::Done)
            .count();
        println!(
            "Flow Status: {} epics, {} tasks ({} done)\n",
            epics.len(),
            total_tasks,
            total_done
        );

        for e in &epics {
            let task_list = tasks_by_epic.get(&e.id);
            let done_count = task_list
                .map(|tasks| {
                    tasks
                        .iter()
                        .filter(|t| t.status == flowctl_core::state_machine::Status::Done)
                        .count()
                })
                .unwrap_or(0);
            let task_count = task_list.map(|t| t.len()).unwrap_or(0);
            let progress = if task_count > 0 {
                format!("{}/{}", done_count, task_count)
            } else {
                "0/0".to_string()
            };

            println!(
                "[{}] {}: {} ({} done)",
                e.status, e.id, e.title, progress
            );

            if let Some(tasks) = task_list {
                for t in tasks {
                    let deps = if t.depends_on.is_empty() {
                        String::new()
                    } else {
                        format!(" (deps: {})", t.depends_on.join(", "))
                    };
                    println!(
                        "    [{}] {}: {}{}",
                        t.status, t.id, t.title, deps
                    );
                }
            }
            println!();
        }
    }
}

// ── Cat command ─────────────────────────────────────────────────────

pub fn cmd_cat(id: String) {
    let flow_dir = ensure_flow_exists();

    let spec_path = if is_epic_id(&id) {
        flow_dir.join(SPECS_DIR).join(format!("{}.md", id))
    } else if is_task_id(&id) {
        flow_dir.join(TASKS_DIR).join(format!("{}.md", id))
    } else {
        error_exit(&format!(
            "Invalid ID: {}. Expected format: fn-N or fn-N-slug (epic), fn-N.M or fn-N-slug.M (task)",
            id
        ));
    };

    match fs::read_to_string(&spec_path) {
        Ok(content) => print!("{}", content),
        Err(_) => {
            error_exit(&format!(
                "Spec not found: {}",
                spec_path.display()
            ));
        }
    }
}

// ── Stub commands (not yet ported) ──────────────────────────────────

pub fn cmd_files(json_mode: bool, epic: String) {
    let flow_dir = ensure_flow_exists();

    if !is_epic_id(&epic) {
        error_exit(&format!("Invalid epic ID: {}", epic));
    }

    let tasks = get_epic_tasks(&flow_dir, &epic);

    // Build ownership map: file -> list of task IDs
    let mut ownership: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for task in &tasks {
        let mut task_files: Vec<String> = task.files.clone();

        // Fallback: parse **Files:** from spec markdown if no structured files
        if task_files.is_empty() {
            let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task.id));
            if let Ok(spec_text) = fs::read_to_string(&spec_path) {
                for line in spec_text.lines() {
                    if let Some(rest) = line.strip_prefix("**Files:**") {
                        task_files = rest
                            .split(',')
                            .map(|f| f.trim().trim_matches('`').to_string())
                            .filter(|f| !f.is_empty())
                            .collect();
                        break;
                    }
                }
            }
        }

        for fp in task_files {
            ownership
                .entry(fp)
                .or_default()
                .push(task.id.clone());
        }
    }

    let conflicts: std::collections::BTreeMap<&String, &Vec<String>> = ownership
        .iter()
        .filter(|(_, tasks)| tasks.len() > 1)
        .collect();

    if json_mode {
        json_output(json!({
            "epic": epic,
            "ownership": ownership,
            "conflicts": conflicts,
            "file_count": ownership.len(),
            "conflict_count": conflicts.len(),
        }));
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "File ownership for {}:\n", epic).ok();
        if ownership.is_empty() {
            writeln!(buf, "  No files declared.").ok();
        } else {
            for (f, task_ids) in &ownership {
                if task_ids.len() == 1 {
                    writeln!(buf, "  {} \u{2192} {}", f, task_ids[0]).ok();
                } else {
                    writeln!(buf, "  {} \u{2192} CONFLICT: {}", f, task_ids.join(", ")).ok();
                }
            }
            if !conflicts.is_empty() {
                writeln!(
                    buf,
                    "\n  \u{26a0} {} file conflict(s) \u{2014} tasks sharing files cannot run in parallel",
                    conflicts.len()
                )
                .ok();
            }
        }
        pretty_output("files", &buf);
    }
}

// ── Lock commands (Teams mode) ─────────────────────────────────────

/// Open DB or exit with error.
fn open_db_or_exit() -> crate::commands::db_shim::Connection {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match crate::commands::db_shim::open(&cwd) {
        Ok(conn) => conn,
        Err(e) => {
            error_exit(&format!("Cannot open database: {}", e));
        }
    }
}

pub fn cmd_lock(json: bool, task: String, files: String) {
    let _flow_dir = ensure_flow_exists();

    let file_list: Vec<&str> = files.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
    if file_list.is_empty() {
        error_exit("No files specified for locking.");
    }

    let conn = open_db_or_exit();
    let repo = crate::commands::db_shim::FileLockRepo::new(&conn);

    let mut locked = Vec::new();
    let mut already_locked = Vec::new();

    for file in &file_list {
        match repo.acquire(file, &task) {
            Ok(()) => locked.push(file.to_string()),
            Err(crate::commands::db_shim::DbError::Constraint(_)) => {
                // Already locked — find out by whom
                let owner = repo.check(file).ok().flatten().unwrap_or_else(|| "unknown".to_string());
                if owner == task {
                    // Re-locking own file is fine, treat as locked
                    locked.push(file.to_string());
                } else {
                    already_locked.push(json!({"file": file, "owner": owner}));
                }
            }
            Err(e) => {
                error_exit(&format!("Failed to lock {}: {}", file, e));
            }
        }
    }

    if json {
        json_output(json!({
            "locked": locked,
            "already_locked": already_locked,
            "task": task,
        }));
    } else {
        if !locked.is_empty() {
            println!("Locked {} file(s) for task {}", locked.len(), task);
        }
        for al in &already_locked {
            println!(
                "Already locked: {} (owner: {})",
                al["file"].as_str().unwrap_or(""),
                al["owner"].as_str().unwrap_or("")
            );
        }
    }
}

pub fn cmd_unlock(json: bool, task: Option<String>, _files: Option<String>, all: bool) {
    let _flow_dir = ensure_flow_exists();
    let conn = open_db_or_exit();
    let repo = crate::commands::db_shim::FileLockRepo::new(&conn);

    if all {
        match repo.release_all() {
            Ok(count) => {
                if json {
                    json_output(json!({
                        "cleared": count,
                        "message": format!("Cleared {} file lock(s)", count),
                    }));
                } else {
                    println!("Cleared {} file lock(s)", count);
                }
            }
            Err(e) => error_exit(&format!("Failed to clear locks: {}", e)),
        }
        return;
    }

    let task_id = match task {
        Some(t) => t,
        None => {
            error_exit("--task is required (or use --all to clear all locks)");
        }
    };

    match repo.release_for_task(&task_id) {
        Ok(count) => {
            if json {
                json_output(json!({
                    "task": task_id,
                    "unlocked": count,
                    "message": format!("Released {} lock(s) for task {}", count, task_id),
                }));
            } else {
                println!("Released {} lock(s) for task {}", count, task_id);
            }
        }
        Err(e) => error_exit(&format!("Failed to unlock: {}", e)),
    }
}

pub fn cmd_lock_check(json: bool, file: Option<String>) {
    let _flow_dir = ensure_flow_exists();
    let conn = open_db_or_exit();
    let repo = crate::commands::db_shim::FileLockRepo::new(&conn);

    match file {
        Some(f) => {
            match repo.check(&f) {
                Ok(Some(owner)) => {
                    if json {
                        json_output(json!({
                            "file": f,
                            "locked": true,
                            "owner": owner,
                        }));
                    } else {
                        println!("{}: locked by {}", f, owner);
                    }
                }
                Ok(None) => {
                    if json {
                        json_output(json!({
                            "file": f,
                            "locked": false,
                        }));
                    } else {
                        println!("{}: not locked", f);
                    }
                }
                Err(e) => error_exit(&format!("Failed to check lock: {}", e)),
            }
        }
        None => {
            // List all locks
            let lock_repo = crate::commands::db_shim::FileLockRepo::new(&conn);
            let rows = lock_repo
                .list_all()
                .unwrap_or_else(|e| { error_exit(&format!("Query failed: {}", e)); });
            let locks: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(file, task_id, locked_at)| json!({
                    "file": file,
                    "task_id": task_id,
                    "locked_at": locked_at,
                }))
                .collect();

            if json {
                json_output(json!({
                    "locks": locks,
                    "count": locks.len(),
                }));
            } else if locks.is_empty() {
                println!("No file locks active.");
            } else {
                println!("Active file locks ({}):\n", locks.len());
                for l in &locks {
                    println!(
                        "  {} → {} (since {})",
                        l["file"].as_str().unwrap_or(""),
                        l["task_id"].as_str().unwrap_or(""),
                        l["locked_at"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }
}
