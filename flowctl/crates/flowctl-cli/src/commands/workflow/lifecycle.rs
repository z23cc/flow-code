//! Lifecycle commands: start, done, block, fail, restart.
//!
//! Thin wrappers that parse CLI args, call service functions, and format output.

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::lifecycle::{
    BlockTaskRequest, DoneTaskRequest, FailTaskRequest, RestartTaskRequest, StartTaskRequest,
};
use flowctl_core::state_machine::Status;

use super::{ensure_flow_exists, resolve_actor};

pub fn cmd_start(json_mode: bool, id: String, force: bool, _note: Option<String>) {
    let flow_dir = ensure_flow_exists();
    let actor = resolve_actor();

    // ── Parallel execution safety check ────────────────────────
    // If other tasks in the same epic are already in_progress, warn about
    // worktree isolation requirement. This catches the #1 audit failure:
    // workers running in the same directory without isolation.
    let epic_id = flowctl_core::id::epic_id_from_task(&id).unwrap_or_default();
    if !epic_id.is_empty() {
        if let Ok(tasks) = flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic_id) {
            let in_progress: Vec<String> = tasks
                .iter()
                .filter(|t| {
                    t.status == flowctl_core::state_machine::Status::InProgress && t.id != id
                })
                .map(|t| t.id.clone())
                .collect();
            if !in_progress.is_empty() && !force {
                error_exit(&format!(
                    "Cannot start {} while {} other task(s) are in_progress: {}. \
                     Workers MUST use worktree isolation for parallel execution. \
                     Pass --force to override.",
                    id,
                    in_progress.len(),
                    in_progress.join(", ")
                ));
            }
        }
    }

    let req = StartTaskRequest {
        task_id: id.clone(),
        force,
        actor,
    };

    match flowctl_core::lifecycle::start_task(&flow_dir, req) {
        Ok(resp) => {
            if json_mode {
                let mut out = json!({
                    "id": resp.task_id,
                    "status": "in_progress",
                });
                // Include parallel warning in JSON so agent sees it
                let epic_id =
                    flowctl_core::id::epic_id_from_task(&resp.task_id).unwrap_or_default();
                if !epic_id.is_empty() {
                    if let Ok(tasks) =
                        flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic_id)
                    {
                        let in_progress_count = tasks
                            .iter()
                            .filter(|t| t.status == flowctl_core::state_machine::Status::InProgress)
                            .count();
                        if in_progress_count > 1 {
                            out["parallel_warning"] = json!(format!(
                                "{} tasks now in_progress — use isolation:worktree for each worker",
                                in_progress_count
                            ));
                        }
                    }
                }
                json_output(out);
            } else {
                println!("Task {} started", resp.task_id);
            }
        }
        Err(e) => error_exit(&e.to_string()),
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
    let actor = resolve_actor();

    let req = DoneTaskRequest {
        task_id: id.clone(),
        summary_file,
        summary,
        evidence_json,
        evidence_inline: evidence,
        force,
        actor,
    };

    match flowctl_core::lifecycle::done_task(&flow_dir, req) {
        Ok(resp) => {
            if json_mode {
                let mut result = json!({
                    "id": resp.task_id,
                    "status": "done",
                    "message": format!("Task {} completed", resp.task_id),
                });
                if let Some(dur) = resp.duration_seconds {
                    result["duration_seconds"] = json!(dur);
                }
                if let Some(ref warn) = resp.ws_warning {
                    result["warning"] = json!(warn);
                }
                json_output(result);
            } else {
                let dur_str = resp.duration_seconds.map(|dur| {
                    let mins = dur / 60;
                    let secs = dur % 60;
                    if mins > 0 {
                        format!(" ({}m {}s)", mins, secs)
                    } else {
                        format!(" ({}s)", secs)
                    }
                });
                println!(
                    "Task {} completed{}",
                    resp.task_id,
                    dur_str.unwrap_or_default()
                );
                if let Some(warn) = resp.ws_warning {
                    println!("  warning: {}", warn);
                }
            }
        }
        Err(e) => error_exit(&e.to_string()),
    }
}

pub fn cmd_block(json_mode: bool, id: String, reason: String) {
    let flow_dir = ensure_flow_exists();

    let req = BlockTaskRequest {
        task_id: id.clone(),
        reason,
    };

    match flowctl_core::lifecycle::block_task(&flow_dir, req) {
        Ok(resp) => {
            if json_mode {
                json_output(json!({
                    "id": resp.task_id,
                    "status": "blocked",
                    "message": format!("Task {} blocked", resp.task_id),
                }));
            } else {
                println!("Task {} blocked", resp.task_id);
            }
        }
        Err(e) => error_exit(&e.to_string()),
    }
}

