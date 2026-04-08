//! Status, doctor, and validate commands.


use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::types::{
    CONFIG_FILE, EPICS_DIR, EpicStatus, MEMORY_DIR, META_FILE, REVIEWS_DIR, SCHEMA_VERSION,
    SPECS_DIR, TASKS_DIR,
};
use flowctl_core::state_machine::Status;

use super::get_flow_dir;


// ── Status command ──────────────────────────────────────────────────

pub fn cmd_status(json: bool, interrupted: bool) {
    let flow_dir = get_flow_dir();
    let flow_exists = flow_dir.exists();

    // Handle --interrupted flag
    if interrupted {
        if !flow_exists {
            if json {
                json_output(json!({"interrupted": []}));
            } else {
                println!("No interrupted work (.flow/ not found)");
            }
            return;
        }

        let interrupted_epics = find_interrupted_epics(&flow_dir);
        if json {
            json_output(json!({
                "interrupted": interrupted_epics,
            }));
        } else if interrupted_epics.is_empty() {
            println!("No interrupted work found.");
        } else {
            println!(
                "Found {} interrupted epic(s):\n",
                interrupted_epics.len()
            );
            for ep in &interrupted_epics {
                let done = ep["done"].as_u64().unwrap_or(0);
                let total = ep["total"].as_u64().unwrap_or(0);
                let todo = ep["todo"].as_u64().unwrap_or(0);
                let in_prog = ep["in_progress"].as_u64().unwrap_or(0);
                let blocked = ep["blocked"].as_u64().unwrap_or(0);

                let mut remaining = Vec::new();
                if todo > 0 {
                    remaining.push(format!("{} todo", todo));
                }
                if in_prog > 0 {
                    remaining.push(format!("{} in_progress", in_prog));
                }
                if blocked > 0 {
                    remaining.push(format!("{} blocked", blocked));
                }

                println!("  {}: {}", ep["id"].as_str().unwrap_or(""), ep["title"].as_str().unwrap_or(""));
                println!(
                    "    Progress: {}/{} done ({})",
                    done,
                    total,
                    remaining.join(", ")
                );

                if in_prog > 0 {
                    let stale_tasks = ep.get("stale_task_ids")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    if !stale_tasks.is_empty() {
                        println!("    Recovery:");
                        for tid in &stale_tasks {
                            if let Some(id) = tid.as_str() {
                                println!("      Run: flowctl restart {id}");
                            }
                        }
                        println!(
                            "      Then: /flow-code:work {}",
                            ep["id"].as_str().unwrap_or("")
                        );
                    }
                } else {
                    println!(
                        "    Resume:   {}",
                        ep["suggested"].as_str().unwrap_or("")
                    );
                }
                println!();
            }
        }
        return;
    }

    // Count epics and tasks by status from DB (sole source of truth)
    let mut epic_counts = json!({"open": 0, "done": 0});
    let mut task_counts = json!({"todo": 0, "in_progress": 0, "blocked": 0, "done": 0});

    if flow_exists {
        if let Some(counts) = status_from_db() {
            epic_counts = counts.0;
            task_counts = counts.1;
        }
    }

    if json {
        json_output(json!({
            "flow_exists": flow_exists,
            "epics": epic_counts,
            "tasks": task_counts,
            "runs": [],
        }));
    } else if !flow_exists {
        println!(".flow/ not initialized");
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(
            buf,
            "Epics: {} open, {} done",
            epic_counts["open"], epic_counts["done"]
        )
        .ok();
        writeln!(
            buf,
            "Tasks: {} todo, {} in_progress, {} done, {} blocked",
            task_counts["todo"],
            task_counts["in_progress"],
            task_counts["done"],
            task_counts["blocked"]
        )
        .ok();
        writeln!(buf).ok();
        writeln!(buf, "No active runs").ok();
        pretty_output("status", &buf);
    }
}

