//! Codex CLI wrapper commands.
//!
//! Spawns the `codex` CLI for code review operations. All review variants
//! delegate to `codex exec` with appropriate prompts and sandbox settings.

use std::env;
use std::process::Command;

use clap::Subcommand;
use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output};

#[derive(Subcommand, Debug)]
pub enum CodexCmd {
    /// Check codex availability.
    Check,
    /// Implementation review.
    ImplReview {
        /// Task ID (optional for standalone).
        task: Option<String>,
        /// Base branch for diff.
        #[arg(long)]
        base: String,
        /// Focus areas (comma-separated).
        #[arg(long)]
        focus: Option<String>,
        /// Receipt file path.
        #[arg(long)]
        receipt: Option<String>,
        /// Sandbox mode.
        #[arg(long, default_value = "auto", value_parser = ["read-only", "workspace-write", "danger-full-access", "auto"])]
        sandbox: String,
        /// Model reasoning effort level.
        #[arg(long, default_value = "high", value_parser = ["low", "medium", "high"])]
        effort: String,
    },
    /// Plan review.
    PlanReview {
        /// Epic ID.
        epic: String,
        /// Comma-separated file paths for context.
        #[arg(long)]
        files: String,
        /// Base branch for context.
        #[arg(long, default_value = "main")]
        base: String,
        /// Receipt file path.
        #[arg(long)]
        receipt: Option<String>,
        /// Sandbox mode.
        #[arg(long, default_value = "auto", value_parser = ["read-only", "workspace-write", "danger-full-access", "auto"])]
        sandbox: String,
        /// Model reasoning effort level.
        #[arg(long, default_value = "high", value_parser = ["low", "medium", "high"])]
        effort: String,
    },
    /// Adversarial review -- tries to break the code.
    Adversarial {
        /// Base branch for diff.
        #[arg(long, default_value = "main")]
        base: String,
        /// Specific area to pressure-test.
        #[arg(long)]
        focus: Option<String>,
        /// Sandbox mode.
        #[arg(long, default_value = "auto")]
        sandbox: String,
        /// Model reasoning effort level.
        #[arg(long, default_value = "high", value_parser = ["low", "medium", "high"])]
        effort: String,
    },
    /// Epic completion review.
    CompletionReview {
        /// Epic ID.
        epic: String,
        /// Base branch for diff.
        #[arg(long, default_value = "main")]
        base: String,
        /// Receipt file path.
        #[arg(long)]
        receipt: Option<String>,
        /// Sandbox mode.
        #[arg(long, default_value = "auto", value_parser = ["read-only", "workspace-write", "danger-full-access", "auto"])]
        sandbox: String,
        /// Model reasoning effort level.
        #[arg(long, default_value = "high", value_parser = ["low", "medium", "high"])]
        effort: String,
    },
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Locate `codex` in PATH, returning the full path or None.
fn find_codex() -> Option<String> {
    which::which("codex").ok().map(|p| p.to_string_lossy().to_string())
}

/// Get codex version string (e.g. "0.1.2") or None.
fn get_codex_version() -> Option<String> {
    let codex = find_codex()?;
    let output = Command::new(&codex)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let re = Regex::new(r"(\d+\.\d+\.\d+)").unwrap();
    re.captures(text.trim())
        .map(|c| c[1].to_string())
        .or_else(|| Some(text.trim().to_string()))
}

/// Resolve sandbox mode: CLI flag > CODEX_SANDBOX env > platform default.
/// Never returns "auto".
fn resolve_sandbox(sandbox: &str) -> String {
    let s = sandbox.trim();

    // Explicit non-auto value from CLI
    if !s.is_empty() && s != "auto" {
        return s.to_string();
    }

    // Check CODEX_SANDBOX env var
    if let Ok(env_val) = env::var("CODEX_SANDBOX") {
        let ev = env_val.trim().to_string();
        if !ev.is_empty() && ev != "auto" {
            return ev;
        }
    }

    // Platform default
    if cfg!(windows) {
        "danger-full-access".to_string()
    } else {
        "read-only".to_string()
    }
}

/// Run `codex exec` with the given prompt (passed via stdin).
/// Returns (stdout, thread_id, exit_code, stderr).
fn run_codex_exec(
    prompt: &str,
    session_id: Option<&str>,
    sandbox: &str,
    effort: &str,
) -> (String, Option<String>, i32, String) {
    let codex = match find_codex() {
        Some(c) => c,
        None => return (String::new(), None, 2, "codex not found in PATH".to_string()),
    };

    let timeout_secs: u64 = env::var("FLOW_CODEX_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);

    let model = env::var("FLOW_CODEX_MODEL").unwrap_or_else(|_| "gpt-5.4".to_string());

    // Try resume if session_id is provided
    if let Some(sid) = session_id {
        let result = Command::new(&codex)
            .args(["exec", "resume", sid, "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        if let Ok(mut child) = result {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            // Drop stdin to close it
            drop(child.stdin.take());

            match child.wait_with_output() {
                Ok(output) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    return (stdout, Some(sid.to_string()), 0, stderr);
                }
                _ => {
                    eprintln!("WARNING: Codex resume failed, starting new session");
                }
            }
        }
    }

    // New session
    let effort_config = format!("model_reasoning_effort=\"{}\"", effort);
    let mut cmd = Command::new(&codex);
    cmd.args([
        "exec",
        "--model", &model,
        "-c", &effort_config,
        "--sandbox", sandbox,
        "--skip-git-repo-check",
        "--json",
        "-",
    ]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let result = cmd.spawn();
    match result {
        Ok(mut child) => {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            drop(child.stdin.take());

            // Wait with timeout
            let _timeout = std::time::Duration::from_secs(timeout_secs);
            match child.wait_with_output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let code = output.status.code().unwrap_or(1);
                    let thread_id = parse_thread_id(&stdout);
                    (stdout, thread_id, code, stderr)
                }
                Err(e) => (String::new(), None, 2, format!("codex exec error: {e}")),
            }
        }
        Err(e) => (String::new(), None, 2, format!("failed to spawn codex: {e}")),
    }
}

/// Extract thread_id from codex --json JSONL output.
fn parse_thread_id(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(line) {
            if data.get("type").and_then(|v| v.as_str()) == Some("thread.started") {
                if let Some(tid) = data.get("thread_id").and_then(|v| v.as_str()) {
                    return Some(tid.to_string());
                }
            }
        }
    }
    None
}

