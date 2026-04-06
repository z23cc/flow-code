//! Approval commands: `flowctl approval create|list|show|approve|reject`.
//!
//! All operations go directly through libSQL.

use std::env;
use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::Value;

use flowctl_core::approvals::{ApprovalKind, ApprovalStatus, CreateApprovalRequest};
use flowctl_service::approvals::{ApprovalStore, LibSqlApprovalStore};

use crate::output::{error_exit, json_output};

use super::helpers::resolve_actor;

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
    // Every subcommand touches async DB/HTTP — run on a Tokio runtime.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|e| error_exit(&format!("tokio runtime: {e}")));
    rt.block_on(async move {
        match cmd {
            ApprovalCmd::Create { task, kind, payload } => {
                cmd_create(json, task, kind, payload).await
            }
            ApprovalCmd::List { pending } => cmd_list(json, *pending).await,
            ApprovalCmd::Show { id, wait, timeout } => {
                cmd_show(json, id, *wait, *timeout).await
            }
            ApprovalCmd::Approve { id } => cmd_approve(json, id).await,
            ApprovalCmd::Reject { id, reason } => {
                cmd_reject(json, id, reason.clone()).await
            }
        }
    });
}

// ── DB operations ───────────────────────────────────────────────────

async fn open_local_store() -> LibSqlApprovalStore {
    let cwd = env::current_dir()
        .unwrap_or_else(|e| error_exit(&format!("cwd: {e}")));
    let db = flowctl_db::open_async(&cwd)
        .await
        .unwrap_or_else(|e| error_exit(&format!("open db: {e}")));
    let conn = db
        .connect()
        .unwrap_or_else(|e| error_exit(&format!("connect db: {e}")));
    // Leak the Database so the connection stays valid for the rest of the
    // process lifetime (CLI is short-lived).
    Box::leak(Box::new(db));
    LibSqlApprovalStore::new(conn)
}

// ── Payload parsing ─────────────────────────────────────────────────

fn parse_payload(s: &str) -> Value {
    if let Some(rest) = s.strip_prefix('@') {
        let content = std::fs::read_to_string(rest)
            .unwrap_or_else(|e| error_exit(&format!("cannot read {rest}: {e}")));
        return serde_json::from_str(&content)
            .unwrap_or_else(|e| error_exit(&format!("invalid JSON in {rest}: {e}")));
    }
    serde_json::from_str(s)
        .unwrap_or_else(|e| error_exit(&format!("invalid --payload JSON: {e}")))
}

// ── Command impls ───────────────────────────────────────────────────

async fn cmd_create(json: bool, task: &str, kind_str: &str, payload: &str) {
    let kind = ApprovalKind::parse(kind_str)
        .unwrap_or_else(|| error_exit(&format!("invalid --kind: {kind_str}")));
    let payload_val = parse_payload(payload);

    let store = open_local_store().await;
    let created = store
        .create(CreateApprovalRequest {
            task_id: task.to_string(),
            kind,
            payload: payload_val,
        })
        .await
        .unwrap_or_else(|e| error_exit(&format!("create: {e}")));
    emit_result(json, serde_json::to_value(&created).unwrap_or_default());
}

async fn cmd_list(json: bool, pending_only: bool) {
    let store = open_local_store().await;
    let filter = if pending_only {
        Some(ApprovalStatus::Pending)
    } else {
        None
    };
    let approvals = store
        .list(filter)
        .await
        .unwrap_or_else(|e| error_exit(&format!("list: {e}")));
    emit_list(json, serde_json::to_value(&approvals).unwrap_or_default());
}

async fn cmd_show(json: bool, id: &str, wait: bool, timeout_secs: u64) {
    if !wait {
        let val = fetch_one(id).await;
        emit_result(json, val);
        return;
    }

    // Poll every 1s until status != pending OR timeout elapsed.
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let val = fetch_one(id).await;
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
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn fetch_one(id: &str) -> Value {
    let store = open_local_store().await;
    let approval = store
        .get(id)
        .await
        .unwrap_or_else(|e| error_exit(&format!("get: {e}")));
    serde_json::to_value(&approval).unwrap_or_default()
}

async fn cmd_approve(json: bool, id: &str) {
    let resolver = resolve_actor();
    let store = open_local_store().await;
    let resolved = store
        .approve(id, Some(resolver))
        .await
        .unwrap_or_else(|e| error_exit(&format!("approve: {e}")));
    emit_result(json, serde_json::to_value(&resolved).unwrap_or_default());
}

async fn cmd_reject(json: bool, id: &str, reason: Option<String>) {
    let resolver = resolve_actor();
    let store = open_local_store().await;
    let resolved = store
        .reject(id, Some(resolver), reason)
        .await
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
        let task = item
            .get("task_id")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
        println!("{id}\t{kind}\ttask={task}\tstatus={status}");
    }
}
