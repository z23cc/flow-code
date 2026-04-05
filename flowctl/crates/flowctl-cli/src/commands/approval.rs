//! Approval commands: `flowctl approval create|list|show|approve|reject`.
//!
//! Transport contract:
//! - Detect daemon via `.flow/.state/flowctl.pid` + `.flow/.state/flowctl.sock`.
//! - When the daemon is reachable, ALL mutations route through the Unix
//!   socket (or TCP fallback) so the event_bus emits exactly one set of
//!   live events from the daemon.
//! - When the daemon is absent, the CLI writes directly to libSQL. No
//!   live events are emitted in that path.

use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::Subcommand;
use serde_json::{json, Value};

use flowctl_core::approvals::{ApprovalKind, ApprovalStatus, CreateApprovalRequest};
use flowctl_core::types::{FLOW_DIR, STATE_DIR};
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

// ── Transport resolution ────────────────────────────────────────────

fn flow_state_dir() -> PathBuf {
    find_flow_dir().join(STATE_DIR)
}

/// Walk up from cwd to locate the nearest `.flow/` directory. Falls back to
/// `./.flow` if none found (fresh repo). This matches how other tools resolve
/// project roots (walk up until `.git`) and avoids the subdirectory pitfall
/// where `flowctl approval ...` was bypassing the daemon when run from a
/// crate folder.
fn find_flow_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cursor: &Path = &cwd;
    loop {
        let candidate = cursor.join(FLOW_DIR);
        if candidate.is_dir() {
            return candidate;
        }
        match cursor.parent() {
            Some(parent) => cursor = parent,
            None => break,
        }
    }
    cwd.join(FLOW_DIR)
}

/// Where the daemon writes its PID + socket.
fn daemon_paths() -> (PathBuf, PathBuf) {
    let state_dir = flow_state_dir();
    (
        state_dir.join("flowctl.pid"),
        state_dir.join("flowctl.sock"),
    )
}

/// Check whether the PID file points to a live process.
fn daemon_is_alive(pid_file: &Path) -> bool {
    let pid_str = match std::fs::read_to_string(pid_file) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let pid: i32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return false,
    };
    // Probe with `kill -0 <pid>` (POSIX). Works without adding a nix dep.
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

async fn fetch_daemon_port() -> Option<u16> {
    // When `flowctl serve --port N` is used, status exposes the TCP port.
    // Read `.flow/.state/flowctl.port` if the daemon wrote one; otherwise None.
    let port_file = flow_state_dir().join("flowctl.port");
    let content = tokio::fs::read_to_string(&port_file).await.ok()?;
    content.trim().parse::<u16>().ok()
}

/// Result of a daemon HTTP call.
enum TransportResult {
    Unreachable,
    Response { status: u16, body: Value },
}

async fn daemon_request(
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> TransportResult {
    let (pid_file, socket_file) = daemon_paths();
    if !daemon_is_alive(&pid_file) {
        return TransportResult::Unreachable;
    }

    // Try Unix socket first.
    if socket_file.exists() {
        if let Some(res) = unix_socket_request(&socket_file, method, path, body).await {
            return res;
        }
    }

    // TCP fallback.
    if let Some(port) = fetch_daemon_port().await {
        if let Some(res) = tcp_request(port, method, path, body).await {
            return res;
        }
    }

    TransportResult::Unreachable
}

async fn unix_socket_request(
    socket_path: &Path,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Option<TransportResult> {
    use http_body_util::{BodyExt, Full};
    use hyper_util::rt::TokioIo;

    let stream = tokio::net::UnixStream::connect(socket_path).await.ok()?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.ok()?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let body_bytes = match body {
        Some(v) => serde_json::to_vec(v).ok()?,
        None => Vec::new(),
    };

    let req = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost")
        .header("content-type", "application/json")
        .body(Full::new(bytes::Bytes::from(body_bytes)))
        .ok()?;

    let resp = sender.send_request(req).await.ok()?;
    let status = resp.status().as_u16();
    let collected = resp.into_body().collect().await.ok()?.to_bytes();
    let json_body: Value = if collected.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&collected).ok()?
    };
    Some(TransportResult::Response {
        status,
        body: json_body,
    })
}

