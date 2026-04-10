//! Scheduling commands: ready, next, queue.

use std::collections::HashMap;
use std::fs;

use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::id::{is_epic_id, parse_id};
use flowctl_core::state_machine::Status;
use flowctl_core::types::EpicStatus;

use super::{
    ensure_flow_exists, get_runtime, load_epic, load_tasks_for_epic, resolve_actor, scan_epic_ids,
    task_sort_key,
};

pub fn cmd_ready(json_mode: bool, epic: String) {
    let flow_dir = ensure_flow_exists();

    if !is_epic_id(&epic) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic
        ));
    }

    // Verify epic exists in DB
    if load_epic(&flow_dir, &epic).is_none() {
        error_exit(&format!("Epic {} not found", epic));
    }

    let current_actor = resolve_actor();
    let tasks = load_tasks_for_epic(&flow_dir, &epic);

    let mut ready = Vec::new();
    let mut in_progress = Vec::new();
    let mut blocked: Vec<(flowctl_core::types::Task, Vec<String>)> = Vec::new();

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

    ready.sort_by_key(task_sort_key);
    in_progress.sort_by_key(task_sort_key);
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
                let assignee = get_runtime(&flow_dir, &t.id)
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
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "Ready tasks for {} (actor: {}):", epic, current_actor).ok();
        if ready.is_empty() {
            writeln!(buf, "  (none)").ok();
        } else {
            for t in &ready {
                writeln!(buf, "  {}: {}", t.id, t.title).ok();
            }
        }
        if !in_progress.is_empty() {
            writeln!(buf, "\nIn progress:").ok();
            for t in &in_progress {
                let assignee = get_runtime(&flow_dir, &t.id)
                    .and_then(|rt| rt.assignee)
                    .unwrap_or_else(|| "unknown".to_string());
                let marker = if assignee == current_actor {
                    " (you)"
                } else {
                    ""
                };
                writeln!(buf, "  {}: {} [{}]{}", t.id, t.title, assignee, marker).ok();
            }
        }
        if !blocked.is_empty() {
            writeln!(buf, "\nBlocked:").ok();
            for (t, deps) in &blocked {
                writeln!(buf, "  {}: {} (by: {})", t.id, t.title, deps.join(", ")).ok();
            }
        }
        pretty_output("ready", &buf);
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
        if require_plan_review && epic.plan_review != flowctl_core::types::ReviewStatus::Passed {
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
        let mut my_in_progress: Vec<&flowctl_core::types::Task> = tasks
            .values()
            .filter(|t| t.status == Status::InProgress)
            .filter(|t| {
                get_runtime(&flow_dir, &t.id)
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
        let mut ready: Vec<&flowctl_core::types::Task> = tasks
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
            .map_or(true, std::vec::Vec::is_empty)
        {
            1
        } else {
            0
        };
        let b_status = if b["status"].as_str() == Some("done") {
            2
        } else if !b["blocked_by"]
            .as_array()
            .map_or(true, std::vec::Vec::is_empty)
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
                .map_or(true, std::vec::Vec::is_empty)
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
                    let names: Vec<&str> = blocked_by.iter().filter_map(|v| v.as_str()).collect();
                    println!("    \u{2298} blocked by: {}", names.join(", "));
                }
            }

            if let Some(deps) = e["depends_on_epics"].as_array() {
                let blocked_by = e["blocked_by"].as_array();
                if !deps.is_empty() && blocked_by.map_or(true, std::vec::Vec::is_empty) {
                    let names: Vec<&str> = deps.iter().filter_map(|v| v.as_str()).collect();
                    println!("    \u{2192} deps (resolved): {}", names.join(", "));
                }
            }

            println!();
        }
    }
}