pub fn cmd_fail(json_mode: bool, id: String, reason: Option<String>, force: bool) {
    let flow_dir = ensure_flow_exists();

    let req = FailTaskRequest {
        task_id: id.clone(),
        reason,
        force,
    };

    match flowctl_core::lifecycle::fail_task(&flow_dir, req) {
        Ok(resp) => {
            if json_mode {
                let mut result = json!({
                    "id": resp.task_id,
                    "status": resp.final_status.to_string(),
                    "message": format!("Task {} {}", resp.task_id, resp.final_status),
                    "reason": resp.reason,
                });
                if !resp.upstream_failed_ids.is_empty() {
                    result["upstream_failed"] = json!(resp.upstream_failed_ids);
                }
                json_output(result);
            } else {
                println!("Task {} {}", resp.task_id, resp.final_status);
                if resp.final_status == Status::UpForRetry {
                    if let (Some(count), Some(max)) = (resp.retry_count, resp.max_retries) {
                        println!(
                            "  retry {}/{} \u{2014} will be retried on next run",
                            count, max
                        );
                    }
                }
                if !resp.upstream_failed_ids.is_empty() {
                    println!(
                        "  upstream_failed propagated to {} downstream task(s):",
                        resp.upstream_failed_ids.len()
                    );
                    for tid in &resp.upstream_failed_ids {
                        println!("    {}", tid);
                    }
                }
            }
        }
        Err(e) => error_exit(&e.to_string()),
    }
}

pub fn cmd_restart(json_mode: bool, id: String, dry_run: bool, force: bool) {
    let flow_dir = ensure_flow_exists();

    let req = RestartTaskRequest {
        task_id: id.clone(),
        dry_run,
        force,
    };

    match flowctl_core::lifecycle::restart_task(&flow_dir, req) {
        Ok(resp) => {
            if dry_run {
                if json_mode {
                    json_output(json!({
                        "dry_run": true,
                        "would_reset": resp.reset_ids,
                        "already_todo": resp.skipped_ids,
                        "in_progress_overridden": resp.in_progress_overridden,
                    }));
                } else {
                    println!(
                        "Dry run \u{2014} would restart {} task(s):",
                        resp.reset_ids.len()
                    );
                    for tid in &resp.reset_ids {
                        let marker = if resp.in_progress_overridden.contains(tid) {
                            " (force)"
                        } else {
                            ""
                        };
                        println!("  {} -> todo{}", tid, marker);
                    }
                    if !resp.skipped_ids.is_empty() {
                        println!("Already todo: {}", resp.skipped_ids.join(", "));
                    }
                }
            } else if json_mode {
                json_output(json!({
                    "reset": resp.reset_ids,
                    "skipped": resp.skipped_ids,
                    "cascade_from": resp.cascade_from,
                }));
            } else if resp.reset_ids.is_empty() {
                println!(
                    "Nothing to restart \u{2014} {} and dependents already todo.",
                    id
                );
            } else {
                let downstream_count =
                    resp.reset_ids.len() - if resp.reset_ids.contains(&id) { 1 } else { 0 };
                println!(
                    "Restarted from {} (cascade: {} downstream):\n",
                    id, downstream_count
                );
                for tid in &resp.reset_ids {
                    let marker = if *tid == id { " (target)" } else { "" };
                    println!("  {}  -> todo{}", tid, marker);
                }
            }
        }
        Err(e) => error_exit(&e.to_string()),
    }
}

pub fn cmd_events(json_mode: bool, epic_id: String) {
    let flow_dir = ensure_flow_exists();

    // Read all events and filter by epic prefix
    match flowctl_core::json_store::events_read_all(&flow_dir) {
        Ok(lines) => {
            // Parse events and filter by epic
            let mut matching: Vec<serde_json::Value> = Vec::new();
            for line in &lines {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    let stream = val.get("stream_id").and_then(|s| s.as_str()).unwrap_or("");
                    let eid = val.get("epic_id").and_then(|s| s.as_str()).unwrap_or("");
                    if stream.contains(&epic_id) || eid == epic_id {
                        matching.push(val);
                    }
                }
            }

            if json_mode {
                json_output(json!({
                    "epic": epic_id,
                    "count": matching.len(),
                    "events": matching,
                }));
            } else if matching.is_empty() {
                println!("No events found for epic {epic_id}");
            } else {
                println!("Events for epic {} ({} total):\n", epic_id, matching.len());
                for e in &matching {
                    let stream = e.get("stream_id").and_then(|s| s.as_str()).unwrap_or("?");
                    let event_type = e.get("type").and_then(|s| s.as_str()).unwrap_or("?");
                    let ts = e.get("timestamp").and_then(|s| s.as_str()).unwrap_or("?");
                    println!("  [{}] {} — {}", stream, event_type, ts);
                }
            }
        }
        Err(e) => error_exit(&format!("Failed to query events: {e}")),
    }
}