async fn tcp_request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Option<TransportResult> {
    use http_body_util::{BodyExt, Full};
    use hyper_util::rt::TokioIo;

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.ok()?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await.ok()?;
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let body_bytes = match body {
        Some(v) => serde_json::to_vec(v).ok()?,
        None => Vec::new(),
    };

    let req = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header("host", format!("127.0.0.1:{port}"))
        .header("content-type", "application/json")
        .body(Full::new(bytes::Bytes::from(body_bytes)))
        .ok()?;

    let resp = sender.send_request(req).await.ok()?;
    let status = resp.status().as_u16();
    let collected = resp.into_body().collect().await.ok()?.to_bytes();
    let json_body: Value = if collected.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&collected).ok()?
    };
    Some(TransportResult::Response {
        status,
        body: json_body,
    })
}

// ── Direct-DB fallback ──────────────────────────────────────────────

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

    let req_json = json!({
        "task_id": task,
        "kind": kind.as_str(),
        "payload": payload_val,
    });

    match daemon_request("POST", "/api/v1/approvals", Some(&req_json)).await {
        TransportResult::Response { status, body } if (200..300).contains(&status) => {
            emit_result(json, body);
            return;
        }
        TransportResult::Response { status, body } => {
            error_exit(&format!("daemon rejected: {status} {body}"));
        }
        TransportResult::Unreachable => {}
    }

    // Direct-DB path.
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
    let path = if pending_only {
        "/api/v1/approvals?status=pending"
    } else {
        "/api/v1/approvals"
    };

    match daemon_request("GET", path, None).await {
        TransportResult::Response { status, body } if (200..300).contains(&status) => {
            emit_list(json, body);
            return;
        }
        TransportResult::Response { status, body } => {
            error_exit(&format!("daemon rejected: {status} {body}"));
        }
        TransportResult::Unreachable => {}
    }

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
    let path = format!("/api/v1/approvals/{id}");
    match daemon_request("GET", &path, None).await {
        TransportResult::Response { status, body } if (200..300).contains(&status) => {
            return body;
        }
        TransportResult::Response { status, body } => {
            error_exit(&format!("daemon rejected: {status} {body}"));
        }
        TransportResult::Unreachable => {}
    }
    let store = open_local_store().await;
    let approval = store
        .get(id)
        .await
        .unwrap_or_else(|e| error_exit(&format!("get: {e}")));
    serde_json::to_value(&approval).unwrap_or_default()
}

async fn cmd_approve(json: bool, id: &str) {
    let resolver = resolve_actor();
    let body = json!({ "resolver": resolver });
    let path = format!("/api/v1/approvals/{id}/approve");

    match daemon_request("POST", &path, Some(&body)).await {
        TransportResult::Response { status, body } if (200..300).contains(&status) => {
            emit_result(json, body);
            return;
        }
        TransportResult::Response { status, body } => {
            error_exit(&format!("daemon rejected: {status} {body}"));
        }
        TransportResult::Unreachable => {}
    }

    let store = open_local_store().await;
    let resolved = store
        .approve(id, Some(resolver))
        .await
        .unwrap_or_else(|e| error_exit(&format!("approve: {e}")));
    emit_result(json, serde_json::to_value(&resolved).unwrap_or_default());
}

async fn cmd_reject(json: bool, id: &str, reason: Option<String>) {
    let resolver = resolve_actor();
    let body = json!({ "resolver": resolver, "reason": reason });
    let path = format!("/api/v1/approvals/{id}/reject");

    match daemon_request("POST", &path, Some(&body)).await {
        TransportResult::Response { status, body } if (200..300).contains(&status) => {
            emit_result(json, body);
            return;
        }
        TransportResult::Response { status, body } => {
            error_exit(&format!("daemon rejected: {status} {body}"));
        }
        TransportResult::Unreachable => {}
    }

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
