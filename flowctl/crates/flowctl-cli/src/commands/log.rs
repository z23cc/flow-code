//! Decision logging commands: log decision, log decisions.
//!
//! Records workflow auto-decisions in the events table (event_type = "decision")
//! for post-hoc traceability. Skills call `flowctl log decision` at each auto-
//! decision point; `flowctl log decisions` queries the stored decisions.

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};
use super::db_shim;
use super::helpers::get_flow_dir;

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
        } => cmd_log_decision(json_mode, key, value, reason, epic.as_deref(), task.as_deref()),
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
    let conn = db_shim::open(&flow_dir).unwrap_or_else(|e| {
        error_exit(&format!("Cannot open DB: {e}"));
    });

    let payload = json!({
        "key": key,
        "value": value,
        "reason": reason,
    })
    .to_string();

    let epic = epic_id.unwrap_or("_global");

    let repo = flowctl_db::repo::EventRepo::new(conn.inner_conn());
    let id = db_shim::block_on_pub(async {
        repo.insert(epic, task_id, "decision", None, Some(&payload), None)
            .await
    })
    .unwrap_or_else(|e| {
        error_exit(&format!("Failed to log decision: {e}"));
    });

    if json_mode {
        json_output(json!({
            "id": id,
            "event_type": "decision",
            "key": key,
            "value": value,
            "reason": reason,
            "epic_id": epic,
            "task_id": task_id,
        }));
    } else {
        pretty_output("log", &format!("Decision logged: {key}={value} (reason: {reason})"));
    }
}

fn cmd_log_decisions(json_mode: bool, epic_id: Option<&str>, limit: usize) {
    let flow_dir = get_flow_dir();
    let conn = db_shim::open(&flow_dir).unwrap_or_else(|e| {
        error_exit(&format!("Cannot open DB: {e}"));
    });

    let repo = flowctl_db::repo::EventRepo::new(conn.inner_conn());
    let events = if let Some(epic) = epic_id {
        db_shim::block_on_pub(async { repo.list_by_epic(epic, limit * 2).await })
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to query decisions: {e}"));
            })
            .into_iter()
            .filter(|e| e.event_type == "decision")
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        db_shim::block_on_pub(async { repo.list_by_type("decision", limit).await })
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to query decisions: {e}"));
            })
    };

    if json_mode {
        let items: Vec<_> = events
            .iter()
            .map(|e| {
                let payload: serde_json::Value = e
                    .payload
                    .as_deref()
                    .and_then(|p| serde_json::from_str(p).ok())
                    .unwrap_or(json!(null));
                json!({
                    "id": e.id,
                    "timestamp": e.timestamp,
                    "epic_id": e.epic_id,
                    "task_id": e.task_id,
                    "key": payload["key"],
                    "value": payload["value"],
                    "reason": payload["reason"],
                })
            })
            .collect();
        json_output(json!({ "decisions": items, "count": items.len() }));
    } else {
        if events.is_empty() {
            pretty_output("log", "No decisions recorded.");
            return;
        }
        pretty_output("log", &format!("Decisions ({}):", events.len()));
        for e in &events {
            let payload: serde_json::Value = e
                .payload
                .as_deref()
                .and_then(|p| serde_json::from_str(p).ok())
                .unwrap_or(json!(null));
            pretty_output(
                "log",
                &format!(
                    "  [{}] {}={} — {} (epic: {}, task: {})",
                    &e.timestamp[..19],
                    payload["key"].as_str().unwrap_or("?"),
                    payload["value"].as_str().unwrap_or("?"),
                    payload["reason"].as_str().unwrap_or("?"),
                    e.epic_id,
                    e.task_id.as_deref().unwrap_or("-"),
                ),
            );
        }
    }
}
