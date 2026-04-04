//! RepoPrompt wrapper commands.
//!
//! Delegates to `rp-cli` for all RP operations. Handles window matching,
//! workspace management, builder invocation, and chat/prompt operations.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use clap::Subcommand;
use regex::Regex;
use sha2::{Digest, Sha256};
use serde_json::json;

use crate::output::{error_exit, json_output};

#[derive(Subcommand, Debug)]
pub enum RpCmd {
    /// List RepoPrompt windows.
    Windows,
    /// Pick window by repo root.
    PickWindow {
        /// Repo root path.
        #[arg(long)]
        repo_root: String,
    },
    /// Ensure workspace and switch window.
    EnsureWorkspace {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Repo root path.
        #[arg(long)]
        repo_root: String,
    },
    /// Run builder and return tab.
    Builder {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Builder summary.
        #[arg(long)]
        summary: String,
        /// Builder response type.
        #[arg(long, value_parser = ["review", "plan", "question", "clarify"])]
        response_type: Option<String>,
    },
    /// Get current prompt.
    PromptGet {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
    },
    /// Set current prompt.
    PromptSet {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
        /// Message file.
        #[arg(long)]
        message_file: String,
    },
    /// Get selection.
    SelectGet {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
    },
    /// Add files to selection.
    SelectAdd {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
        /// Paths to add.
        paths: Vec<String>,
    },
    /// Send chat via rp-cli.
    ChatSend {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
        /// Message file.
        #[arg(long)]
        message_file: String,
        /// Start new chat.
        #[arg(long)]
        new_chat: bool,
        /// Chat name (with --new-chat).
        #[arg(long)]
        chat_name: Option<String>,
        /// Continue specific chat by ID.
        #[arg(long)]
        chat_id: Option<String>,
        /// Chat mode.
        #[arg(long, default_value = "chat", value_parser = ["chat", "review", "plan", "edit"])]
        mode: String,
        /// Override selected paths.
        #[arg(long)]
        selected_paths: Option<Vec<String>>,
    },
    /// Export prompt to file.
    PromptExport {
        /// Window id.
        #[arg(long)]
        window: i64,
        /// Tab id or name.
        #[arg(long)]
        tab: String,
        /// Output file.
        #[arg(long)]
        out: String,
    },
    /// Atomic: pick-window + workspace + builder.
    SetupReview {
        /// Repo root path.
        #[arg(long)]
        repo_root: String,
        /// Builder summary/instructions.
        #[arg(long)]
        summary: String,
        /// Use builder review mode.
        #[arg(long, value_parser = ["review"])]
        response_type: Option<String>,
        /// Create new RP window if none matches.
        #[arg(long)]
        create: bool,
    },
    /// Prepare JSON for rp-cli chat_send.
    PrepChat {
        /// (ignored) Epic/task ID for compatibility.
        id: Option<String>,
        /// File containing message text.
        #[arg(long)]
        message_file: String,
        /// Chat mode.
        #[arg(long, default_value = "chat", value_parser = ["chat", "ask"])]
        mode: String,
        /// Start new chat.
        #[arg(long)]
        new_chat: bool,
        /// Name for new chat.
        #[arg(long)]
        chat_name: Option<String>,
        /// Files to include in context.
        #[arg(long)]
        selected_paths: Option<Vec<String>>,
        /// Output file (default: stdout).
        #[arg(short, long)]
        output: Option<String>,
    },
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Find rp-cli in PATH.
fn require_rp_cli() -> String {
    which::which("rp-cli")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| error_exit("rp-cli not found in PATH"))
}

/// Run rp-cli with args. Returns CompletedProcess-like tuple (stdout, stderr).
/// Exits on error with formatted message.
fn run_rp_cli(args: &[&str], timeout_secs: Option<u64>) -> (String, String) {
    let rp = require_rp_cli();
    let _timeout = timeout_secs.unwrap_or_else(|| {
        env::var("FLOW_RP_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1200)
    });

    let result = Command::new(&rp)
        .args(args)
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if !output.status.success() {
                let msg = if !stderr.is_empty() { &stderr } else if !stdout.is_empty() { &stdout } else { "unknown error" };
                error_exit(&format!("rp-cli failed: {}", msg.trim()));
            }
            (stdout, stderr)
        }
        Err(e) => {
            error_exit(&format!("rp-cli failed: {e}"));
        }
    }
}

