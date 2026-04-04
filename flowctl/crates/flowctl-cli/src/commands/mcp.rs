//! MCP (Model Context Protocol) server over stdio.
//!
//! Implements a minimal JSON-RPC 2.0 server that exposes flowctl operations
//! as MCP tools. AI clients supporting MCP can connect via stdio to manage
//! tasks, epics, and workflows without invoking CLI subprocesses.

use std::env;
use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

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

/// Execute a flowctl tool by shelling out to the CLI with --json.
fn run_flowctl_tool(name: &str, args: &Value) -> Result<String, String> {
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
        "flowctl_start" => {
            cmd_args.push("start".to_string());
            let task = args.get("task_id").and_then(|v| v.as_str())
                .ok_or("task_id is required")?;
            cmd_args.push(task.to_string());
        }
        "flowctl_done" => {
            cmd_args.push("done".to_string());
            let task = args.get("task_id").and_then(|v| v.as_str())
                .ok_or("task_id is required")?;
            cmd_args.push(task.to_string());
            if let Some(summary) = args.get("summary").and_then(|v| v.as_str()) {
                cmd_args.extend(["--summary".to_string(), summary.to_string()]);
            }
            cmd_args.push("--force".to_string());
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
