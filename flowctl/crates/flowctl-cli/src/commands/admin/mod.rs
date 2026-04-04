//! Admin commands: init, detect, status, doctor, validate, state-path, migrate-state,
//! review-backend, parse-findings, guard, worker-prompt, config.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, stub};

use flowctl_core::types::{
    CONFIG_FILE, EPICS_DIR, FLOW_DIR, MEMORY_DIR, META_FILE, REVIEWS_DIR, SCHEMA_VERSION,
    SPECS_DIR, TASKS_DIR,
};

// ── Helpers ─────────────────────────────────────────────────────────

/// Get the .flow/ directory path (current directory + .flow/).
fn get_flow_dir() -> std::path::PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(FLOW_DIR)
}

/// Default config structure matching Python's get_default_config().
fn get_default_config() -> serde_json::Value {
    json!({
        "memory": {"enabled": true},
        "planSync": {"enabled": true, "crossEpic": false},
        "review": {"backend": null},
        "scouts": {"github": false},
        "stack": {},
    })
}

/// Deep merge: override values win for conflicts.
fn deep_merge(base: &serde_json::Value, over: &serde_json::Value) -> serde_json::Value {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            let mut result = b.clone();
            for (key, value) in o {
                if let Some(base_val) = result.get(key) {
                    result.insert(key.clone(), deep_merge(base_val, value));
                } else {
                    result.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        (_, over_val) => over_val.clone(),
    }
}

/// Resolve current actor: FLOW_ACTOR env > git config user.email > git config user.name > $USER > "unknown"
#[allow(dead_code)]
pub fn resolve_actor() -> String {
    if let Ok(actor) = env::var("FLOW_ACTOR") {
        let trimmed = actor.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    if let Ok(output) = Command::new("git")
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

    if let Ok(output) = Command::new("git")
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

// ── Init command ────────────────────────────────────────────────────

pub fn cmd_init(json: bool) {
    let flow_dir = get_flow_dir();
    let mut actions: Vec<String> = Vec::new();

    // Create directories if missing (idempotent, never destroys existing)
    for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
        let dir_path = flow_dir.join(subdir);
        if !dir_path.exists() {
            if let Err(e) = fs::create_dir_all(&dir_path) {
                error_exit(&format!("Failed to create {}: {}", dir_path.display(), e));
            }
            actions.push(format!("created {}/", subdir));
        }
    }

    // Create meta.json if missing (never overwrite existing)
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        let meta = json!({
            "schema_version": SCHEMA_VERSION,
            "next_epic": 1
        });
        write_json_file(&meta_path, &meta);
        actions.push("created meta.json".to_string());
    }

    // Config: create or upgrade (merge missing defaults)
    let config_path = flow_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        write_json_file(&config_path, &get_default_config());
        actions.push("created config.json".to_string());
    } else {
        // Load raw config, compare with merged (which includes new defaults)
        let raw = match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(json!({})),
            Err(_) => json!({}),
        };
        let merged = deep_merge(&get_default_config(), &raw);
        if merged != raw {
            write_json_file(&config_path, &merged);
            actions.push("upgraded config.json (added missing keys)".to_string());
        }
    }

    // Build output
    let message = if actions.is_empty() {
        ".flow/ already up to date".to_string()
    } else {
        format!(".flow/ updated: {}", actions.join(", "))
    };

    if json {
        json_output(json!({
            "message": message,
            "path": flow_dir.to_string_lossy(),
            "actions": actions,
        }));
    } else {
        println!("{}", message);
    }
}

/// Write JSON to a file with pretty formatting.
fn write_json_file(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(value).unwrap();
    if let Err(e) = fs::write(path, &content) {
        error_exit(&format!("Failed to write {}: {}", path.display(), e));
    }
}

// ── Detect command ──────────────────────────────────────────────────