/// Try to get status counts from JSON files.
fn status_from_db() -> Option<(serde_json::Value, serde_json::Value)> {
    let flow_dir = crate::commands::helpers::get_flow_dir();
    let epics = flowctl_core::json_store::epic_list(&flow_dir).ok()?;

    let mut epic_open = 0u64;
    let mut epic_done = 0u64;
    for epic in &epics {
        match epic.status {
            flowctl_core::types::EpicStatus::Open => epic_open += 1,
            flowctl_core::types::EpicStatus::Done => epic_done += 1,
        }
    }

    let tasks = flowctl_core::json_store::task_list_all(&flow_dir).ok()?;

    let mut todo = 0u64;
    let mut in_progress = 0u64;
    let mut blocked = 0u64;
    let mut done = 0u64;
    for task in &tasks {
        match task.status {
            flowctl_core::state_machine::Status::Todo => todo += 1,
            flowctl_core::state_machine::Status::InProgress => in_progress += 1,
            flowctl_core::state_machine::Status::Done => done += 1,
            flowctl_core::state_machine::Status::Blocked => blocked += 1,
            _ => {}
        }
    }

    Some((
        json!({"open": epic_open, "done": epic_done}),
        json!({"todo": todo, "in_progress": in_progress, "blocked": blocked, "done": done}),
    ))
}


/// Find open epics with undone tasks (interrupted work) from JSON files.
fn find_interrupted_epics(flow_dir: &Path) -> Vec<serde_json::Value> {
    let mut interrupted = Vec::new();

    let epics = match flowctl_core::json_store::epic_list(flow_dir) {
        Ok(e) => e,
        Err(_) => return interrupted,
    };

    for epic in epics {
        if epic.status != flowctl_core::types::EpicStatus::Open {
            continue;
        }

        let tasks = match flowctl_core::json_store::task_list_by_epic(flow_dir, &epic.id) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let mut counts = std::collections::HashMap::new();
        counts.insert("todo", 0u64);
        counts.insert("in_progress", 0u64);
        counts.insert("done", 0u64);
        counts.insert("blocked", 0u64);
        counts.insert("skipped", 0u64);
        let mut stale_task_ids: Vec<String> = Vec::new();

        for task in &tasks {
            let status_key = task.status.to_string();
            if status_key == "in_progress" {
                stale_task_ids.push(task.id.clone());
            }
            if let Some(count) = counts.get_mut(status_key.as_str()) {
                *count += 1;
            }
        }
        stale_task_ids.sort();

        let total: u64 = counts.values().sum();
        if total == 0 {
            continue;
        }

        let todo = *counts.get("todo").unwrap_or(&0);
        let in_progress = *counts.get("in_progress").unwrap_or(&0);
        let done = *counts.get("done").unwrap_or(&0);
        let blocked = *counts.get("blocked").unwrap_or(&0);
        let skipped = *counts.get("skipped").unwrap_or(&0);

        if todo > 0 || in_progress > 0 {
            interrupted.push(json!({
                "id": epic.id,
                "title": epic.title,
                "total": total,
                "done": done,
                "todo": todo,
                "in_progress": in_progress,
                "blocked": blocked,
                "skipped": skipped,
                "stale_task_ids": stale_task_ids,
                "reason": if done == 0 && in_progress == 0 { "planned_not_started" } else { "partially_complete" },
                "suggested": format!("/flow-code:work {}", epic.id),
            }));
        }
    }

    interrupted
}

// ── Validate command ────────────────────────────────────────────────

/// Validate .flow/ root invariants. Returns list of errors.
pub(super) fn validate_flow_root(flow_dir: &Path) -> Vec<String> {
    let mut errors = Vec::new();

    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        errors.push(format!("meta.json missing: {}", meta_path.display()));
    } else {
        match fs::read_to_string(&meta_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(meta) => {
                    let version = meta.get("schema_version").and_then(serde_json::Value::as_u64);
                    if version != Some(SCHEMA_VERSION as u64) {
                        errors.push(format!(
                            "schema_version unsupported in meta.json (expected {}, got {:?})",
                            SCHEMA_VERSION, version
                        ));
                    }
                }
                Err(e) => errors.push(format!("meta.json invalid JSON: {}", e)),
            },
            Err(e) => errors.push(format!("meta.json unreadable: {}", e)),
        }
    }

    for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
        if !flow_dir.join(subdir).exists() {
            errors.push(format!("Required directory missing: {}/", subdir));
        }
    }

    errors
}

