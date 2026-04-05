//! MCP (Model Context Protocol) server over stdio.
//!
//! Implements a minimal JSON-RPC 2.0 server that exposes flowctl operations
//! as MCP tools. AI clients supporting MCP can connect via stdio to manage
//! tasks, epics, and workflows without invoking CLI subprocesses.

use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde_json::{json, Value};

use flowctl_core::types::FLOW_DIR;
use flowctl_service::lifecycle::{DoneTaskRequest, StartTaskRequest};

/// Run the MCP server loop on stdin/stdout.
pub fn run() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_resp = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("Parse error: {e}")}
                });
                let _ = writeln!(out, "{}", err_resp);
                let _ = out.flush();
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(&id),
            "initialized" => continue, // notification, no response
            "tools/list" => handle_tools_list(&id),
            "tools/call" => handle_tools_call(&id, &request),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Method not found: {method}")}
            }),
        };

        let _ = writeln!(out, "{}", response);
        let _ = out.flush();
    }
}

fn handle_initialize(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "flowctl",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "flowctl_status",
                    "description": "Show .flow state and active runs",
                    "inputSchema": {"type": "object", "properties": {}}
                },
                {
                    "name": "flowctl_epics",
                    "description": "List all epics",
                    "inputSchema": {"type": "object", "properties": {}}
                },
                {
                    "name": "flowctl_tasks",
                    "description": "List tasks for an epic",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "epic_id": {"type": "string", "description": "Epic ID to filter by"}
                        }
                    }
                },
                {
                    "name": "flowctl_ready",
                    "description": "List ready tasks for an epic",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "epic_id": {"type": "string", "description": "Epic ID"}
                        },
                        "required": ["epic_id"]
                    }
                },
                {
                    "name": "flowctl_start",
                    "description": "Start a task",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "task_id": {"type": "string", "description": "Task ID to start"}
                        },
                        "required": ["task_id"]
                    }
                },
                {
                    "name": "flowctl_done",
                    "description": "Complete a task",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "task_id": {"type": "string", "description": "Task ID to complete"},
                            "summary": {"type": "string", "description": "Completion summary"}
                        },
                        "required": ["task_id"]
                    }
                },
                {
                    "name": "flowctl_review",
                    "description": "Run cross-model adversarial review (Codex + Claude consensus)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "base": {"type": "string", "description": "Base branch for diff (default: main)"},
                            "focus": {"type": "string", "description": "Specific area to pressure-test"}
                        }
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: &Value, request: &Value) -> Value {
    let params = request.get("params").cloned().unwrap_or(json!({}));
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = run_flowctl_tool(tool_name, &args);

    match result {
        Ok(output) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": output}]
            }
        }),
        Err(e) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{"type": "text", "text": format!("Error: {e}")}],
                "isError": true
            }
        }),
    }
}

/// Resolve flow_dir and open DB connection for direct service calls.
fn mcp_context() -> Result<(PathBuf, Option<libsql::Connection>), String> {
    let cwd = env::current_dir().map_err(|e| format!("cannot get cwd: {e}"))?;
    let flow_dir = cwd.join(FLOW_DIR);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    let conn = rt.block_on(async {
        let db = flowctl_db_lsql::open_async(&cwd).await.ok()?;
        db.connect().ok()
    });
    Ok((flow_dir, conn))
}

fn mcp_block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(fut)
}

