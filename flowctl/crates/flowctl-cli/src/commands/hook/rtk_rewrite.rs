//! RTK Rewrite hook: rewrite Bash commands via rtk token optimizer (PreToolUse hook).

use std::process::Command;

use serde_json::json;

use super::common::read_stdin_json;

pub fn cmd_rtk_rewrite() {
    let hook_input = read_stdin_json();

    // Extract tool_input.command from the hook JSON
    let command = hook_input
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if command.is_empty() {
        std::process::exit(0);
    }

    // Check if rtk is installed
    let rtk_available = Command::new("sh")
        .args(["-c", "command -v rtk"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !rtk_available {
        // rtk not installed — silent passthrough
        std::process::exit(0);
    }

    // Call rtk rewrite with the command
    let result = Command::new("rtk")
        .args(["rewrite", command])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            let rewritten = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !rewritten.is_empty() && rewritten != command {
                let response = json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "allow",
                        "permissionDecisionReason": "RTK token optimization",
                        "updatedInput": {
                            "command": rewritten
                        }
                    }
                });
                println!("{}", serde_json::to_string(&response).unwrap_or_default());
            }
            std::process::exit(0);
        }
        _ => {
            // Exit code 1 (unsupported) or error — silent passthrough
            std::process::exit(0);
        }
    }
}
