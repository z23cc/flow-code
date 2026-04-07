//! Status, doctor, and validate commands.

use std::env;
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

pub fn cmd_doctor(json_mode: bool, workflow: bool) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let mut checks: Vec<serde_json::Value> = Vec::new();

    // Check 1: Run validate --all internally
    let root_errors = validate_flow_root(&flow_dir);
    let mut validate_errors = root_errors.clone();

    {
        if let Ok(epics) = flowctl_core::json_store::epic_list(&flow_dir) {
            for epic in &epics {
                let (errors, _, _) = validate_epic(&flow_dir, &epic.id);
                validate_errors.extend(errors);
            }
        }
    }

    if validate_errors.is_empty() {
        checks.push(json!({"name": "validate", "status": "pass", "message": "All epics and tasks validated successfully"}));
    } else {
        checks.push(json!({"name": "validate", "status": "fail", "message": format!("Validation found {} error(s). Run 'flowctl validate --all' for details", validate_errors.len())}));
    }

    // Check 2: State-dir accessibility
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match crate::commands::db_shim::resolve_state_dir(&cwd) {
        Ok(state_dir) => {
            if let Err(e) = fs::create_dir_all(&state_dir) {
                checks.push(json!({"name": "state_dir_access", "status": "fail", "message": format!("State dir not accessible: {}", e)}));
            } else {
                // Test write access
                let test_file = state_dir.join(".doctor-probe");
                match fs::write(&test_file, "probe") {
                    Ok(_) => {
                        let _ = fs::remove_file(&test_file);
                        checks.push(json!({"name": "state_dir_access", "status": "pass", "message": format!("State dir accessible: {}", state_dir.display())}));
                    }
                    Err(e) => {
                        checks.push(json!({"name": "state_dir_access", "status": "fail", "message": format!("State dir not writable: {}", e)}));
                    }
                }
            }
        }
        Err(e) => {
            checks.push(json!({"name": "state_dir_access", "status": "fail", "message": format!("Could not resolve state dir: {}", e)}));
        }
    }

    // Check 3: Config validity
    let config_path = flow_dir.join(CONFIG_FILE);
    if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(raw_text) => match serde_json::from_str::<serde_json::Value>(&raw_text) {
                Ok(parsed) => {
                    if !parsed.is_object() {
                        checks.push(json!({"name": "config", "status": "fail", "message": "config.json is not a JSON object"}));
                    } else {
                        let known_keys: std::collections::HashSet<&str> =
                            ["memory", "notifications", "planSync", "review", "scouts", "stack"]
                                .iter()
                                .copied()
                                .collect();
                        let unknown: Vec<String> = parsed
                            .as_object()
                            .unwrap()
                            .keys()
                            .filter(|k| !known_keys.contains(k.as_str()))
                            .cloned()
                            .collect();
                        if unknown.is_empty() {
                            checks.push(json!({"name": "config", "status": "pass", "message": "config.json valid with known keys"}));
                        } else {
                            checks.push(json!({"name": "config", "status": "warn", "message": format!("Unknown config keys: {}", unknown.join(", "))}));
                        }
                    }
                }
                Err(e) => {
                    checks.push(json!({"name": "config", "status": "fail", "message": format!("config.json invalid JSON: {}", e)}));
                }
            },
            Err(e) => {
                checks.push(json!({"name": "config", "status": "warn", "message": format!("Could not read config: {}", e)}));
            }
        }
    } else {
        checks.push(json!({"name": "config", "status": "warn", "message": "config.json missing (run 'flowctl init')"}));
    }

    // Check 4: git common-dir reachability
    match Command::new("git")
        .args(["rev-parse", "--git-common-dir", "--path-format=absolute"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let common_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if Path::new(&common_dir).exists() {
                checks.push(json!({"name": "git_common_dir", "status": "pass", "message": format!("git common-dir reachable: {}", common_dir)}));
            } else {
                checks.push(json!({"name": "git_common_dir", "status": "warn", "message": format!("git common-dir path does not exist: {}", common_dir)}));
            }
        }
        Ok(_) => {
            checks.push(json!({"name": "git_common_dir", "status": "warn", "message": "Not in a git repository (git common-dir unavailable)"}));
        }
        Err(_) => {
            checks.push(json!({"name": "git_common_dir", "status": "warn", "message": "git not found on PATH"}));
        }
    }

    // Check 5-8: Workflow checks (only when --workflow flag is passed)
    if workflow {
        // Check 5: review backend configured
        let _backend_out = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });
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
                checks.push(json!({"name": "review_backend", "status": "pass", "message": format!("Review backend configured: {}", review_backend.as_ref().unwrap())}));
            }
            _ => {
                checks.push(json!({"name": "review_backend", "status": "warn", "message": "Review backend not configured. Run /flow-code:setup or set review.backend in .flow/config.json"}));
            }
        }

        // Check 6: configured backend tool available
        if let Some(ref backend) = review_backend {
            let tool = match backend.as_str() {
                "rp" => Some("rp-cli"),
                "codex" => Some("codex"),
                _ => None,
            };
            if let Some(tool_name) = tool {
                match Command::new("which").arg(tool_name).output() {
                    Ok(o) if o.status.success() => {
                        checks.push(json!({"name": "tool_available", "status": "pass", "message": format!("{} found on PATH", tool_name)}));
                    }
                    _ => {
                        checks.push(json!({"name": "tool_available", "status": "fail", "message": format!("{} not found on PATH (required by review.backend={})", tool_name, backend)}));
                    }
                }
            }
        }

        // Check 7: stale file locks (count via SQL)
        let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        if let Ok(conn) = crate::commands::db_shim::open(&cwd) {
            let lock_count = crate::commands::db_shim::block_on_pub(async {
                let mut rows = conn.inner_conn()
                    .query("SELECT COUNT(*) FROM file_locks", ())
                    .await
                    .map_err(flowctl_db::DbError::LibSql)?;
                if let Some(row) = rows.next().await.map_err(flowctl_db::DbError::LibSql)? {
                    Ok::<i64, flowctl_db::DbError>(row.get::<i64>(0).unwrap_or(0))
                } else {
                    Ok(0)
                }
            });
            match lock_count {
                Ok(n) if n > 0 => {
                    checks.push(json!({"name": "stale_locks", "status": "warn", "message": format!("{} file lock(s) active — verify with 'flowctl lock-check'", n)}));
                }
                Ok(_) => {
                    checks.push(json!({"name": "stale_locks", "status": "pass", "message": "No active file locks"}));
                }
                Err(_) => {
                    checks.push(json!({"name": "stale_locks", "status": "warn", "message": "Could not query file locks"}));
                }
            }
        }
    }

    // Build summary
    let mut summary = json!({"pass": 0, "warn": 0, "fail": 0});
    for c in &checks {
        let status = c["status"].as_str().unwrap_or("warn");
        if let Some(count) = summary.get_mut(status) {
            *count = json!(count.as_u64().unwrap_or(0) + 1);
        }
    }
    let overall_healthy = summary["fail"].as_u64().unwrap_or(0) == 0;

    if json_mode {
        json_output(json!({
            "checks": checks,
            "summary": summary,
            "healthy": overall_healthy,
        }));
    } else {
        println!("Doctor diagnostics:");
        for c in &checks {
            let icon = match c["status"].as_str().unwrap_or("warn") {
                "pass" => "OK",
                "warn" => "WARN",
                "fail" => "FAIL",
                _ => "?",
            };
            println!(
                "  [{}] {}: {}",
                icon,
                c["name"].as_str().unwrap_or(""),
                c["message"].as_str().unwrap_or("")
            );
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