pub fn cmd_detect(json: bool) {
    let flow_dir = get_flow_dir();
    let exists = flow_dir.exists();
    let mut issues: Vec<String> = Vec::new();

    if exists {
        let meta_path = flow_dir.join(META_FILE);
        if !meta_path.exists() {
            issues.push("meta.json missing".to_string());
        } else {
            match fs::read_to_string(&meta_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(meta) => {
                        let version = meta.get("schema_version").and_then(|v| v.as_u64());
                        if version != Some(SCHEMA_VERSION as u64) {
                            issues.push(format!(
                                "schema_version unsupported (expected {}, got {:?})",
                                SCHEMA_VERSION, version
                            ));
                        }
                    }
                    Err(e) => issues.push(format!("meta.json parse error: {}", e)),
                },
                Err(e) => issues.push(format!("meta.json unreadable: {}", e)),
            }
        }

        for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
            if !flow_dir.join(subdir).exists() {
                issues.push(format!("{}/ missing", subdir));
            }
        }
    }

    let valid = exists && issues.is_empty();

    if json {
        json_output(json!({
            "exists": exists,
            "valid": valid,
            "issues": issues,
            "path": flow_dir.to_string_lossy(),
        }));
    } else if exists {
        if valid {
            println!(".flow/ exists and is valid");
        } else {
            println!(".flow/ exists but has issues:");
            for issue in &issues {
                println!("  - {}", issue);
            }
        }
    } else {
        println!(".flow/ not found");
    }
}

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
            json_output(json!({"interrupted": interrupted_epics}));
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
                println!(
                    "    Resume:   {}",
                    ep["suggested"].as_str().unwrap_or("")
                );
                println!();
            }
        }
        return;
    }

    // Count epics and tasks by status using Markdown scanning
    let mut epic_counts = json!({"open": 0, "done": 0});
    let mut task_counts = json!({"todo": 0, "in_progress": 0, "blocked": 0, "done": 0});

    if flow_exists {
        let epics_dir = flow_dir.join(EPICS_DIR);
        let tasks_dir = flow_dir.join(TASKS_DIR);

        // Try DB first, fall back to Markdown
        if let Some(counts) = status_from_db() {
            epic_counts = counts.0;
            task_counts = counts.1;
        } else {
            // Scan Markdown files
            if epics_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&epics_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("md") {
                            continue;
                        }
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if !flowctl_core::id::is_epic_id(stem) {
                            continue;
                        }
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(epic) =
                                flowctl_core::frontmatter::parse_frontmatter::<flowctl_core::types::Epic>(&content)
                            {
                                let key = epic.status.to_string();
                                if let Some(count) = epic_counts.get_mut(&key) {
                                    *count = json!(count.as_u64().unwrap_or(0) + 1);
                                }
                            }
                        }
                    }
                }
            }

            if tasks_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&tasks_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) != Some("md") {
                            continue;
                        }
                        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if !flowctl_core::id::is_task_id(stem) {
                            continue;
                        }
                        if let Ok(content) = fs::read_to_string(&path) {
                            if let Ok(task) =
                                flowctl_core::frontmatter::parse_frontmatter::<flowctl_core::types::Task>(&content)
                            {
                                let key = task.status.to_string();
                                if let Some(count) = task_counts.get_mut(&key) {
                                    *count = json!(count.as_u64().unwrap_or(0) + 1);
                                }
                            }
                        }
                    }
                }
            }
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
        println!(
            "Epics: {} open, {} done",
            epic_counts["open"], epic_counts["done"]
        );
        println!(
            "Tasks: {} todo, {} in_progress, {} done, {} blocked",
            task_counts["todo"],
            task_counts["in_progress"],
            task_counts["done"],
            task_counts["blocked"]
        );
        println!();
        println!("No active runs");
    }
}