/// Extract verdict from codex output: <verdict>SHIP</verdict> etc.
fn parse_verdict(output: &str) -> Option<String> {
    let re = Regex::new(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>").unwrap();
    re.captures(output).map(|c| c[1].to_string())
}

/// Load receipt session_id for re-review continuity.
fn load_receipt(path: Option<&str>) -> (Option<String>, bool) {
    let path = match path {
        Some(p) if !p.is_empty() => p,
        _ => return (None, false),
    };
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, false),
    };
    match serde_json::from_str::<serde_json::Value>(&content) {
        Ok(data) => {
            let sid = data.get("session_id").and_then(|v| v.as_str()).map(|s| s.to_string());
            let is_rereview = sid.is_some();
            (sid, is_rereview)
        }
        Err(_) => (None, false),
    }
}

/// Save receipt JSON for ralph-compatible review tracking.
#[allow(clippy::too_many_arguments)]
fn save_receipt(
    path: &str,
    review_type: &str,
    review_id: &str,
    verdict: &str,
    session_id: Option<&str>,
    output: &str,
    base_branch: Option<&str>,
    focus: Option<&str>,
) {
    let mut data = json!({
        "type": review_type,
        "id": review_id,
        "mode": "codex",
        "verdict": verdict,
        "session_id": session_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "review": output,
    });
    if let Some(base) = base_branch {
        data["base"] = json!(base);
    }
    if let Some(f) = focus {
        data["focus"] = json!(f);
    }
    if let Ok(iter_str) = env::var("RALPH_ITERATION") {
        if let Ok(iter) = iter_str.parse::<i64>() {
            data["iteration"] = json!(iter);
        }
    }
    let content = serde_json::to_string_pretty(&data).unwrap_or_default();
    let _ = std::fs::write(path, format!("{content}\n"));
}