/// Validate a single epic. Returns (errors, warnings, task_count).
pub(super) fn validate_epic(flow_dir: &Path, epic_id: &str) -> (Vec<String>, Vec<String>, usize) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Read tasks from JSON files
    let mut tasks: std::collections::HashMap<String, flowctl_core::types::Task> =
        std::collections::HashMap::new();

    if let Ok(task_list) = flowctl_core::json_store::task_list_by_epic(flow_dir, epic_id) {
        for task in task_list {
            tasks.insert(task.id.clone(), task);
        }
    }

    // Validate each task
    for (task_id, task) in &tasks {
        // Validate task body has required headings (read from JSON)
        {
            match flowctl_core::json_store::task_spec_read(flow_dir, task_id) {
                Ok(body) => {
                    if body.is_empty() {
                        warnings.push(format!("Task {}: no spec body", task_id));
                    } else {
                        for heading in flowctl_core::types::TASK_SPEC_HEADINGS {
                            if !body.contains(heading) {
                                errors.push(format!("Task {}: missing required heading '{}'", task_id, heading));
                            }
                        }
                    }
                }
                Err(_) => {
                    errors.push(format!("Task {}: could not read spec", task_id));
                }
            }
        }

        // Check dependencies exist and are within epic
        for dep in &task.depends_on {
            if !tasks.contains_key(dep) {
                errors.push(format!("Task {}: dependency {} not found", task_id, dep));
            }
            if !dep.starts_with(&format!("{}.", epic_id)) {
                errors.push(format!(
                    "Task {}: dependency {} is outside epic {}",
                    task_id, dep, epic_id
                ));
            }
        }
    }

    // Check for dependency cycles using DFS
    let task_ids: Vec<&String> = tasks.keys().collect();
    for start_id in &task_ids {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![start_id.as_str()];
        while let Some(current) = stack.pop() {
            if !visited.insert(current.to_string()) {
                if current == start_id.as_str() {
                    errors.push(format!("Dependency cycle detected involving {}", start_id));
                }
                continue;
            }
            if let Some(task) = tasks.get(current) {
                for dep in &task.depends_on {
                    stack.push(dep);
                }
            }
        }
    }

    let task_count = tasks.len();

    // Check epic spec exists
    let epic_spec = flow_dir.join(SPECS_DIR).join(format!("{}.md", epic_id));
    if !epic_spec.exists() {
        warnings.push(format!("Epic spec missing: {}", epic_spec.display()));
    }

    (errors, warnings, task_count)
}

pub fn cmd_validate(json_mode: bool, epic: Option<String>, all: bool) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    if epic.is_none() && !all {
        error_exit("Must specify --epic or --all");
    }

    if all {
        // Validate all epics
        let root_errors = validate_flow_root(&flow_dir);

        let mut epic_ids: Vec<String> = flowctl_core::json_store::epic_list(&flow_dir)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.id)
            .collect();
        epic_ids.sort();

        let mut all_errors: Vec<String> = root_errors.clone();
        let mut all_warnings: Vec<String> = Vec::new();
        let mut total_tasks = 0usize;
        let mut epic_results: Vec<serde_json::Value> = Vec::new();

        for eid in &epic_ids {
            let (errors, warnings, task_count) = validate_epic(&flow_dir, eid);
            all_errors.extend(errors.clone());
            all_warnings.extend(warnings.clone());
            total_tasks += task_count;
            epic_results.push(json!({
                "epic": eid,
                "valid": errors.is_empty(),
                "errors": errors,
                "warnings": warnings,
                "task_count": task_count,
            }));
        }

        let valid = all_errors.is_empty();

        if json_mode {
            json_output(json!({
                "valid": valid,
                "root_errors": root_errors,
                "epics": epic_results,
                "total_epics": epic_ids.len(),
                "total_tasks": total_tasks,
                "total_errors": all_errors.len(),
                "total_warnings": all_warnings.len(),
            }));
        } else {
            println!("Validation for all epics:");
            println!("  Epics: {}", epic_ids.len());
            println!("  Tasks: {}", total_tasks);
            println!("  Valid: {}", valid);
            if !all_errors.is_empty() {
                println!("  Errors:");
                for e in &all_errors {
                    println!("    - {}", e);
                }
            }
            if !all_warnings.is_empty() {
                println!("  Warnings:");
                for w in &all_warnings {
                    println!("    - {}", w);
                }
            }
        }

        if !valid {
            std::process::exit(1);
        }
        return;
    }

    // Single epic validation
    let epic_id = epic.unwrap();
    if !flowctl_core::id::is_epic_id(&epic_id) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic_id
        ));
    }

    let (errors, warnings, task_count) = validate_epic(&flow_dir, &epic_id);
    let valid = errors.is_empty();

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "valid": valid,
            "errors": errors,
            "warnings": warnings,
            "task_count": task_count,
        }));
    } else {
        println!("Validation for {}:", epic_id);
        println!("  Tasks: {}", task_count);
        println!("  Valid: {}", valid);
        if !errors.is_empty() {
            println!("  Errors:");
            for e in &errors {
                println!("    - {}", e);
            }
        }
        if !warnings.is_empty() {
            println!("  Warnings:");
            for w in &warnings {
                println!("    - {}", w);
            }
        }
    }

    if !valid {
        std::process::exit(1);
    }
}

