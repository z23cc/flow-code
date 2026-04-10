//! Commit Gate hook: gate git commit on flowctl guard pass (Pre/PostToolUse hook).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::Value;

use super::common::{get_flow_dir, output_block, read_stdin_json, self_exe};

pub fn cmd_commit_gate() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let data = read_stdin_json();
    let event = data
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let command = data
        .get("tool_input")
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Subagent workers bypass
    if session_id.contains('@') {
        std::process::exit(0);
    }

    let state_file = commit_gate_state_file(&flow_dir);

    // PostToolUse: track guard pass
    if event == "PostToolUse" {
        if command.contains("flowctl") && command.contains("guard") {
            let response_text = match data.get("tool_response") {
                Some(Value::Object(map)) => map
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| {
                        serde_json::to_string(&Value::Object(map.clone())).unwrap_or_default()
                    }),
                Some(Value::String(s)) => s.clone(),
                Some(other) => serde_json::to_string(other).unwrap_or_default(),
                None => String::new(),
            };
            let text_lower = response_text.to_lowercase();
            let guard_ok = (text_lower.contains("guards passed") && !text_lower.contains("failed"))
                || text_lower.contains("nothing to run")
                || text_lower.contains("no stack detected");
            if guard_ok {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = fs::write(&state_file, now.to_string());
            }
        }
        std::process::exit(0);
    }

    // PreToolUse: gate git commit
    if event != "PreToolUse" {
        std::process::exit(0);
    }
    if !command.contains("git") || !command.contains("commit") {
        std::process::exit(0);
    }
    // More precise: must be "git commit" (not "git show commit" etc.)
    let git_commit_re = Regex::new(r"\bgit\s+commit\b").expect("static regex must compile");
    if !git_commit_re.is_match(command) {
        std::process::exit(0);
    }

    // Check: any task in_progress?
    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };
    let result = Command::new(&flowctl).args(["tasks", "--json"]).output();
    let has_active = match result {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if let Ok(val) = serde_json::from_str::<Value>(&stdout) {
                val.get("tasks")
                    .and_then(|v| v.as_array())
                    .map(|tasks| {
                        tasks.iter().any(|t| {
                            t.get("status").and_then(|s| s.as_str()) == Some("in_progress")
                        })
                    })
                    .unwrap_or(false)
            } else {
                false
            }
        }
        _ => false,
    };

    if !has_active {
        std::process::exit(0);
    }

    // Check guard evidence
    if state_file.exists() {
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(guard_time) = content.trim().parse::<u64>() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if now.saturating_sub(guard_time) < 600 {
                    let _ = fs::remove_file(&state_file);
                    std::process::exit(0);
                }
            }
        }
    }

    // Block
    output_block("BLOCKED: git commit requires passing guard first.\nRun: flowctl guard");
}

fn commit_gate_state_file(flow_dir: &Path) -> PathBuf {
    let canonical = fs::canonicalize(flow_dir).unwrap_or_else(|_| flow_dir.to_path_buf());
    let hash = md5_hex(canonical.to_string_lossy().as_bytes());
    PathBuf::from(format!(
        "{}/flow-commit-gate-{hash}",
        env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into())
    ))
}

fn md5_hex(data: &[u8]) -> String {
    // Simple MD5 — we only need a stable hash for temp filenames.
    // Use Command to call md5/md5sum for cross-platform compat.
    use std::io::Write;
    let input_str = String::from_utf8_lossy(data).to_string();
    // Try md5 -qs (macOS)
    let result = Command::new("md5").args(["-qs", &input_str]).output();
    if let Ok(o) = result {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout).trim().to_string();
        }
    }
    // Try md5sum (Linux)
    let mut child = match Command::new("md5sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return "default".into(),
    };
    if let Some(ref mut stdin) = child.stdin {
        let _ = stdin.write_all(data);
    }
    match child.wait_with_output() {
        Ok(o) if o.status.success() => {
            let out = String::from_utf8_lossy(&o.stdout);
            out.split_whitespace()
                .next()
                .unwrap_or("default")
                .to_string()
        }
        _ => "default".into(),
    }
}