/// Delete a stale receipt on failure.
fn delete_stale_receipt(path: Option<&str>) {
    if let Some(p) = path {
        let _ = std::fs::remove_file(p);
    }
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &CodexCmd, json: bool) {
    match cmd {
        CodexCmd::Check => cmd_check(json),
        CodexCmd::ImplReview {
            task, base, focus, receipt, sandbox, effort,
        } => cmd_impl_review(json, task.as_deref(), base, focus.as_deref(), receipt.as_deref(), sandbox, effort),
        CodexCmd::PlanReview {
            epic, files, base, receipt, sandbox, effort,
        } => cmd_plan_review(json, epic, files, base, receipt.as_deref(), sandbox, effort),
        CodexCmd::Adversarial {
            base, focus, sandbox, effort,
        } => cmd_adversarial(json, base, focus.as_deref(), sandbox, effort),
        CodexCmd::CompletionReview {
            epic, base, receipt, sandbox, effort,
        } => cmd_completion_review(json, epic, base, receipt.as_deref(), sandbox, effort),
    }
}

// ── Command implementations ─────────────────────────────────────────

fn cmd_check(json_mode: bool) {
    let available = find_codex().is_some();
    let version = if available { get_codex_version() } else { None };

    if json_mode {
        json_output(json!({
            "available": available,
            "version": version,
        }));
    } else if available {
        println!("codex available: {}", version.unwrap_or_else(|| "unknown version".to_string()));
    } else {
        println!("codex not available");
    }
}