// ── Doctor command ─────────────────────────────────────────────────

// ── Progress command ──────────────────────────────────────────────

pub fn cmd_progress(json_mode: bool, epic_id: Option<String>) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    // Find the epic — either from flag or auto-detect first open epic
    let epic = if let Some(id) = epic_id {
        id
    } else {
        match flowctl_core::json_store::epic_list(&flow_dir) {
            Ok(epics) => {
                epics
                    .iter()
                    .find(|e| e.status == EpicStatus::Open)
                    .map(|e| e.id.clone())
                    .unwrap_or_else(|| {
                        error_exit("No open epic found. Pass --epic <id>.");
                    })
            }
            Err(_) => error_exit("Cannot read epics."),
        }
    };

    // Load tasks for this epic
    let tasks = match flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic) {
        Ok(t) => t,
        Err(_) => {
            error_exit(&format!("Cannot load tasks for epic {epic}"));
        }
    };

    let total = tasks.len();
    let done = tasks.iter().filter(|t| t.status == Status::Done || t.status == Status::Skipped).count();
    let in_progress: Vec<&str> = tasks.iter().filter(|t| t.status == Status::InProgress).map(|t| t.id.as_str()).collect();
    let blocked = tasks.iter().filter(|t| t.status == Status::Blocked).count();
    let todo = tasks.iter().filter(|t| t.status == Status::Todo).count();
    let failed = tasks.iter().filter(|t| t.status == Status::Failed || t.status == Status::UpstreamFailed).count();

    // Estimate wave: count how many distinct "rounds" have completed
    // Simple heuristic: wave = number of tasks that are done/in_progress groups
    // Wave estimation reserved for future use
    let percent = if total > 0 { (done * 100) / total } else { 0 };

    if json_mode {
        json_output(json!({
            "epic": epic,
            "tasks_total": total,
            "tasks_done": done,
            "tasks_in_progress": in_progress,
            "tasks_blocked": blocked,
            "tasks_todo": todo,
            "tasks_failed": failed,
            "percent": percent,
        }));
    } else {
        // Progress bar using Unicode blocks
        let bar_width = 30;
        let filled = (percent * bar_width) / 100;
        let empty = bar_width - filled;
        let bar: String = "\u{2588}".repeat(filled) + &"\u{2591}".repeat(empty);

        println!("Epic: {epic}");
        println!("Tasks: {done}/{total} done, {} in progress, {} todo, {} blocked, {} failed",
            in_progress.len(), todo, blocked, failed);
        if !in_progress.is_empty() {
            println!("Active: [{}]", in_progress.join(", "));
        }
        println!("{bar} {percent}%");
    }
}

// ── Doctor helpers ─────────────────────────────────────────────────

/// Check if an external tool is available via `which` crate, return (status, path_or_none).
fn check_tool(name: &str) -> (String, Option<String>) {
    match which::which(name) {
        Ok(path) => ("ok".to_string(), Some(path.to_string_lossy().to_string())),
        Err(_) => ("missing".to_string(), None),
    }
}