/// Try to get status counts from SQLite database.
fn status_from_db() -> Option<(serde_json::Value, serde_json::Value)> {
    let cwd = env::current_dir().ok()?;
    let conn = flowctl_db::open(&cwd).ok()?;

    let epic_repo = flowctl_db::EpicRepo::new(&conn);
    let epics = epic_repo.list(None).ok()?;

    let mut epic_open = 0u64;
    let mut epic_done = 0u64;
    for epic in &epics {
        match epic.status {
            flowctl_core::types::EpicStatus::Open => epic_open += 1,
            flowctl_core::types::EpicStatus::Done => epic_done += 1,
        }
    }

    // Check if there are actually any epics/tasks indexed
    if epics.is_empty() {
        // DB might be empty (not yet indexed), fall back to Markdown
        return None;
    }

    let task_repo = flowctl_db::TaskRepo::new(&conn);
    let tasks = task_repo.list_all(None, None).ok()?;

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

/// Find open epics with undone tasks (interrupted work).
fn find_interrupted_epics(flow_dir: &Path) -> Vec<serde_json::Value> {
    let mut interrupted = Vec::new();
    let epics_dir = flow_dir.join(EPICS_DIR);
    let tasks_dir = flow_dir.join(TASKS_DIR);

    if !epics_dir.is_dir() {
        return interrupted;
    }

    // Collect all epics
    let mut epic_entries: Vec<_> = match fs::read_dir(&epics_dir) {
        Ok(entries) => entries.flatten().collect(),
        Err(_) => return interrupted,
    };
    epic_entries.sort_by_key(|e| e.path());

    for entry in epic_entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !flowctl_core::id::is_epic_id(stem) {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let epic = match flowctl_core::frontmatter::parse_frontmatter::<flowctl_core::types::Epic>(&content) {
            Ok(e) => e,
            Err(_) => continue,
        };

        if epic.status != flowctl_core::types::EpicStatus::Open {
            continue;
        }

        // Count tasks for this epic
        let mut counts = std::collections::HashMap::new();
        counts.insert("todo", 0u64);
        counts.insert("in_progress", 0u64);
        counts.insert("done", 0u64);
        counts.insert("blocked", 0u64);
        counts.insert("skipped", 0u64);

        if tasks_dir.is_dir() {
            if let Ok(task_entries) = fs::read_dir(&tasks_dir) {
                for task_entry in task_entries.flatten() {
                    let task_path = task_entry.path();
                    if task_path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    let task_stem = task_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if !flowctl_core::id::is_task_id(task_stem) {
                        continue;
                    }
                    if let Ok(task_content) = fs::read_to_string(&task_path) {
                        if let Ok(task) =
                            flowctl_core::frontmatter::parse_frontmatter::<flowctl_core::types::Task>(&task_content)
                        {
                            if task.epic != epic.id {
                                continue;
                            }
                            let status_key = task.status.to_string();
                            if let Some(count) = counts.get_mut(status_key.as_str()) {
                                *count += 1;
                            }
                        }
                    }
                }
            }
        }

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
                "reason": if done == 0 && in_progress == 0 { "planned_not_started" } else { "partially_complete" },
                "suggested": format!("/flow-code:work {}", epic.id),
            }));
        }
    }

    interrupted
}

// ── Validate command ────────────────────────────────────────────────