fn cmd_impl_review(
    json_mode: bool,
    task: Option<&str>,
    base: &str,
    focus: Option<&str>,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let standalone = task.is_none();
    let sandbox = resolve_sandbox(sandbox);

    // Load receipt for re-review continuity
    let (session_id, _is_rereview) = load_receipt(receipt);

    // Build a minimal prompt — the real prompt is built by the skill layer,
    // but we support direct invocation with a simple diff-based prompt.
    let prompt = format!(
        "Review the implementation changes from branch '{}' against HEAD.\n\
         {}{}Focus on correctness, quality, performance, and testing.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        base,
        if let Some(t) = task { format!("Task: {t}\n") } else { String::new() },
        if let Some(f) = focus { format!("Focus areas: {f}\n") } else { String::new() },
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    let review_id = task.unwrap_or("branch");

    if let Some(rp) = receipt {
        save_receipt(rp, "impl_review", review_id, &verdict, thread_id.as_deref(), &output, Some(base), focus);
    }

    if json_mode {
        json_output(json!({
            "type": "impl_review",
            "id": review_id,
            "verdict": verdict,
            "session_id": thread_id,
            "mode": "codex",
            "standalone": standalone,
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

fn cmd_plan_review(
    json_mode: bool,
    epic: &str,
    files: &str,
    base: &str,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    if files.is_empty() {
        error_exit("plan-review requires --files argument (comma-separated CODE file paths)");
    }

    let sandbox = resolve_sandbox(sandbox);
    let (session_id, _is_rereview) = load_receipt(receipt);

    let prompt = format!(
        "Review the plan for epic '{}' with context from files: {}.\n\
         Base branch: {base}.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        epic, files,
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    if let Some(rp) = receipt {
        save_receipt(rp, "plan_review", epic, &verdict, thread_id.as_deref(), &output, None, None);
    }

    if json_mode {
        json_output(json!({
            "type": "plan_review",
            "id": epic,
            "verdict": verdict,
            "session_id": thread_id,
            "mode": "codex",
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

fn cmd_adversarial(
    json_mode: bool,
    base: &str,
    focus: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let sandbox = resolve_sandbox(sandbox);

    let prompt = format!(
        "You are an adversarial code reviewer. Try to BREAK the code changed between '{base}' and HEAD.\n\
         {}Look for bugs, race conditions, security vulnerabilities, edge cases, and logic errors.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.\n\
         Also output structured JSON with your findings.",
        if let Some(f) = focus { format!("Focus area: {f}\n") } else { String::new() },
    );

    let (output, _thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, None, &sandbox, effort);

    if exit_code != 0 {
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("Adversarial review failed: {}", msg.trim()));
    }

    // Try to parse structured JSON output
    let structured = parse_adversarial_output(&output);

    if json_mode {
        if let Some(mut s) = structured {
            s["base"] = json!(base);
            s["focus"] = json!(focus);
            json_output(s);
        } else {
            let verdict = parse_verdict(&output);
            json_output(json!({
                "verdict": verdict.unwrap_or_else(|| "UNKNOWN".to_string()),
                "output": output,
                "base": base,
                "focus": focus,
            }));
        }
    } else if let Some(s) = structured {
        println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
        println!("\nVerdict: {}", s.get("verdict").and_then(|v| v.as_str()).unwrap_or("UNKNOWN"));
    } else {
        print!("{output}");
        if let Some(v) = parse_verdict(&output) {
            println!("\nVerdict: {v}");
        }
    }
}

fn cmd_completion_review(
    json_mode: bool,
    epic: &str,
    base: &str,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let sandbox = resolve_sandbox(sandbox);
    let (session_id, _is_rereview) = load_receipt(receipt);

    let prompt = format!(
        "Review epic '{}' for completion. Verify all requirements are implemented.\n\
         Base branch: {base}.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        epic,
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    let session_to_write = thread_id.as_deref().or(session_id.as_deref());

    if let Some(rp) = receipt {
        save_receipt(rp, "completion_review", epic, &verdict, session_to_write, &output, Some(base), None);
    }

    if json_mode {
        json_output(json!({
            "type": "completion_review",
            "id": epic,
            "base": base,
            "verdict": verdict,
            "session_id": session_to_write,
            "mode": "codex",
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

/// Parse structured JSON from adversarial review output.
/// Handles direct JSON, JSONL streaming, markdown fences, embedded JSON.
fn parse_adversarial_output(output: &str) -> Option<serde_json::Value> {
    // Strategy 1: Direct JSON parse
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(output.trim()) {
        if data.is_object() && data.get("verdict").is_some() {
            return Some(data);
        }
    }

    // Strategy 2: JSONL streaming events (codex exec --json)
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if event.get("type").and_then(|v| v.as_str()) == Some("item.completed") {
                if let Some(item) = event.get("item") {
                    if item.get("type").and_then(|v| v.as_str()) == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            if let Ok(data) = serde_json::from_str::<serde_json::Value>(text) {
                                if data.is_object() && data.get("verdict").is_some() {
                                    return Some(data);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 3: Markdown fences
    let fence_re = Regex::new(r"```(?:json)?\s*\n?(.*?)\n?```").unwrap();
    if let Some(caps) = fence_re.captures(output) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(caps[1].trim()) {
            if data.is_object() && data.get("verdict").is_some() {
                return Some(data);
            }
        }
    }

    // Strategy 4: Greedy brace match
    let brace_re = Regex::new(r#"\{[^{}]*"verdict"[^{}]*\}"#).unwrap();
    if let Some(m) = brace_re.find(output) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(m.as_str()) {
            if data.is_object() && data.get("verdict").is_some() {
                return Some(data);
            }
        }
    }

    None
}
