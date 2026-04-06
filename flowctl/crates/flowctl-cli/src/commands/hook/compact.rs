//! Compact and lifecycle hooks: PreCompact, SubagentContext, TaskCompleted.

use std::collections::HashMap;
use std::fs;

use serde_json::json;

use crate::output::pretty_output;
use super::common::{chrono_utc_now, get_flow_dir, read_stdin_json, run_flowctl, self_exe};

// ═══════════════════════════════════════════════════════════════════════
// Pre-Compact
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_pre_compact() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };

    let mut lines: Vec<String> = Vec::new();

    // 1. Active epics and their progress
    if let Some(epics_val) = run_flowctl(&flowctl, &["epics", "--json"]) {
        if let Some(epics) = epics_val.get("epics").and_then(|v| v.as_array()) {
            for e in epics {
                let eid = match e.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                let status = e
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("open");
                if status == "done" {
                    continue;
                }

                if let Some(tasks_val) =
                    run_flowctl(&flowctl, &["tasks", "--epic", eid, "--json"])
                {
                    if let Some(tasks) = tasks_val.get("tasks").and_then(|v| v.as_array()) {
                        let mut counts: HashMap<String, usize> = HashMap::new();
                        for t in tasks {
                            let s = t
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("todo");
                            *counts.entry(s.to_string()).or_insert(0) += 1;
                        }
                        let mut progress_parts: Vec<String> =
                            counts.iter().map(|(s, c)| format!("{s}={c}")).collect();
                        progress_parts.sort();
                        lines.push(format!("Epic {eid}: {}", progress_parts.join(" ")));

                        // Show in-progress tasks
                        for t in tasks {
                            if t.get("status").and_then(|v| v.as_str()) == Some("in_progress") {
                                let tid = t
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                let title = t
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let files = t
                                    .get("files")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .take(3)
                                            .filter_map(|f| f.as_str())
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                    .unwrap_or_default();
                                let files_str = if files.is_empty() {
                                    String::new()
                                } else {
                                    format!(" files=[{files}]")
                                };
                                lines.push(format!(
                                    "  IN_PROGRESS: {tid} \"{title}\"{files_str}"
                                ));
                            }
                        }
                    }
                }
            }

            // 2. Active file locks
            if let Some(locks_val) = run_flowctl(&flowctl, &["lock-check", "--json"]) {
                let count = locks_val
                    .get("count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                if count > 0 {
                    lines.push(format!("File locks ({count} active):"));
                    if let Some(locks) = locks_val.get("locks").and_then(|v| v.as_object()) {
                        let mut sorted_keys: Vec<&String> = locks.keys().collect();
                        sorted_keys.sort();
                        for f in sorted_keys {
                            if let Some(info) = locks.get(f) {
                                let task_id = info
                                    .get("task_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                lines.push(format!("  {f} -> {task_id}"));
                            }
                        }
                    }
                }
            }

            // 3. Ready tasks
            for e in epics {
                if e.get("status").and_then(|v| v.as_str()) == Some("done") {
                    continue;
                }
                let eid = match e.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                if let Some(ready_val) =
                    run_flowctl(&flowctl, &["ready", "--epic", eid, "--json"])
                {
                    if let Some(ready) = ready_val.get("ready").and_then(|v| v.as_array()) {
                        if !ready.is_empty() {
                            let ids: Vec<&str> = ready
                                .iter()
                                .take(5)
                                .filter_map(|t| t.get("id").and_then(|v| v.as_str()))
                                .collect();
                            lines.push(format!("Ready: {}", ids.join(", ")));
                        }
                    }
                }
            }
        }
    }

    if !lines.is_empty() {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "[flow-code state]").ok();
        for line in &lines {
            writeln!(buf, "{line}").ok();
        }
        writeln!(buf, "[/flow-code state]").ok();
        pretty_output("hook_precompact", &buf);
    }

    std::process::exit(0);
}

// ═══════════════════════════════════════════════════════════════════════
// Subagent Context
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_subagent_context() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };

    if let Some(val) = run_flowctl(&flowctl, &["tasks", "--status", "in_progress", "--json"]) {
        let json_str = serde_json::to_string(&val).unwrap_or_default();
        if json_str != "[]" && !json_str.is_empty() {
            let line = format!("Active flow-code tasks: {json_str}");
            pretty_output("hook_subagent", &line);
        }
    }

    std::process::exit(0);
}

// ═══════════════════════════════════════════════════════════════════════
// Task Completed
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_task_completed() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let data = read_stdin_json();
    let teammate_name = data
        .get("teammate_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let team_name = data
        .get("team_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_subject = data
        .get("task_subject")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract flow task ID from teammate_name (e.g., "worker-fn-1-add-auth.2" -> "fn-1-add-auth.2")
    let mut flow_task_id = if !teammate_name.is_empty() {
        teammate_name.strip_prefix("worker-").unwrap_or(teammate_name).to_string()
    } else {
        String::new()
    };

    // Fallback: try to extract from task_subject
    if flow_task_id.is_empty() || !flow_task_id.starts_with("fn-") {
        let task_id_re = regex::Regex::new(r"fn-[a-z0-9-]+\.\d+").unwrap();
        if let Some(m) = task_id_re.find(task_subject) {
            flow_task_id = m.as_str().to_string();
        }
    }

    // Ensure hooks-log directory exists
    let log_dir = flow_dir.join("hooks-log");
    let _ = fs::create_dir_all(&log_dir);

    // Log the event
    let timestamp = chrono_utc_now();
    let event_json = json!({
        "event": "task_completed",
        "time": timestamp,
        "teammate": teammate_name,
        "team": team_name,
        "flow_task": flow_task_id,
        "subject": task_subject,
    });
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("events.jsonl"))
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", serde_json::to_string(&event_json).unwrap_or_default())
        });

    // If we identified a flow task, unlock its files
    if !flow_task_id.is_empty() && flow_task_id.starts_with("fn-") {
        let flowctl = match self_exe() {
            Some(f) => f,
            None => std::process::exit(0),
        };

        // Check if task exists and is in_progress or done
        if let Some(show_val) = run_flowctl(&flowctl, &["show", &flow_task_id, "--json"]) {
            let status = show_val
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status == "in_progress" || status == "done" {
                let _ = std::process::Command::new(&flowctl)
                    .args(["unlock", "--task", &flow_task_id, "--json"])
                    .output();

                let unlock_json = json!({
                    "event": "files_unlocked",
                    "time": timestamp,
                    "task": flow_task_id,
                });
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_dir.join("events.jsonl"))
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "{}", serde_json::to_string(&unlock_json).unwrap_or_default())
                    });
            }
        }
    }

    std::process::exit(0);
}
