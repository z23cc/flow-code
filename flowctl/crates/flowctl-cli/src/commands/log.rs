//! Decision logging commands: log decision, log decisions.
//!
//! Records workflow auto-decisions in the JSONL event log for post-hoc
//! traceability. Skills call `flowctl log decision` at each auto-decision
//! point; `flowctl log decisions` queries the stored decisions.

use clap::Subcommand;
use serde_json::json;

use super::helpers::get_flow_dir;
use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::json_store;

#[derive(Subcommand, Debug)]
pub enum LogCmd {
    /// Record a workflow decision.
    Decision {
        /// Decision key (e.g., "review_backend", "branch_strategy").
        #[arg(long)]
        key: String,
        /// Decision value (e.g., "rp-mcp", "worktree").
        #[arg(long)]
        value: String,
        /// Why this decision was made.
        #[arg(long)]
        reason: String,
        /// Epic ID (optional, for scoping).
        #[arg(long)]
        epic: Option<String>,
        /// Task ID (optional, for scoping).
        #[arg(long)]
        task: Option<String>,
    },
    /// Query stored decisions.
    Decisions {
        /// Filter by epic ID.
        #[arg(long)]
        epic: Option<String>,
        /// Maximum number of results (default 20).
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

pub fn dispatch(cmd: &LogCmd, json_mode: bool) {
    match cmd {
        LogCmd::Decision {
            key,
            value,
            reason,
            epic,
            task,
        } => cmd_log_decision(
            json_mode,
            key,
            value,
            reason,
            epic.as_deref(),
            task.as_deref(),
        ),
        LogCmd::Decisions { epic, limit } => cmd_log_decisions(json_mode, epic.as_deref(), *limit),
    }
}

fn cmd_log_decision(
    json_mode: bool,
    key: &str,
    value: &str,
    reason: &str,
    epic_id: Option<&str>,
    task_id: Option<&str>,
) {
    let flow_dir = get_flow_dir();

    let epic = epic_id.unwrap_or("_global");

    let event = json!({
        "stream_id": format!("decision:{epic}"),
        "type": "decision",
        "epic_id": epic,
        "task_id": task_id,
        "key": key,
        "value": value,
        "reason": reason,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if let Err(e) = json_store::events_append(&flow_dir, &event.to_string()) {
        error_exit(&format!("Failed to log decision: {e}"));
    }

    if json_mode {
        json_output(json!({
            "event_type": "decision",
            "key": key,
            "value": value,
            "reason": reason,
            "epic_id": epic,
            "task_id": task_id,
        }));
    } else {
        pretty_output(
            "log",
            &format!("Decision logged: {key}={value} (reason: {reason})"),
        );
    }
}

fn cmd_log_decisions(json_mode: bool, epic_id: Option<&str>, limit: usize) {
    let flow_dir = get_flow_dir();

    let all_lines = json_store::events_read_all(&flow_dir).unwrap_or_else(|e| {
        error_exit(&format!("Failed to query decisions: {e}"));
    });

    // Filter for decision events, optionally by epic
    let mut decisions: Vec<serde_json::Value> = Vec::new();
    for line in all_lines.iter().rev() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            let event_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if event_type != "decision" {
                continue;
            }
            if let Some(epic) = epic_id {
                let eid = val.get("epic_id").and_then(|v| v.as_str()).unwrap_or("");
                if eid != epic {
                    continue;
                }
            }
            decisions.push(val);
            if decisions.len() >= limit {
                break;
            }
        }
    }

    if json_mode {
        let items: Vec<serde_json::Value> = decisions
            .iter()
            .map(|e| {
                json!({
                    "timestamp": e.get("timestamp").and_then(|v| v.as_str()).unwrap_or(""),
                    "epic_id": e.get("epic_id").and_then(|v| v.as_str()).unwrap_or(""),
                    "task_id": e.get("task_id"),
                    "key": e.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                    "value": e.get("value").and_then(|v| v.as_str()).unwrap_or(""),
                    "reason": e.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
                })
            })
            .collect();
        json_output(json!({ "decisions": items, "count": items.len() }));
    } else {
        if decisions.is_empty() {
            pretty_output("log", "No decisions recorded.");
            return;
        }
        pretty_output("log", &format!("Decisions ({}):", decisions.len()));
        for e in &decisions {
            let ts = e.get("timestamp").and_then(|v| v.as_str()).unwrap_or("?");
            let key = e.get("key").and_then(|v| v.as_str()).unwrap_or("?");
            let value = e.get("value").and_then(|v| v.as_str()).unwrap_or("?");
            let reason = e.get("reason").and_then(|v| v.as_str()).unwrap_or("?");
            let epic = e.get("epic_id").and_then(|v| v.as_str()).unwrap_or("?");
            let task = e.get("task_id").and_then(|v| v.as_str()).unwrap_or("-");
            pretty_output(
                "log",
                &format!(
                    "  [{}] {}={} — {} (epic: {}, task: {})",
                    &ts[..std::cmp::min(19, ts.len())],
                    key,
                    value,
                    reason,
                    epic,
                    task,
                ),
            );
        }
    }
}