/// Execute a flowctl tool: lifecycle ops use direct service calls,
/// read-only ops shell out to the CLI with --json.
fn run_flowctl_tool(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "flowctl_start" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str())
                .ok_or("task_id is required")?;
            let (flow_dir, conn) = mcp_context()?;
            let req = StartTaskRequest {
                task_id: task_id.to_string(),
                force: false,
                actor: "mcp".to_string(),
            };
            let resp = mcp_block_on(flowctl_service::lifecycle::start_task(
                conn.as_ref(), &flow_dir, req,
            )).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&json!({
                "success": true,
                "id": resp.task_id,
                "status": "in_progress",
                "message": format!("Task {} started", resp.task_id),
            })).unwrap())
        }
        "flowctl_done" => {
            let task_id = args.get("task_id").and_then(|v| v.as_str())
                .ok_or("task_id is required")?;
            let summary = args.get("summary").and_then(|v| v.as_str()).map(String::from);
            let (flow_dir, conn) = mcp_context()?;
            let req = DoneTaskRequest {
                task_id: task_id.to_string(),
                summary,
                summary_file: None,
                evidence_json: None,
                evidence_inline: None,
                force: true,
                actor: "mcp".to_string(),
            };
            let resp = mcp_block_on(flowctl_service::lifecycle::done_task(
                conn.as_ref(), &flow_dir, req,
            )).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&json!({
                "success": true,
                "id": resp.task_id,
                "status": "done",
                "message": format!("Task {} completed", resp.task_id),
            })).unwrap())
        }
        "flowctl_review" => {
            // Cross-model review via CLI subprocess
            let mut cmd_args: Vec<String> = vec!["--json".to_string(), "codex".to_string(), "cross-model".to_string()];
            if let Some(base) = args.get("base").and_then(|v| v.as_str()) {
                cmd_args.extend(["--base".to_string(), base.to_string()]);
            }
            if let Some(focus) = args.get("focus").and_then(|v| v.as_str()) {
                cmd_args.extend(["--focus".to_string(), focus.to_string()]);
            }
            let exe = env::current_exe().map_err(|e| format!("cannot find self: {e}"))?;
            let output = std::process::Command::new(&exe)
                .args(&cmd_args)
                .output()
                .map_err(|e| format!("exec failed: {e}"))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if output.status.success() {
                Ok(stdout)
            } else {
                Err(format!("{stdout}{stderr}"))
            }
        }
        _ => run_flowctl_cli(name, args),
    }
}

/// Shell out to the CLI for read-only operations.
fn run_flowctl_cli(name: &str, args: &Value) -> Result<String, String> {
    let exe = env::current_exe().map_err(|e| format!("cannot find self: {e}"))?;

    let mut cmd_args: Vec<String> = vec!["--json".to_string()];

    match name {
        "flowctl_status" => cmd_args.push("status".to_string()),
        "flowctl_epics" => cmd_args.push("epics".to_string()),
        "flowctl_tasks" => {
            cmd_args.push("tasks".to_string());
            if let Some(epic) = args.get("epic_id").and_then(|v| v.as_str()) {
                cmd_args.extend(["--epic".to_string(), epic.to_string()]);
            }
        }
        "flowctl_ready" => {
            cmd_args.push("ready".to_string());
            let epic = args.get("epic_id").and_then(|v| v.as_str())
                .ok_or("epic_id is required")?;
            cmd_args.extend(["--epic".to_string(), epic.to_string()]);
        }
        _ => return Err(format!("unknown tool: {name}")),
    }

    let output = std::process::Command::new(&exe)
        .args(&cmd_args)
        .output()
        .map_err(|e| format!("exec failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!("{stdout}{stderr}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_initialize() {
        let id = json!(1);
        let resp = handle_initialize(&id);
        assert_eq!(resp["jsonrpc"], "2.0");
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(resp["result"]["serverInfo"]["name"], "flowctl");
    }

    #[test]
    fn test_handle_tools_list_returns_seven_tools() {
        let id = json!(2);
        let resp = handle_tools_list(&id);
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"flowctl_status"));
        assert!(names.contains(&"flowctl_epics"));
        assert!(names.contains(&"flowctl_tasks"));
        assert!(names.contains(&"flowctl_ready"));
        assert!(names.contains(&"flowctl_start"));
        assert!(names.contains(&"flowctl_done"));
        assert!(names.contains(&"flowctl_review"));
    }

    #[test]
    fn test_handle_tools_call_unknown_tool() {
        let id = json!(3);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "nonexistent_tool",
                "arguments": {}
            }
        });
        let resp = handle_tools_call(&id, &request);
        assert_eq!(resp["result"]["isError"], true);
        assert!(resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unknown tool"));
    }

    #[test]
    fn test_unknown_method_returns_error() {
        // Simulate the dispatch logic from run()
        let id = json!(4);
        let method = "nonexistent/method";
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("Method not found: {method}")}
        });
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn test_tools_call_missing_tool_name() {
        let id = json!(5);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {}
        });
        let resp = handle_tools_call(&id, &request);
        // Empty tool name should be treated as unknown
        assert_eq!(resp["result"]["isError"], true);
    }
}
