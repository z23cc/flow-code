//! Lifecycle commands: start, done, block, fail, restart.
//!
//! Thin wrappers that parse CLI args, call service functions, and format output.

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::state_machine::Status;
use flowctl_service::lifecycle::{
    BlockTaskRequest, DoneTaskRequest, FailTaskRequest, RestartTaskRequest, StartTaskRequest,
};

use super::{block_on, ensure_flow_exists, resolve_actor, try_open_lsql_conn};

pub fn cmd_start(json_mode: bool, id: String, force: bool, _note: Option<String>) {
    let flow_dir = ensure_flow_exists();
    let conn = try_open_lsql_conn();
    let actor = resolve_actor();

    let req = StartTaskRequest {
        task_id: id.clone(),
        force,
        actor,
    };

    match block_on(flowctl_service::lifecycle::start_task(conn.as_ref(), &flow_dir, req)) {
        Ok(resp) => {
            if json_mode {
                json_output(json!({
                    "id": resp.task_id,
                    "status": "in_progress",
                    "message": format!("Task {} started", resp.task_id),
                }));
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
    let conn = try_open_lsql_conn();
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

    match block_on(flowctl_service::lifecycle::done_task(conn.as_ref(), &flow_dir, req)) {
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
                println!("Task {} completed{}", resp.task_id, dur_str.unwrap_or_default());
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
    let conn = try_open_lsql_conn();

    let req = BlockTaskRequest {
        task_id: id.clone(),
        reason,
    };

    match block_on(flowctl_service::lifecycle::block_task(conn.as_ref(), &flow_dir, req)) {
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
    let conn = try_open_lsql_conn();

    let req = FailTaskRequest {
        task_id: id.clone(),
        reason,
        force,
    };

    match block_on(flowctl_service::lifecycle::fail_task(conn.as_ref(), &flow_dir, req)) {
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
                        println!("  retry {}/{} \u{2014} will be retried on next run", count, max);
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
    let conn = try_open_lsql_conn();

    let req = RestartTaskRequest {
        task_id: id.clone(),
        dry_run,
        force,
    };

    match block_on(flowctl_service::lifecycle::restart_task(conn.as_ref(), &flow_dir, req)) {
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
                    // In dry-run mode we don't have per-task status info in the response,
                    // so just list the IDs
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