/// Run a command and return trimmed stdout, or None on failure.
fn run_cmd(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Detect stale locks: locks held by tasks that are done/failed/blocked.
fn count_stale_locks(flow_dir: &Path) -> (usize, usize) {
    let locks = match flowctl_core::json_store::locks_read(flow_dir) {
        Ok(l) => l,
        Err(_) => return (0, 0),
    };
    let total = locks.len();
    let mut stale = 0usize;
    let tasks = flowctl_core::json_store::task_list_all(flow_dir).unwrap_or_default();
    let task_map: std::collections::HashMap<&str, &flowctl_core::state_machine::Status> =
        tasks.iter().map(|t| (t.id.as_str(), &t.status)).collect();
    for lock in &locks {
        match task_map.get(lock.task_id.as_str()) {
            Some(Status::Done) | Some(Status::Blocked) | Some(Status::Failed)
            | Some(Status::UpstreamFailed) | Some(Status::Skipped) | None => {
                stale += 1;
            }
            _ => {}
        }
    }
    (total, stale)
}

/// Count orphaned tasks (tasks whose epic doesn't exist).
fn count_orphaned_tasks(flow_dir: &Path) -> usize {
    let epics: std::collections::HashSet<String> = flowctl_core::json_store::epic_list(flow_dir)
        .unwrap_or_default()
        .into_iter()
        .map(|e| e.id)
        .collect();
    let tasks = flowctl_core::json_store::task_list_all(flow_dir).unwrap_or_default();
    tasks
        .iter()
        .filter(|t| {
            // Epic ID is the part before the last dot: fn-1.2 -> fn-1
            let epic_id = t.id.rsplitn(2, '.').nth(1).unwrap_or("");
            !epics.contains(epic_id)
        })
        .count()
}

pub fn cmd_doctor(json_mode: bool, workflow: bool) {
    let flow_dir = get_flow_dir();

    // Structured results for JSON mode
    let mut result = json!({});
    // Flat check list (preserved from original for summary)
    let mut checks: Vec<serde_json::Value> = Vec::new();

    // ── 1. Binary info ────────────────────────────────────────────
    let version = env!("CARGO_PKG_VERSION");
    let binary_path = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    result["binary"] = json!({
        "version": version,
        "path": binary_path,
    });
    checks.push(json!({"name": "binary", "status": "pass",
        "message": format!("v{} at {}", version, binary_path)}));

    // ── 2. .flow/ directory ───────────────────────────────────────
    let flow_exists = flow_dir.exists();
    let flow_writable = if flow_exists {
        let probe = flow_dir.join(".doctor-probe");
        let w = fs::write(&probe, "probe").is_ok();
        let _ = fs::remove_file(&probe);
        w
    } else {
        false
    };

    let expected_subdirs = ["epics", "tasks", "specs", "memory", "reviews", "checklists", "index"];
    let mut present_subdirs: Vec<String> = Vec::new();
    let mut missing_subdirs: Vec<String> = Vec::new();
    for sub in &expected_subdirs {
        if flow_dir.join(sub).exists() {
            present_subdirs.push(sub.to_string());
        } else {
            missing_subdirs.push(sub.to_string());
        }
    }

    result["flow_dir"] = json!({
        "exists": flow_exists,
        "writable": flow_writable,
        "subdirs": present_subdirs,
        "missing_subdirs": missing_subdirs,
    });

    if !flow_exists {
        checks.push(json!({"name": "flow_dir", "status": "fail",
            "message": ".flow/ does not exist. Run 'flowctl init' first."}));
    } else if !flow_writable {
        checks.push(json!({"name": "flow_dir", "status": "fail",
            "message": ".flow/ exists but is not writable"}));
    } else if !missing_subdirs.is_empty() {
        checks.push(json!({"name": "flow_dir", "status": "warn",
            "message": format!("exists, writable, missing subdirs: {}", missing_subdirs.join(", "))}));
    } else {
        checks.push(json!({"name": "flow_dir", "status": "pass",
            "message": "exists, writable"}));
    }

    // ── 3. Review backends ────────────────────────────────────────
    let (rp_status, rp_path) = check_tool("rp-cli");
    let (codex_status, codex_path) = check_tool("codex");
    result["review_backends"] = json!({
        "rp_cli": {"status": rp_status, "path": rp_path},
        "codex_cli": {"status": codex_status, "path": codex_path},
    });
    {
        let rp_icon = if rp_status == "ok" { "ok" } else { "missing" };
        let codex_icon = if codex_status == "ok" { "ok" } else { "missing" };
        let status = if rp_status == "ok" || codex_status == "ok" { "pass" } else { "warn" };
        checks.push(json!({"name": "review_backends", "status": status,
            "message": format!("rp-cli {} codex {}", rp_icon, codex_icon)}));
    }

    // ── 4. Git status ─────────────────────────────────────────────
    let is_repo = run_cmd("git", &["rev-parse", "--is-inside-work-tree"])
        .map(|s| s == "true")
        .unwrap_or(false);
    let branch = run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default();
    let uncommitted_count = run_cmd("git", &["status", "--porcelain"])
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);
    let clean = uncommitted_count == 0;

    result["git"] = json!({
        "is_repo": is_repo,
        "branch": branch,
        "clean": clean,
        "uncommitted_count": uncommitted_count,
    });
    if !is_repo {
        checks.push(json!({"name": "git", "status": "warn",
            "message": "not a git repository"}));
    } else if !clean {
        checks.push(json!({"name": "git", "status": "warn",
            "message": format!("{}, {} uncommitted file(s)", branch, uncommitted_count)}));
    } else {
        checks.push(json!({"name": "git", "status": "pass",
            "message": format!("{}, clean", branch)}));
    }

    // ── 5. State integrity ────────────────────────────────────────
    let epics_count = if flow_exists {
        flowctl_core::json_store::epic_list(&flow_dir)
            .map(|e| e.len())
            .unwrap_or(0)
    } else {
        0
    };
    let tasks_count = if flow_exists {
        flowctl_core::json_store::task_list_all(&flow_dir)
            .map(|t| t.len())
            .unwrap_or(0)
    } else {
        0
    };
    let orphaned_tasks = if flow_exists { count_orphaned_tasks(&flow_dir) } else { 0 };
    let (total_locks, stale_locks) = if flow_exists { count_stale_locks(&flow_dir) } else { (0, 0) };

    result["state_integrity"] = json!({
        "epics_count": epics_count,
        "tasks_count": tasks_count,
        "orphaned_tasks": orphaned_tasks,
        "total_locks": total_locks,
        "stale_locks": stale_locks,
    });
    {
        let status = if orphaned_tasks > 0 || stale_locks > 0 { "warn" } else { "pass" };
        let mut parts = vec![
            format!("{} epic(s)", epics_count),
            format!("{} task(s)", tasks_count),
            format!("{} orphaned", orphaned_tasks),
        ];
        if stale_locks > 0 {
            parts.push(format!("{} stale lock(s)", stale_locks));
        }
        checks.push(json!({"name": "state_integrity", "status": status,
            "message": parts.join(", ")}));
    }

    // ── 6. Project context ────────────────────────────────────────
    let project_context_path = flow_dir.join("project-context.md");
    let pc_exists = project_context_path.exists();
    result["project_context"] = json!({
        "exists": pc_exists,
        "path": ".flow/project-context.md",
    });
    if pc_exists {
        checks.push(json!({"name": "project_context", "status": "pass",
            "message": ".flow/project-context.md"}));
    } else {
        checks.push(json!({"name": "project_context", "status": "warn",
            "message": "project-context.md missing (optional but recommended)"}));
    }

    // ── 7. Search tools ───────────────────────────────────────────
    let ngram_path = flow_dir.join("index").join("ngram.bin");
    let ngram_status;
    let mut ngram_file_count = 0usize;
    if ngram_path.exists() {
        // Check if the index is recent (< 24h old)
        let stale = fs::metadata(&ngram_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
            .is_some_and(|d| d.as_secs() > 86400);
        if stale {
            ngram_status = "stale";
        } else {
            ngram_status = "ok";
        }
        // Estimate file count from index size (rough heuristic)
        ngram_file_count = fs::metadata(&ngram_path)
            .map(|m| (m.len() / 100).max(1) as usize)
            .unwrap_or(0);
    } else {
        ngram_status = "missing";
    }

    let frecency_path = flow_dir.join("frecency.json");
    let frecency_status;
    let mut frecency_entry_count = 0usize;
    if frecency_path.exists() {
        match fs::read_to_string(&frecency_path) {
            Ok(content) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    frecency_entry_count = val.as_object().map(|o| o.len()).unwrap_or(0);
                    frecency_status = if frecency_entry_count == 0 { "empty" } else { "ok" };
                } else {
                    frecency_status = "empty";
                }
            }
            Err(_) => {
                frecency_status = "empty";
            }
        }
    } else {
        frecency_status = "missing";
    }

    result["search_tools"] = json!({
        "ngram_index": {"status": ngram_status, "file_count": ngram_file_count},
        "frecency": {"status": frecency_status, "entry_count": frecency_entry_count},
    });
    {
        let ngram_msg = match ngram_status {
            "ok" => format!("index ok (~{} files)", ngram_file_count),
            "stale" => "index stale".to_string(),
            _ => "index missing".to_string(),
        };
        let frec_msg = match frecency_status {
            "ok" => format!("frecency {} entries", frecency_entry_count),
            "empty" => "frecency empty".to_string(),
            _ => "frecency missing".to_string(),
        };
        let status = if ngram_status == "ok" && frecency_status == "ok" {
            "pass"
        } else if ngram_status == "missing" && frecency_status != "ok" {
            "warn"
        } else {
            "warn"
        };
        checks.push(json!({"name": "search_tools", "status": status,
            "message": format!("{} | {}", ngram_msg, frec_msg)}));
    }

    // ── 8. External tools ─────────────────────────────────────────
    let git_version = run_cmd("git", &["--version"])
        .map(|s| s.replace("git version ", ""))
        .unwrap_or_default();
    let (git_tool_status, _) = if git_version.is_empty() {
        ("missing".to_string(), None)
    } else {
        ("ok".to_string(), Some(git_version.clone()))
    };

    let external_tools = ["jq", "gh", "rg"];
    let mut ext_results = json!({
        "git": {"status": git_tool_status, "version": git_version},
    });
    let mut ext_parts = vec![format!("git {}", if git_tool_status == "ok" { "ok" } else { "missing" })];

    for tool_name in &external_tools {
        let (st, _path) = check_tool(tool_name);
        ext_results[*tool_name] = json!({"status": st});
        ext_parts.push(format!("{} {}", tool_name, st));
    }
    result["external_tools"] = ext_results;
    {
        let all_ok = ext_parts.iter().all(|p| p.ends_with("ok"));
        let status = if all_ok { "pass" } else { "warn" };
        checks.push(json!({"name": "external_tools", "status": status,
            "message": ext_parts.join("  ")}));
    }

    // ── 9. Original checks: validate, state-dir, config, git-common-dir ──
    // Validate
    if flow_exists {
        let root_errors = validate_flow_root(&flow_dir);
        let mut validate_errors = root_errors;
        if let Ok(epics) = flowctl_core::json_store::epic_list(&flow_dir) {
            for epic in &epics {
                let (errors, _, _) = validate_epic(&flow_dir, &epic.id);
                validate_errors.extend(errors);
            }
        }
        if validate_errors.is_empty() {
            checks.push(json!({"name": "validate", "status": "pass",
                "message": "All epics and tasks validated successfully"}));
        } else {
            checks.push(json!({"name": "validate", "status": "fail",
                "message": format!("Validation found {} error(s). Run 'flowctl validate --all' for details",
                    validate_errors.len())}));
        }
    }

    // Config validity
    if flow_exists {
        let config_path = flow_dir.join(CONFIG_FILE);
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(raw_text) => match serde_json::from_str::<serde_json::Value>(&raw_text) {
                    Ok(parsed) => {
                        if !parsed.is_object() {
                            checks.push(json!({"name": "config", "status": "fail",
                                "message": "config.json is not a JSON object"}));
                        } else {
                            let known_keys: std::collections::HashSet<&str> =
                                ["memory", "notifications", "planSync", "review", "scouts", "stack", "outputs", "worker"]
                                    .iter().copied().collect();
                            let unknown: Vec<String> = parsed.as_object().unwrap()
                                .keys().filter(|k| !known_keys.contains(k.as_str()))
                                .cloned().collect();
                            if unknown.is_empty() {
                                checks.push(json!({"name": "config", "status": "pass",
                                    "message": "config.json valid with known keys"}));
                            } else {
                                checks.push(json!({"name": "config", "status": "warn",
                                    "message": format!("Unknown config keys: {}", unknown.join(", "))}));
                            }
                        }
                    }
                    Err(e) => {
                        checks.push(json!({"name": "config", "status": "fail",
                            "message": format!("config.json invalid JSON: {}", e)}));
                    }
                },
                Err(e) => {
                    checks.push(json!({"name": "config", "status": "warn",
                        "message": format!("Could not read config: {}", e)}));
                }
            }
        } else {
            checks.push(json!({"name": "config", "status": "warn",
                "message": "config.json missing (run 'flowctl init')"}));
        }
    }

    // ── 10. Workflow checks (only when --workflow flag is passed) ──
    if workflow {
        // Review backend configured
        let config_path = flow_dir.join(CONFIG_FILE);
        let review_backend = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| v["review"]["backend"].as_str().map(String::from))
        } else {
            None
        };
        match review_backend.as_deref() {
            Some("rp") | Some("codex") | Some("none") => {
                checks.push(json!({"name": "review_backend", "status": "pass",
                    "message": format!("Review backend configured: {}",
                        review_backend.as_ref().unwrap())}));
            }
            _ => {
                checks.push(json!({"name": "review_backend", "status": "warn",
                    "message": "Review backend not configured. Run /flow-code:setup or set review.backend in .flow/config.json"}));
            }
        }

        // Configured backend tool available
        if let Some(ref backend) = review_backend {
            let tool = match backend.as_str() {
                "rp" => Some("rp-cli"),
                "codex" => Some("codex"),
                _ => None,
            };
            if let Some(tool_name) = tool {
                let (st, _) = check_tool(tool_name);
                if st == "ok" {
                    checks.push(json!({"name": "tool_available", "status": "pass",
                        "message": format!("{} found on PATH", tool_name)}));
                } else {
                    checks.push(json!({"name": "tool_available", "status": "fail",
                        "message": format!("{} not found on PATH (required by review.backend={})",
                            tool_name, backend)}));
                }
            }
        }

        // Stale file locks (detailed)
        if stale_locks > 0 {
            checks.push(json!({"name": "stale_locks", "status": "warn",
                "message": format!("{} stale lock(s) of {} total -- run 'flowctl unlock --all' to clear",
                    stale_locks, total_locks)}));
        } else if total_locks > 0 {
            checks.push(json!({"name": "stale_locks", "status": "pass",
                "message": format!("{} active lock(s), none stale", total_locks)}));
        } else {
            checks.push(json!({"name": "stale_locks", "status": "pass",
                "message": "No active file locks"}));
        }
    }

    // ── Build summary ─────────────────────────────────────────────
    let mut summary = json!({"pass": 0, "warn": 0, "fail": 0});
    for c in &checks {
        let status = c["status"].as_str().unwrap_or("warn");
        if let Some(count) = summary.get_mut(status) {
            *count = json!(count.as_u64().unwrap_or(0) + 1);
        }
    }
    let overall_healthy = summary["fail"].as_u64().unwrap_or(0) == 0;

    if json_mode {
        result["checks"] = json!(checks);
        result["summary"] = summary.clone();
        result["healthy"] = json!(overall_healthy);
        json_output(result);
    } else {
        println!("flowctl doctor");
        // Pretty grouped output
        for c in &checks {
            let icon = match c["status"].as_str().unwrap_or("warn") {
                "pass" => "\u{2713}",   // checkmark
                "warn" => "\u{26a0}",   // warning
                "fail" => "\u{2717}",   // cross
                _ => "?",
            };
            let name = c["name"].as_str().unwrap_or("");
            let msg = c["message"].as_str().unwrap_or("");
            // Pad name to 18 chars for alignment
            println!("  {:<18} {} {}", format!("{}:", name), msg, icon);
        }
        println!();
        println!(
            "Summary: {} pass, {} warn, {} fail",
            summary["pass"], summary["warn"], summary["fail"]
        );
        if !overall_healthy {
            println!("Health check FAILED \u{2014} resolve fail items above.");
        }
    }

    if !overall_healthy {
        std::process::exit(1);
    }
}
