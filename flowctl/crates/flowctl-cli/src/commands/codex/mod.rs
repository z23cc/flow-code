//! Codex CLI wrapper commands.
//!
//! Spawns the `codex` CLI for code review operations. All review variants
//! delegate to `codex exec` with appropriate prompts and sandbox settings.

mod review;
mod sync;

use std::env;
use std::process::Command;

use clap::Subcommand;
use regex::Regex;
use serde_json::json;



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
    /// Cross-model review: runs both Codex adversarial AND Claude review,
    /// then computes consensus.
    CrossModel {
        /// Base branch for diff.
        #[arg(long, default_value = "main")]
        base: String,
        /// Specific area to pressure-test.
        #[arg(long)]
        focus: Option<String>,
        /// Sandbox mode for Codex.
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
    /// Sync agent .md files to Codex artifacts.
    Sync {
        /// Directory containing agent .md files.
        #[arg(long, default_value = "agents")]
        agents_dir: String,
        /// Output directory for generated Codex artifacts.
        #[arg(long, default_value = "codex")]
        output_dir: String,
        /// Source hooks.json file to patch.
        #[arg(long, default_value = "hooks/hooks.json")]
        hooks: String,
        /// Validate without writing files.
        #[arg(long)]
        dry_run: bool,
        /// Show per-file details.
        #[arg(long)]
        verbose: bool,
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
            let sid = data.get("session_id").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
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
        CodexCmd::Check => review::cmd_check(json),
        CodexCmd::ImplReview {
            task, base, focus, receipt, sandbox, effort,
        } => review::cmd_impl_review(json, task.as_deref(), base, focus.as_deref(), receipt.as_deref(), sandbox, effort),
        CodexCmd::PlanReview {
            epic, files, base, receipt, sandbox, effort,
        } => review::cmd_plan_review(json, epic, files, base, receipt.as_deref(), sandbox, effort),
        CodexCmd::Adversarial {
            base, focus, sandbox, effort,
        } => review::cmd_adversarial(json, base, focus.as_deref(), sandbox, effort),
        CodexCmd::CrossModel {
            base, focus, sandbox, effort,
        } => review::cmd_cross_model(json, base, focus.as_deref(), sandbox, effort),
        CodexCmd::CompletionReview {
            epic, base, receipt, sandbox, effort,
        } => review::cmd_completion_review(json, epic, base, receipt.as_deref(), sandbox, effort),
        CodexCmd::Sync {
            agents_dir, output_dir, hooks, dry_run, verbose,
        } => sync::cmd_sync(json, agents_dir, output_dir, hooks, *dry_run, *verbose),
    }
}