/// Validate .flow/ root invariants. Returns list of errors.
fn validate_flow_root(flow_dir: &Path) -> Vec<String> {
    let mut errors = Vec::new();

    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        errors.push(format!("meta.json missing: {}", meta_path.display()));
    } else {
        match fs::read_to_string(&meta_path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(meta) => {
                    let version = meta.get("schema_version").and_then(|v| v.as_u64());
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
fn validate_epic(flow_dir: &Path, epic_id: &str) -> (Vec<String>, Vec<String>, usize) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let tasks_dir = flow_dir.join(TASKS_DIR);

    // Scan tasks for this epic from Markdown frontmatter
    let mut tasks: std::collections::HashMap<String, flowctl_core::types::Task> =
        std::collections::HashMap::new();

    if tasks_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&tasks_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !flowctl_core::id::is_task_id(stem) {
                    continue;
                }
                // Check if task belongs to this epic
                if !stem.starts_with(&format!("{}.", epic_id)) {
                    continue;
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    match flowctl_core::frontmatter::parse_frontmatter::<flowctl_core::types::Task>(
                        &content,
                    ) {
                        Ok(task) => {
                            tasks.insert(task.id.clone(), task);
                        }
                        Err(e) => {
                            let path_str = path.display().to_string();
                            errors.push(format!("Task {}: frontmatter parse error: {}", stem, e));
                            crate::diagnostics::report_frontmatter_error(
                                &path_str,
                                &content,
                                &e.to_string(),
                            );
                        }
                    }
                }
            }
        }
    }

    // Validate each task
    for (task_id, task) in &tasks {
        // Check task spec exists
        let task_spec_path = tasks_dir.join(format!("{}.md", task_id));
        if !task_spec_path.exists() {
            errors.push(format!("Task spec missing: {}", task_spec_path.display()));
        } else if let Ok(spec_content) = fs::read_to_string(&task_spec_path) {
            // Validate required headings
            for heading in flowctl_core::types::TASK_SPEC_HEADINGS {
                if !spec_content.contains(heading) {
                    errors.push(format!("Task {}: missing required heading '{}'", task_id, heading));
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
        let epics_dir = flow_dir.join(EPICS_DIR);

        let mut epic_ids: Vec<String> = Vec::new();
        if epics_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&epics_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("md") {
                        continue;
                    }
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if flowctl_core::id::is_epic_id(stem) {
                        epic_ids.push(stem.to_string());
                    }
                }
            }
        }
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

pub fn cmd_doctor(json_mode: bool) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let mut checks: Vec<serde_json::Value> = Vec::new();

    // Check 1: Run validate --all internally
    let root_errors = validate_flow_root(&flow_dir);
    let epics_dir = flow_dir.join(EPICS_DIR);
    let mut validate_errors = root_errors.clone();

    if epics_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&epics_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if flowctl_core::id::is_epic_id(stem) {
                    let (errors, _, _) = validate_epic(&flow_dir, stem);
                    validate_errors.extend(errors);
                }
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
    match flowctl_db::resolve_state_dir(&cwd) {
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
                            ["memory", "planSync", "review", "scouts", "stack"]
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

// ── State-path command ─────────────────────────────────────────────

pub fn cmd_state_path(json_mode: bool, task: Option<String>) {
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let state_dir = match flowctl_db::resolve_state_dir(&cwd) {
        Ok(d) => d,
        Err(e) => {
            error_exit(&format!("Could not resolve state dir: {}", e));
        }
    };

    if let Some(task_id) = task {
        if !flowctl_core::id::is_task_id(&task_id) {
            error_exit(&format!(
                "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
                task_id
            ));
        }
        let state_path = state_dir.join("tasks").join(format!("{}.state.json", task_id));
        if json_mode {
            json_output(json!({
                "state_dir": state_dir.to_string_lossy(),
                "task_state_path": state_path.to_string_lossy(),
            }));
        } else {
            println!("{}", state_path.display());
        }
    } else if json_mode {
        json_output(json!({"state_dir": state_dir.to_string_lossy()}));
    } else {
        println!("{}", state_dir.display());
    }
}

// ── Migrate-state command (stub - complex migration logic) ─────────

pub fn cmd_migrate_state(json: bool, clean: bool) {
    let _ = clean;
    stub("migrate-state", json);
}

// ── Review-backend command ─────────────────────────────────────────

pub fn cmd_review_backend(json_mode: bool, compare: Option<String>, epic: Option<String>) {
    // Priority: FLOW_REVIEW_BACKEND env > config > ASK
    let (backend, source) = if let Ok(env_val) = env::var("FLOW_REVIEW_BACKEND") {
        let trimmed = env_val.trim().to_string();
        if ["rp", "codex", "none"].contains(&trimmed.as_str()) {
            (trimmed, "env".to_string())
        } else {
            ("ASK".to_string(), "none".to_string())
        }
    } else {
        let flow_dir = get_flow_dir();
        if flow_dir.exists() {
            let config_path = flow_dir.join(CONFIG_FILE);
            let config = if config_path.exists() {
                match fs::read_to_string(&config_path) {
                    Ok(content) => {
                        let raw = serde_json::from_str::<serde_json::Value>(&content)
                            .unwrap_or(json!({}));
                        deep_merge(&get_default_config(), &raw)
                    }
                    Err(_) => get_default_config(),
                }
            } else {
                get_default_config()
            };

            let cfg_val = config
                .pointer("/review/backend")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if ["rp", "codex", "none"].contains(&cfg_val) {
                (cfg_val.to_string(), "config".to_string())
            } else {
                ("ASK".to_string(), "none".to_string())
            }
        } else {
            ("ASK".to_string(), "none".to_string())
        }
    };

    // --compare mode: compare review receipt files
    let receipt_files: Option<Vec<String>> = if let Some(epic_id) = &epic {
        if compare.is_none() {
            let flow_dir = get_flow_dir();
            let reviews_dir = flow_dir.join(REVIEWS_DIR);
            if !reviews_dir.exists() {
                if json_mode {
                    json_output(json!({"backend": backend, "source": source}));
                } else {
                    println!("{}", backend);
                }
                return;
            }
            let mut files: Vec<String> = Vec::new();
            if let Ok(entries) = fs::read_dir(&reviews_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.contains(&format!("-{}.", epic_id)) && name.ends_with(".json") {
                        files.push(entry.path().to_string_lossy().to_string());
                    }
                }
            }
            files.sort();
            if files.is_empty() {
                None
            } else {
                Some(files)
            }
        } else {
            None
        }
    } else {
        None
    };

    let receipt_files = receipt_files.or_else(|| {
        compare.map(|c| c.split(',').map(|f| f.trim().to_string()).collect())
    });

    if let Some(files) = receipt_files {
        let mut reviews: Vec<serde_json::Value> = Vec::new();
        for rf in &files {
            let rpath = Path::new(rf);
            if !rpath.exists() {
                error_exit(&format!("Receipt file not found: {}", rf));
            }
            match fs::read_to_string(rpath) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(rdata) => {
                        reviews.push(json!({
                            "file": rf,
                            "mode": rdata.get("mode").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "verdict": rdata.get("verdict").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "id": rdata.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                            "timestamp": rdata.get("timestamp").and_then(|v| v.as_str()).unwrap_or(""),
                            "review": rdata.get("review").and_then(|v| v.as_str()).unwrap_or(""),
                        }));
                    }
                    Err(e) => {
                        error_exit(&format!("Invalid receipt JSON: {}: {}", rf, e));
                    }
                },
                Err(e) => {
                    error_exit(&format!("Could not read receipt: {}: {}", rf, e));
                }
            }
        }

        // Analyze verdicts
        let mut verdicts: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for r in &reviews {
            let mode = r["mode"].as_str().unwrap_or("unknown").to_string();
            let verdict = r["verdict"].as_str().unwrap_or("unknown").to_string();
            verdicts.insert(mode, verdict);
        }
        let verdict_values: std::collections::HashSet<&String> = verdicts.values().collect();
        let all_same = verdict_values.len() <= 1;
        let consensus = if all_same {
            verdicts.values().next().cloned()
        } else {
            None
        };

        if json_mode {
            json_output(json!({
                "reviews": reviews.len(),
                "verdicts": verdicts,
                "consensus": consensus,
                "has_conflict": !all_same,
                "details": reviews,
            }));
        } else {
            println!("Review Comparison ({} reviews):\n", reviews.len());
            for r in &reviews {
                println!(
                    "  [{}] verdict: {}  ({})",
                    r["mode"].as_str().unwrap_or(""),
                    r["verdict"].as_str().unwrap_or(""),
                    r["file"].as_str().unwrap_or("")
                );
            }
            println!();
            if all_same {
                println!("Consensus: {}", consensus.unwrap_or_default());
            } else {
                println!("CONFLICT \u{2014} reviewers disagree:");
                for (mode, verdict) in &verdicts {
                    println!("  {}: {}", mode, verdict);
                }
            }
        }
        return;
    }

    if json_mode {
        json_output(json!({"backend": backend, "source": source}));
    } else {
        println!("{}", backend);
    }
}

