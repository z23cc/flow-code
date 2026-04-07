//! RTK Rewrite hook: rewrite Bash commands via rtk token optimizer (PreToolUse hook).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use serde_json::json;

use super::common::read_stdin_json;

/// Cache TTL for RTK probe result (1 hour).
const RTK_PROBE_CACHE_TTL_SECS: u64 = 3600;

/// Returns the path for the RTK probe cache file.
fn rtk_probe_cache_path() -> PathBuf {
    let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(tmp).join("flowctl-rtk-probe")
}

/// Check if rtk is available, using a file-based cache to avoid repeated `command -v rtk` calls.
/// Returns true if rtk is installed and available.
fn is_rtk_available() -> bool {
    let cache_path = rtk_probe_cache_path();

    // Check cache: if file exists and is fresh, use cached result
    if let Ok(metadata) = fs::metadata(&cache_path) {
        let is_fresh = metadata
            .modified()
            .ok()
            .and_then(|mtime| SystemTime::now().duration_since(mtime).ok())
            .map(|age| age.as_secs() < RTK_PROBE_CACHE_TTL_SECS)
            .unwrap_or(false);

        if is_fresh {
            if let Ok(content) = fs::read_to_string(&cache_path) {
                return content.trim() == "found";
            }
        }
    }

    // Cache miss or stale — probe for rtk
    let available = Command::new("sh")
        .args(["-c", "command -v rtk"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Write result to cache (best-effort)
    let _ = fs::write(&cache_path, if available { "found" } else { "not-found" });

    available
}

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

    if !is_rtk_available() {
        // rtk not installed — silent passthrough (cached fast path)
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