/// Normalize repo root for window matching.
/// Handles macOS /tmp symlink and git worktree resolution.
fn normalize_repo_root(path: &str) -> Vec<String> {
    let real = match fs::canonicalize(path) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => path.to_string(),
    };

    let mut roots = vec![real.clone()];

    // macOS /tmp symlink handling
    if real.starts_with("/private/tmp/") {
        roots.push(format!("/tmp/{}", &real["/private/tmp/".len()..]));
    } else if real.starts_with("/tmp/") {
        roots.push(format!("/private/tmp/{}", &real["/tmp/".len()..]));
    }

    // Git worktree -> main repo resolution
    if let Ok(output) = Command::new("git")
        .args(["-C", &real, "rev-parse", "--git-common-dir"])
        .output()
    {
        if output.status.success() {
            let common_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !common_dir.is_empty() && !common_dir.starts_with('.') {
                if let Ok(canon) = fs::canonicalize(&common_dir) {
                    if let Some(parent) = canon.parent() {
                        let main_repo = parent.to_string_lossy().to_string();
                        if main_repo != real {
                            roots.push(main_repo.clone());
                            if main_repo.starts_with("/private/tmp/") {
                                roots.push(format!("/tmp/{}", &main_repo["/private/tmp/".len()..]));
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    roots.retain(|r| seen.insert(r.clone()));
    roots
}

/// Parse windows JSON from rp-cli output.
fn parse_windows(raw: &str) -> Vec<serde_json::Value> {
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(arr) = data.as_array() {
            return arr.clone();
        }
        if let Some(obj) = data.as_object() {
            if let Some(wins) = obj.get("windows").and_then(|v| v.as_array()) {
                return wins.clone();
            }
        }
    }
    if raw.contains("single-window mode") {
        return vec![json!({"windowID": 1, "rootFolderPaths": []})];
    }
    error_exit("windows JSON parse failed");
}

/// Extract window ID from a window object.
fn extract_window_id(win: &serde_json::Value) -> Option<i64> {
    for key in &["windowID", "windowId", "id"] {
        if let Some(v) = win.get(key) {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Extract root folder paths from a window object.
fn extract_root_paths(win: &serde_json::Value) -> Vec<String> {
    for key in &["rootFolderPaths", "rootFolders", "rootFolderPath"] {
        if let Some(v) = win.get(key) {
            if let Some(arr) = v.as_array() {
                return arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
            }
            if let Some(s) = v.as_str() {
                return vec![s.to_string()];
            }
        }
    }
    Vec::new()
}

/// Parse builder output for tab ID.
fn parse_builder_tab(output: &str) -> Option<String> {
    let re = Regex::new(r"Tab:\s*([A-Za-z0-9-]+)").unwrap();
    re.captures(output).map(|c| c[1].to_string())
}

/// Parse chat ID from output.
fn parse_chat_id(output: &str) -> Option<String> {
    let re1 = Regex::new(r#"Chat\s*:\s*`([^`]+)`"#).unwrap();
    if let Some(c) = re1.captures(output) {
        return Some(c[1].to_string());
    }
    let re2 = Regex::new(r#""chat_id"\s*:\s*"([^"]+)""#).unwrap();
    re2.captures(output).map(|c| c[1].to_string())
}

/// Build chat payload JSON.
fn build_chat_payload(
    message: &str,
    mode: &str,
    new_chat: bool,
    chat_name: Option<&str>,
    chat_id: Option<&str>,
    selected_paths: Option<&[String]>,
) -> String {
    let mut payload = json!({
        "message": message,
        "mode": mode,
    });
    if new_chat {
        payload["new_chat"] = json!(true);
    }
    if let Some(name) = chat_name {
        payload["chat_name"] = json!(name);
    }
    if let Some(cid) = chat_id {
        payload["chat_id"] = json!(cid);
    }
    if let Some(paths) = selected_paths {
        payload["selected_paths"] = json!(paths);
    }
    serde_json::to_string(&payload).unwrap_or_default()
}

/// Shell-quote a string (simple: wrap in single quotes, escape inner quotes).
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &RpCmd, json: bool) {
    match cmd {
        RpCmd::Windows => cmd_windows(json),
        RpCmd::PickWindow { repo_root } => cmd_pick_window(json, repo_root),
        RpCmd::EnsureWorkspace { window, repo_root } => cmd_ensure_workspace(json, *window, repo_root),
        RpCmd::Builder { window, summary, response_type } => cmd_builder(json, *window, summary, response_type.as_deref()),
        RpCmd::PromptGet { window, tab } => cmd_prompt_get(*window, tab),
        RpCmd::PromptSet { window, tab, message_file } => cmd_prompt_set(*window, tab, message_file),
        RpCmd::SelectGet { window, tab } => cmd_select_get(*window, tab),
        RpCmd::SelectAdd { window, tab, paths } => cmd_select_add(*window, tab, paths),
        RpCmd::ChatSend { window, tab, message_file, new_chat, chat_name, chat_id, mode, selected_paths } => {
            cmd_chat_send(json, *window, tab, message_file, *new_chat, chat_name.as_deref(), chat_id.as_deref(), mode, selected_paths.as_ref().map(|v| v.as_slice()));
        }
        RpCmd::PromptExport { window, tab, out } => cmd_prompt_export(*window, tab, out),
        RpCmd::SetupReview { repo_root, summary, response_type, create } => {
            cmd_setup_review(json, repo_root, summary, response_type.as_deref(), *create);
        }
        RpCmd::PrepChat { id: _, message_file, mode, new_chat, chat_name, selected_paths, output } => {
            cmd_prep_chat(message_file, mode, *new_chat, chat_name.as_deref(), selected_paths.as_ref().map(|v| v.as_slice()), output.as_deref());
        }
    }
}

// ── Command implementations ─────────────────────────────────────────

fn cmd_windows(json_mode: bool) {
    let (stdout, _) = run_rp_cli(&["--raw-json", "-e", "windows"], None);
    if json_mode {
        let windows = parse_windows(&stdout);
        println!("{}", serde_json::to_string(&windows).unwrap_or_default());
    } else {
        print!("{stdout}");
    }
}

fn cmd_pick_window(json_mode: bool, repo_root: &str) {
    let roots = normalize_repo_root(repo_root);
    let (stdout, _) = run_rp_cli(&["--raw-json", "-e", "windows"], None);
    let windows = parse_windows(&stdout);

    // Single window with no root paths — use it
    if windows.len() == 1 && extract_root_paths(&windows[0]).is_empty() {
        if let Some(win_id) = extract_window_id(&windows[0]) {
            if json_mode {
                println!("{}", json!({"window": win_id}));
            } else {
                println!("{win_id}");
            }
            return;
        }
    }

    // Match by root path
    for win in &windows {
        if let Some(win_id) = extract_window_id(win) {
            for path in extract_root_paths(win) {
                if roots.contains(&path) {
                    if json_mode {
                        println!("{}", json!({"window": win_id}));
                    } else {
                        println!("{win_id}");
                    }
                    return;
                }
            }
        }
    }

    error_exit("No window matches repo root");
}

fn cmd_ensure_workspace(_json_mode: bool, window: i64, repo_root: &str) {
    let real_root = fs::canonicalize(repo_root)
        .unwrap_or_else(|_| std::path::PathBuf::from(repo_root))
        .to_string_lossy()
        .to_string();
    let ws_name = Path::new(&real_root)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // List workspaces
    let list_payload = serde_json::to_string(&json!({"action": "list"})).unwrap();
    let list_expr = format!("call manage_workspaces {list_payload}");
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["--raw-json", "-w", &win_str, "-e", &list_expr], None);

    // Check if workspace exists
    let ws_exists = if let Ok(data) = serde_json::from_str::<serde_json::Value>(&stdout) {
        extract_workspace_names(&data).contains(&ws_name)
    } else {
        false
    };

    // Create if needed
    if !ws_exists {
        let create_payload = serde_json::to_string(&json!({
            "action": "create",
            "name": ws_name,
            "folder_path": real_root,
        })).unwrap();
        let create_expr = format!("call manage_workspaces {create_payload}");
        run_rp_cli(&["-w", &win_str, "-e", &create_expr], None);
    }

    // Switch to workspace
    let switch_payload = serde_json::to_string(&json!({
        "action": "switch",
        "workspace": ws_name,
        "window_id": window,
    })).unwrap();
    let switch_expr = format!("call manage_workspaces {switch_payload}");
    run_rp_cli(&["-w", &win_str, "-e", &switch_expr], None);
}

/// Extract workspace names from the list response.
fn extract_workspace_names(data: &serde_json::Value) -> Vec<String> {
    let list = if let Some(ws) = data.get("workspaces") {
        ws
    } else if let Some(r) = data.get("result") {
        r
    } else {
        data
    };

    if let Some(arr) = list.as_array() {
        arr.iter().filter_map(|item| {
            if let Some(s) = item.as_str() {
                return Some(s.to_string());
            }
            if let Some(obj) = item.as_object() {
                for key in &["name", "workspace", "title"] {
                    if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
                        return Some(v.to_string());
                    }
                }
            }
            None
        }).collect()
    } else {
        Vec::new()
    }
}

fn cmd_builder(json_mode: bool, window: i64, summary: &str, response_type: Option<&str>) {
    let summary_json = serde_json::to_string(summary).unwrap_or_else(|_| format!("\"{summary}\""));
    let mut builder_expr = format!("builder {summary_json}");
    if let Some(rt) = response_type {
        builder_expr.push_str(&format!(" --type {rt}"));
    }

    let win_str = window.to_string();
    let mut args: Vec<&str> = Vec::new();
    if response_type.is_some() {
        args.extend_from_slice(&["--raw-json"]);
    }
    args.extend_from_slice(&["-w", &win_str, "-e", &builder_expr]);

    let (stdout, stderr) = run_rp_cli(&args, None);
    let output = format!("{stdout}{}", if stderr.is_empty() { String::new() } else { format!("\n{stderr}") });

    if response_type == Some("review") {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&stdout) {
            let tab = data.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
            let chat_id = data.get("review").and_then(|v| v.get("chat_id")).and_then(|v| v.as_str()).unwrap_or("");
            let review_response = data.get("review").and_then(|v| v.get("response")).and_then(|v| v.as_str()).unwrap_or("");

            if json_mode {
                json_output(json!({
                    "window": window,
                    "tab": tab,
                    "chat_id": chat_id,
                    "review": review_response,
                    "file_count": data.get("file_count").and_then(|v| v.as_i64()).unwrap_or(0),
                    "total_tokens": data.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                }));
            } else {
                println!("T={tab} CHAT_ID={chat_id}");
                if !review_response.is_empty() {
                    println!("{review_response}");
                }
            }
            return;
        }
        // Fallback to tab parsing
    }

    match parse_builder_tab(&output) {
        Some(tab) => {
            if json_mode {
                json_output(json!({"window": window, "tab": tab}));
            } else {
                println!("{tab}");
            }
        }
        None => error_exit("builder output missing Tab id"),
    }
}

fn cmd_prompt_get(window: i64, tab: &str) {
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", "prompt get"], None);
    print!("{stdout}");
}

fn cmd_prompt_set(window: i64, tab: &str, message_file: &str) {
    let message = fs::read_to_string(message_file)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read message file: {e}")));
    let payload = serde_json::to_string(&json!({"op": "set", "text": message})).unwrap();
    let expr = format!("call prompt {payload}");
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", &expr], None);
    print!("{stdout}");
}

fn cmd_select_get(window: i64, tab: &str) {
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", "select get"], None);
    print!("{stdout}");
}

fn cmd_select_add(window: i64, tab: &str, paths: &[String]) {
    if paths.is_empty() {
        error_exit("select-add requires at least one path");
    }
    let quoted: Vec<String> = paths.iter().map(|p| shell_quote(p)).collect();
    let expr = format!("select add {}", quoted.join(" "));
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", &expr], None);
    print!("{stdout}");
}

fn cmd_chat_send(
    json_mode: bool,
    window: i64,
    tab: &str,
    message_file: &str,
    new_chat: bool,
    chat_name: Option<&str>,
    chat_id: Option<&str>,
    mode: &str,
    selected_paths: Option<&[String]>,
) {
    let message = fs::read_to_string(message_file)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read message file: {e}")));
    let payload = build_chat_payload(
        &message,
        mode,
        new_chat,
        chat_name,
        chat_id,
        selected_paths,
    );
    let expr = format!("call chat_send {payload}");
    let win_str = window.to_string();
    let (stdout, stderr) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", &expr], None);
    let output = format!("{stdout}{}", if stderr.is_empty() { String::new() } else { format!("\n{stderr}") });

    let cid = parse_chat_id(&output);
    if json_mode {
        println!("{}", json!({"chat": cid}));
    } else {
        print!("{stdout}");
    }
}

fn cmd_prompt_export(window: i64, tab: &str, out: &str) {
    let quoted_out = shell_quote(out);
    let expr = format!("prompt export {quoted_out}");
    let win_str = window.to_string();
    let (stdout, _) = run_rp_cli(&["-w", &win_str, "-t", tab, "-e", &expr], None);
    print!("{stdout}");
}

fn cmd_setup_review(
    json_mode: bool,
    repo_root: &str,
    summary: &str,
    response_type: Option<&str>,
    create: bool,
) {
    let real_root = fs::canonicalize(repo_root)
        .unwrap_or_else(|_| std::path::PathBuf::from(repo_root))
        .to_string_lossy()
        .to_string();

    // Step 1: pick-window
    let roots = normalize_repo_root(&real_root);
    let (stdout, _) = run_rp_cli(&["--raw-json", "-e", "windows"], Some(30));
    let windows = parse_windows(&stdout);

    let mut win_id: Option<i64> = None;

    // Single window with no root paths — use it
    if windows.len() == 1 && extract_root_paths(&windows[0]).is_empty() {
        win_id = extract_window_id(&windows[0]);
    }

    // Match by root path
    if win_id.is_none() {
        for win in &windows {
            if let Some(wid) = extract_window_id(win) {
                for path in extract_root_paths(win) {
                    if roots.contains(&path) {
                        win_id = Some(wid);
                        break;
                    }
                }
                if win_id.is_some() { break; }
            }
        }
    }

    if win_id.is_none() {
        if create {
            let ws_name = Path::new(&real_root)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let create_cmd = format!(
                "workspace create {} --new-window --folder-path {}",
                shell_quote(&ws_name),
                shell_quote(&real_root),
            );
            let (stdout, _) = run_rp_cli(&["--raw-json", "-e", &create_cmd], None);
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&stdout) {
                win_id = data.get("window_id").and_then(|v| v.as_i64());
            }
            if win_id.is_none() {
                error_exit("Failed to create RP window");
            }
        } else {
            error_exit("No RepoPrompt window matches repo root");
        }
    }

    let win_id = win_id.unwrap();

    // Write state file for ralph-guard
    let mut hasher = Sha256::new();
    hasher.update(real_root.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let state_path = format!("/tmp/.ralph-pick-window-{}", &hash[..16]);
    let _ = fs::write(&state_path, format!("{win_id}\n{real_root}\n"));

    // Step 2: builder
    let summary_json = serde_json::to_string(summary).unwrap_or_else(|_| format!("\"{summary}\""));
    let mut builder_expr = format!("builder {summary_json}");
    if let Some(rt) = response_type {
        builder_expr.push_str(&format!(" --type {rt}"));
    }

    let win_str = win_id.to_string();
    let mut args: Vec<&str> = Vec::new();
    args.push("-w");
    args.push(&win_str);
    if response_type.is_some() {
        args.push("--raw-json");
    }
    args.push("-e");
    args.push(&builder_expr);

    let (stdout, stderr) = run_rp_cli(&args, Some(1000));
    let output = format!("{stdout}{}", if stderr.is_empty() { String::new() } else { format!("\n{stderr}") });

    if response_type == Some("review") {
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(data) => {
                let tab = data.get("tab_id").and_then(|v| v.as_str()).unwrap_or("");
                let chat_id = data.get("review").and_then(|v| v.get("chat_id")).and_then(|v| v.as_str()).unwrap_or("");
                let review_response = data.get("review").and_then(|v| v.get("response")).and_then(|v| v.as_str()).unwrap_or("");

                if tab.is_empty() {
                    error_exit("Builder did not return a tab id");
                }

                if json_mode {
                    json_output(json!({
                        "window": win_id,
                        "tab": tab,
                        "chat_id": chat_id,
                        "review": review_response,
                        "repo_root": real_root,
                        "file_count": data.get("file_count").and_then(|v| v.as_i64()).unwrap_or(0),
                        "total_tokens": data.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                    }));
                } else {
                    println!("W={win_id} T={tab} CHAT_ID={chat_id}");
                    if !review_response.is_empty() {
                        println!("{review_response}");
                    }
                }
            }
            Err(_) => error_exit("Failed to parse builder review response"),
        }
    } else {
        match parse_builder_tab(&output) {
            Some(tab) => {
                if json_mode {
                    json_output(json!({"window": win_id, "tab": tab, "repo_root": real_root}));
                } else {
                    println!("W={win_id} T={tab}");
                }
            }
            None => error_exit("Builder did not return a tab id"),
        }
    }
}

fn cmd_prep_chat(
    message_file: &str,
    mode: &str,
    new_chat: bool,
    chat_name: Option<&str>,
    selected_paths: Option<&[String]>,
    output_file: Option<&str>,
) {
    let message = fs::read_to_string(message_file)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read message file: {e}")));
    let json_str = build_chat_payload(
        &message,
        mode,
        new_chat,
        chat_name,
        None,
        selected_paths,
    );

    if let Some(out) = output_file {
        fs::write(out, &json_str)
            .unwrap_or_else(|e| error_exit(&format!("Failed to write output file: {e}")));
        eprintln!("Wrote {out}");
    } else {
        println!("{json_str}");
    }
}