// ── Parse-findings command ─────────────────────────────────────────

pub fn cmd_parse_findings(
    json_mode: bool,
    file: String,
    _epic: Option<String>,
    _register: bool,
    _source: String,
) {
    // Read input from file or stdin
    let text = if file == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to read stdin: {}", e));
            });
        buf
    } else {
        match fs::read_to_string(&file) {
            Ok(content) => content,
            Err(e) => {
                error_exit(&format!("Failed to read file {}: {}", file, e));
            }
        }
    };

    let mut findings: Vec<serde_json::Value> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let required_keys = ["title", "severity", "location", "recommendation"];

    // Tiered extraction:
    // 1. <findings>...</findings> tag
    // 2. Bare JSON array
    // 3. Markdown code block
    let raw_json = if let Some(start) = text.find("<findings>") {
        if let Some(end) = text.find("</findings>") {
            let inner = &text[start + 10..end];
            Some(inner.trim().to_string())
        } else {
            None
        }
    } else {
        None
    };

    let raw_json = raw_json.or_else(|| {
        // Tier 2: bare JSON array [{...}]
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                let candidate = &text[start..=end];
                warnings.push("No <findings> tag found; extracted bare JSON array".to_string());
                Some(candidate.to_string())
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(raw) = raw_json {
        // Remove trailing commas before ] or }
        let cleaned = raw
            .replace(",]", "]")
            .replace(",}", "}");

        match serde_json::from_str::<serde_json::Value>(&cleaned) {
            Ok(serde_json::Value::Array(arr)) => {
                for (i, item) in arr.iter().enumerate() {
                    if !item.is_object() {
                        warnings.push(format!("Finding {} is not an object, skipping", i));
                        continue;
                    }
                    let missing: Vec<&&str> = required_keys
                        .iter()
                        .filter(|k| item.get(**k).is_none())
                        .collect();
                    if !missing.is_empty() {
                        let keys: Vec<&str> = missing.iter().map(|k| **k).collect();
                        warnings.push(format!(
                            "Finding {} missing keys: {}, skipping",
                            i,
                            keys.join(", ")
                        ));
                        continue;
                    }
                    findings.push(item.clone());
                }
                // Cap at 50
                if findings.len() > 50 {
                    warnings.push(format!(
                        "Found {} findings, capping at 50",
                        findings.len()
                    ));
                    findings.truncate(50);
                }
            }
            Ok(_) => {
                warnings.push("Findings JSON is not a list".to_string());
            }
            Err(e) => {
                warnings.push(format!("Failed to parse findings JSON: {}", e));
            }
        }
    } else {
        warnings.push("No findings found in review output".to_string());
    }

    if json_mode {
        json_output(json!({
            "findings": findings,
            "count": findings.len(),
            "registered": 0,
            "warnings": warnings,
        }));
    } else {
        println!("Found {} finding(s)", findings.len());
        for w in &warnings {
            eprintln!("  Warning: {}", w);
        }
        for f in &findings {
            let sev = f["severity"].as_str().unwrap_or("unknown");
            let title = f["title"].as_str().unwrap_or("");
            let location = f["location"].as_str().unwrap_or("");
            println!("  [{}] {} \u{2014} {}", sev, title, location);
        }
    }
}

// ── Guard command ──────────────────────────────────────────────────

pub fn cmd_guard(json_mode: bool, layer: String) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    // Load stack config
    let config_path = flow_dir.join(CONFIG_FILE);
    let config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let raw =
                    serde_json::from_str::<serde_json::Value>(&content).unwrap_or(json!({}));
                deep_merge(&get_default_config(), &raw)
            }
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    let stack = config.get("stack").cloned().unwrap_or(json!({}));
    let stack_obj = stack.as_object();

    if stack_obj.is_none() || stack_obj.unwrap().is_empty() {
        if json_mode {
            json_output(json!({
                "results": [],
                "message": "no stack detected, nothing to run",
            }));
        } else {
            println!("No stack detected. Nothing to run.");
        }
        return;
    }

    let cmd_types = ["test", "lint", "typecheck"];
    let mut commands: Vec<(String, String, String)> = Vec::new(); // (layer_name, type, cmd)

    for (layer_name, layer_conf) in stack_obj.unwrap() {
        if layer != "all" && layer_name != &layer {
            continue;
        }
        if let Some(layer_obj) = layer_conf.as_object() {
            for ct in &cmd_types {
                if let Some(cmd_val) = layer_obj.get(*ct) {
                    if let Some(cmd_str) = cmd_val.as_str() {
                        if !cmd_str.is_empty() {
                            commands.push((
                                layer_name.clone(),
                                ct.to_string(),
                                cmd_str.to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    if commands.is_empty() {
        if json_mode {
            json_output(json!({
                "results": [],
                "message": "no guard commands configured",
            }));
        } else {
            println!("No guard commands found in stack config.");
        }
        return;
    }

    // Find repo root for running commands
    let repo_root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(
                    String::from_utf8_lossy(&o.stdout)
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .unwrap_or_else(|| ".".to_string());

    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut all_passed = true;

    for (layer_name, cmd_type, cmd) in &commands {
        if !json_mode {
            println!("\u{25b8} [{}] {}: {}", layer_name, cmd_type, cmd);
        }

        let output = Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&repo_root)
            .output();

        let rc = match &output {
            Ok(o) => o.status.code().unwrap_or(1),
            Err(_) => 1,
        };

        let passed = rc == 0;
        if !passed {
            all_passed = false;
        }

        results.push(json!({
            "layer": layer_name,
            "type": cmd_type,
            "command": cmd,
            "passed": passed,
            "exit_code": rc,
        }));

        if !json_mode {
            let status = if passed { "\u{2713}" } else { "\u{2717}" };
            println!("  {} exit {}", status, rc);
        }
    }

    if json_mode {
        json_output(json!({"results": results}));
    } else {
        let passed_count = results.iter().filter(|r| r["passed"].as_bool().unwrap_or(false)).count();
        let total = results.len();
        let suffix = if all_passed { "" } else { " \u{2014} FAILED" };
        println!("\n{}/{} guards passed{}", passed_count, total, suffix);
    }

    if !all_passed {
        std::process::exit(1);
    }
}

// ── Worker-prompt command ──────────────────────────────────────────

pub fn cmd_worker_prompt(json_mode: bool, task: String, tdd: bool, review: Option<String>) {
    // Determine epic from task ID
    let epic_id = if flowctl_core::id::is_task_id(&task) {
        flowctl_core::id::epic_id_from_task(&task).unwrap_or_else(|_| task.clone())
    } else {
        task.clone()
    };

    // Build phase sequence
    let has_review = review.is_some();
    let phases: Vec<&str> = if tdd && has_review {
        flowctl_core::types::PHASE_SEQ_TDD
            .iter()
            .chain(flowctl_core::types::PHASE_SEQ_REVIEW.iter())
            .copied()
            .collect::<std::collections::BTreeSet<&str>>()
            .into_iter()
            .collect()
    } else if tdd {
        flowctl_core::types::PHASE_SEQ_TDD.to_vec()
    } else if has_review {
        flowctl_core::types::PHASE_SEQ_REVIEW.to_vec()
    } else {
        flowctl_core::types::PHASE_SEQ_DEFAULT.to_vec()
    };

    // Build a minimal bootstrap prompt
    let review_line = review
        .as_ref()
        .map(|r| format!("REVIEW_MODE: {}", r))
        .unwrap_or_else(|| "REVIEW_MODE: none".to_string());
    let tdd_line = if tdd { "TDD_MODE: true" } else { "TDD_MODE: false" };

    let phase_list: Vec<String> = phases
        .iter()
        .filter_map(|pid| {
            flowctl_core::types::PHASE_DEFS
                .iter()
                .find(|(id, _, _)| id == pid)
                .map(|(id, title, _)| format!("Phase {}: {}", id, title))
        })
        .collect();

    let prompt_text = format!(
        "TASK_ID: {task}\nEPIC_ID: {epic_id}\n{tdd_line}\n{review_line}\nTEAM_MODE: true\n\nPhase sequence:\n{phases}\n\nExecute phases in order. Use flowctl worker-phase next/done to track progress.",
        task = task,
        epic_id = epic_id,
        tdd_line = tdd_line,
        review_line = review_line,
        phases = phase_list.join("\n"),
    );

    let estimated_tokens = prompt_text.len() / 4;

    if json_mode {
        json_output(json!({
            "prompt": prompt_text,
            "mode": "bootstrap",
            "estimated_tokens": estimated_tokens,
        }));
    } else {
        println!("{}", prompt_text);
    }
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Get a config value.
    Get {
        /// Config key (e.g., memory.enabled).
        key: String,
    },
    /// Set a config value.
    Set {
        /// Config key.
        key: String,
        /// Config value.
        value: String,
    },
}

pub fn cmd_config(cmd: &ConfigCmd, json: bool) {
    match cmd {
        ConfigCmd::Get { key } => cmd_config_get(json, key),
        ConfigCmd::Set { key, value } => cmd_config_set(json, key, value),
    }
}

fn cmd_config_get(json_mode: bool, key: &str) {
    let flow_dir = get_flow_dir();
    let config_path = flow_dir.join(CONFIG_FILE);

    // Load config with defaults
    let config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let raw = serde_json::from_str::<serde_json::Value>(&content)
                    .unwrap_or(json!({}));
                deep_merge(&get_default_config(), &raw)
            }
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    // Navigate nested key path
    let mut current = &config;
    for part in key.split('.') {
        match current.get(part) {
            Some(val) => current = val,
            None => {
                if json_mode {
                    json_output(json!({
                        "key": key,
                        "value": null,
                    }));
                } else {
                    println!("{}: (not set)", key);
                }
                return;
            }
        }
    }

    if json_mode {
        json_output(json!({
            "key": key,
            "value": current,
        }));
    } else {
        println!("{}: {}", key, current);
    }
}

fn cmd_config_set(json_mode: bool, key: &str, value: &str) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let config_path = flow_dir.join(CONFIG_FILE);

    // Load existing config
    let mut config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(json!({})),
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    // Parse value (handle type conversion)
    let parsed_value: serde_json::Value = match value.to_lowercase().as_str() {
        "true" => json!(true),
        "false" => json!(false),
        _ if value.parse::<i64>().is_ok() => json!(value.parse::<i64>().unwrap()),
        _ => json!(value),
    };

    // Navigate/create nested path
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &mut config;
    for part in &parts[..parts.len() - 1] {
        if !current.is_object() || !current.as_object().unwrap().contains_key(*part) {
            current[*part] = json!({});
        }
        current = &mut current[*part];
    }
    if let Some(last) = parts.last() {
        current[*last] = parsed_value.clone();
    }

    write_json_file(&config_path, &config);

    if json_mode {
        json_output(json!({
            "key": key,
            "value": parsed_value,
            "message": format!("Set {} = {}", key, parsed_value),
        }));
    } else {
        println!("Set {} = {}", key, parsed_value);
    }
}

// ── Extracted submodules ─────────────────────────────────────────
mod exchange;
pub use exchange::{cmd_export, cmd_import};
