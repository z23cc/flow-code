//! Approval commands: `flowctl approval create|list|show|approve|reject`.
//!
//! All operations use file-based storage via FlowStore.

use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::Value;

use flowctl_core::approvals::FileApprovalStore;
use flowctl_core::approvals::{ApprovalKind, ApprovalStatus, CreateApprovalRequest};

use crate::output::{error_exit, json_output};

use super::helpers::{get_flow_dir, resolve_actor};

#[derive(Subcommand, Debug)]
pub enum ApprovalCmd {
    /// Create a new pending approval.
    Create {
        /// Task ID requesting approval.
        #[arg(long)]
        task: String,
        /// Approval kind: file_access | mutation | generic.
        #[arg(long)]
        kind: String,
        /// JSON payload, or `@path/to/file.json` to read from disk.
        #[arg(long)]
        payload: String,
    },
    /// List approvals.
    List {
        /// Show only pending approvals.
        #[arg(long)]
        pending: bool,
    },
    /// Show a single approval, optionally waiting for resolution.
    Show {
        /// Approval ID.
        id: String,
        /// Poll until status != pending.
        #[arg(long)]
        wait: bool,
        /// Max seconds to wait (default 300).
        #[arg(long, default_value = "300")]
        timeout: u64,
    },
    /// Approve a pending approval.
    Approve {
        /// Approval ID.
        id: String,
    },
    /// Reject a pending approval.
    Reject {
        /// Approval ID.
        id: String,
        /// Optional reason.
        #[arg(long)]
        reason: Option<String>,
    },
}

pub fn dispatch(cmd: &ApprovalCmd, json: bool) {
    match cmd {
        ApprovalCmd::Create {
            task,
            kind,
            payload,
        } => cmd_create(json, task, kind, payload),
        ApprovalCmd::List { pending } => cmd_list(json, *pending),
        ApprovalCmd::Show { id, wait, timeout } => cmd_show(json, id, *wait, *timeout),
        ApprovalCmd::Approve { id } => cmd_approve(json, id),
        ApprovalCmd::Reject { id, reason } => cmd_reject(json, id, reason.clone()),
    }
}

// ── Store access ───────────────────────────────────────────────────

fn open_local_store() -> FileApprovalStore {
    let flow_dir = get_flow_dir();
    FileApprovalStore::new(flow_dir)
}

// ── Payload parsing ─────────────────────────────────────────────────

fn parse_payload(s: &str) -> Value {
    if let Some(rest) = s.strip_prefix('@') {
        let content = std::fs::read_to_string(rest)
            .unwrap_or_else(|e| error_exit(&format!("cannot read {rest}: {e}")));
        return serde_json::from_str(&content)
            .unwrap_or_else(|e| error_exit(&format!("invalid JSON in {rest}: {e}")));
    }
    serde_json::from_str(s).unwrap_or_else(|e| error_exit(&format!("invalid --payload JSON: {e}")))
}

// ── Command impls ───────────────────────────────────────────────────

fn cmd_create(json: bool, task: &str, kind_str: &str, payload: &str) {
    let kind = ApprovalKind::parse(kind_str)
        .unwrap_or_else(|| error_exit(&format!("invalid --kind: {kind_str}")));
    let payload_val = parse_payload(payload);

    let store = open_local_store();
    let created = store
        .create(CreateApprovalRequest {
            task_id: task.to_string(),
            kind,
            payload: payload_val,
        })
        .unwrap_or_else(|e| error_exit(&format!("create: {e}")));
    emit_result(json, serde_json::to_value(&created).unwrap_or_default());
}

fn cmd_list(json: bool, pending_only: bool) {
    let store = open_local_store();
    let filter = if pending_only {
        Some(ApprovalStatus::Pending)
    } else {
        None
    };
    let approvals = store
        .list(filter)
        .unwrap_or_else(|e| error_exit(&format!("list: {e}")));
    emit_list(json, serde_json::to_value(&approvals).unwrap_or_default());
}

fn cmd_show(json: bool, id: &str, wait: bool, timeout_secs: u64) {
    if !wait {
        let val = fetch_one(id);
        emit_result(json, val);
        return;
    }

    // Poll every 1s until status != pending OR timeout elapsed.
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let val = fetch_one(id);
        let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if status != "pending" {
            emit_result(json, val);
            return;
        }
        if Instant::now() >= deadline {
            error_exit(&format!(
                "timeout waiting for approval {id} (status still pending after {timeout_secs}s)"
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn fetch_one(id: &str) -> Value {
    let store = open_local_store();
    let approval = store
        .get(id)
        .unwrap_or_else(|e| error_exit(&format!("get: {e}")));
    serde_json::to_value(&approval).unwrap_or_default()
}

fn cmd_approve(json: bool, id: &str) {
    let resolver = resolve_actor();
    let store = open_local_store();
    let resolved = store
        .approve(id, Some(resolver))
        .unwrap_or_else(|e| error_exit(&format!("approve: {e}")));
    emit_result(json, serde_json::to_value(&resolved).unwrap_or_default());
}

fn cmd_reject(json: bool, id: &str, reason: Option<String>) {
    let resolver = resolve_actor();
    let store = open_local_store();
    let resolved = store
        .reject(id, Some(resolver), reason)
        .unwrap_or_else(|e| error_exit(&format!("reject: {e}")));
    emit_result(json, serde_json::to_value(&resolved).unwrap_or_default());
}

fn emit_result(json: bool, val: Value) {
    if json {
        json_output(val);
    } else if let Some(obj) = val.as_object() {
        let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let status = obj.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let task = obj.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
        println!("{id}\ttask={task}\tstatus={status}");
    } else {
        println!("{val}");
    }
}

fn emit_list(json: bool, val: Value) {
    if json {
        json_output(val);
        return;
    }
    let arr = match val.as_array() {
        Some(a) => a,
        None => {
            println!("{val}");
            return;
        }
    };
    if arr.is_empty() {
        println!("(no approvals)");
        return;
    }
    for item in arr {
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        let task = item.get("task_id").and_then(|v| v.as_str()).unwrap_or("?");
        let status = item.get("status").and_then(|v| v.as_str()).unwrap_or("?");
        let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        println!("{id}\t{kind}\ttask={task}\tstatus={status}");
    }
}
