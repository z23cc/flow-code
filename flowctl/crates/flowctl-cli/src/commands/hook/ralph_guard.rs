//! Ralph Guard hook: enforce Ralph workflow rules (Pre/PostToolUse, Stop hooks).

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::{json, Value};

use super::common::{
    find_flowctl_for_guard, get_repo_root, is_memory_enabled, normalize_command, output_block,
    output_json_and_exit, pathdiff_relative, read_stdin_json,
};

/// Ralph guard version for drift detection.
#[allow(dead_code)]
const RALPH_GUARD_VERSION: &str = "0.15.0";

/// Files that Ralph must never modify during a run.
const PROTECTED_FILE_PATTERNS: [&str; 5] = [
    "ralph-guard.py",
    "ralph-guard",
    "flowctl",
    "/hooks/hooks.json",
    "/flowctl/",
];

/// Max debug log size before rotation (1 MB).
const LOG_MAX_BYTES: u64 = 1_048_576;

pub fn cmd_ralph_guard() {
    let debug_file = debug_log_path();
    append_log(
        &debug_file,
        &format!(
            "[{}] Hook called\n",
            env::var("FLOW_RALPH").unwrap_or_else(|_| "unset".into())
        ),
    );

    // Early exit if not in Ralph mode
    if env::var("FLOW_RALPH").as_deref() != Ok("1") {
        append_log(&debug_file, "  -> Exiting: FLOW_RALPH not set to 1\n");
        std::process::exit(0);
    }

    let data = read_stdin_json();
    let event = data
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tool_name = data
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    append_log(
        &debug_file,
        &format!("  -> Event: {event}, Tool: {tool_name}\n"),
    );

    // Block Edit/Write to protected files and enforce file locks
    if event == "PreToolUse" && (tool_name == "Edit" || tool_name == "Write") {
        handle_protected_file_check(&data);
        handle_file_lock_check(&data);
        std::process::exit(0);
    }

    // Only process Bash tool calls for Pre/Post
    if (event == "PreToolUse" || event == "PostToolUse") && tool_name != "Bash" {
        append_log(&debug_file, "  -> Skipping: not Bash\n");
        std::process::exit(0);
    }

    match event {
        "PreToolUse" => handle_pre_tool_use(&data),
        "PostToolUse" => handle_post_tool_use(&data),
        "Stop" => handle_stop(&data),
        "SubagentStop" => handle_stop(&data),
        _ => std::process::exit(0),
    }
}

// ── State management ──────────────────────────────────────────────────

fn get_state_dir() -> PathBuf {
    if let Ok(run_dir) = env::var("RUN_DIR") {
        let p = PathBuf::from(&run_dir);
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from(env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()))
}

fn get_state_file(session_id: &str) -> PathBuf {
    get_state_dir().join(format!("ralph-guard-{session_id}.json"))
}

fn load_state(session_id: &str) -> GuardState {
    let state_file = get_state_file(session_id);
    if state_file.exists() {
        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(v) = serde_json::from_str::<Value>(&content) {
                return GuardState::from_json(&v);
            }
        }
    }
    GuardState::default()
}

fn save_state(session_id: &str, state: &GuardState) {
    let state_file = get_state_file(session_id);
    let v = state.to_json();
    let _ = fs::write(&state_file, serde_json::to_string(&v).unwrap_or_default());
}

#[derive(Default)]
struct GuardState {
    chats_sent: u64,
    last_verdict: Option<String>,
    window: Option<String>,
    tab: Option<String>,
    chat_send_succeeded: bool,
    flowctl_done_called: HashSet<String>,
    codex_review_succeeded: bool,
}

impl GuardState {
    fn from_json(v: &Value) -> Self {
        let mut state = Self::default();
        if let Some(n) = v.get("chats_sent").and_then(serde_json::Value::as_u64) {
            state.chats_sent = n;
        }
        state.last_verdict = v
            .get("last_verdict")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        state.window = v
            .get("window")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        state.tab = v
            .get("tab")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);
        if let Some(b) = v.get("chat_send_succeeded").and_then(serde_json::Value::as_bool) {
            state.chat_send_succeeded = b;
        }
        if let Some(b) = v.get("codex_review_succeeded").and_then(serde_json::Value::as_bool) {
            state.codex_review_succeeded = b;
        }
        if let Some(arr) = v.get("flowctl_done_called").and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(s) = item.as_str() {
                    state.flowctl_done_called.insert(s.to_string());
                }
            }
        }
        state
    }

    fn to_json(&self) -> Value {
        let done_called: Vec<&String> = self.flowctl_done_called.iter().collect();
        json!({
            "chats_sent": self.chats_sent,
            "last_verdict": self.last_verdict,
            "window": self.window,
            "tab": self.tab,
            "chat_send_succeeded": self.chat_send_succeeded,
            "flowctl_done_called": done_called,
            "codex_review_succeeded": self.codex_review_succeeded,
        })
    }
}

// ── Debug logging ─────────────────────────────────────────────────────

fn debug_log_path() -> PathBuf {
    let log = if let Ok(run_dir) = env::var("RUN_DIR") {
        PathBuf::from(run_dir).join("guard-debug.log")
    } else {
        PathBuf::from("/tmp/ralph-guard-debug.log")
    };
    // Rotate if > 1MB
    if let Ok(meta) = fs::metadata(&log) {
        if meta.len() > LOG_MAX_BYTES {
            let rotated = log.with_extension("log.1");
            let _ = fs::rename(&log, &rotated);
        }
    }
    log
}

fn append_log(path: &Path, msg: &str) {
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(msg.as_bytes())
        });
}

// ── Protected file check ──────────────────────────────────────────────

fn handle_protected_file_check(data: &Value) {
    let file_path = data
        .get("tool_input")
        .and_then(|v| v.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if file_path.is_empty() {
        return;
    }
    for pattern in &PROTECTED_FILE_PATTERNS {
        if file_path.ends_with(pattern) {
            let basename = Path::new(file_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.to_string());
            output_block(&format!(
                "BLOCKED: Cannot modify protected file '{basename}'. \
                 Ralph must not edit its own workflow tooling (ralph-guard, flowctl, hooks). \
                 If the guard is blocking incorrectly, report the bug instead of bypassing it."
            ));
        }
    }
}

// ── File lock check ───────────────────────────────────────────────────

fn handle_file_lock_check(data: &Value) {
    if env::var("FLOW_TEAMS").as_deref() != Ok("1") {
        return;
    }

    let file_path = data
        .get("tool_input")
        .and_then(|v| v.get("file_path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if file_path.is_empty() {
        return;
    }

    let my_task_id = match env::var("FLOW_TASK_ID") {
        Ok(id) if !id.is_empty() => id,
        _ => return, // No task context -- fail-open
    };

    let repo_root = get_repo_root();
    let rel_path = pathdiff_relative(file_path, &repo_root);

    let flowctl = find_flowctl_for_guard();
    let Some(flowctl) = flowctl else { return };

    let result = Command::new(&flowctl)
        .args(["lock", "--task", &my_task_id, "--files", &rel_path, "--json"])
        .current_dir(&repo_root)
        .output();

    let output = match result {
        Ok(o) => o,
        Err(_) => return, // Fail-open
    };

    if output.status.success() {
        return; // Lock acquired or already owned
    }

    let error_text = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);
    let owner_re = Regex::new(r#"(?i)locked by ['"]?([^'"\s]+)"#).expect("static regex must compile");
    let owner = owner_re
        .captures(&error_text)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "another task".into());

    output_block(&format!(
        "BLOCKED: File '{rel_path}' is locked by task '{owner}'. \
         Your task ({my_task_id}) does not own this file. \
         Request access via 'Need file access:' protocol message or work on your own files."
    ));
}

// ── PreToolUse handler ────────────────────────────────────────────────

fn handle_pre_tool_use(data: &Value) {
    let command = normalize_command(
        data.get("tool_input")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Check for chat-send commands
    if command.contains("chat-send") {
        if Regex::new(r"chat-send.*--json")
            .expect("static regex must compile")
            .is_match(&command)
        {
            output_block(
                "BLOCKED: Do not use --json with chat-send. \
                 It suppresses the review text. Remove --json flag.",
            );
        }

        if command.contains("--new-chat") {
            let state = load_state(session_id);
            if state.chats_sent > 0 {
                output_block(
                    "BLOCKED: Do not use --new-chat for re-reviews. \
                     Stay in the same chat so reviewer has context. \
                     Remove --new-chat flag.",
                );
            }
        }
    }

    // Block direct codex calls (must use flowctl codex wrappers)
    if Regex::new(r"\bcodex\b").expect("static regex must compile").is_match(&command) {
        let is_wrapper = Regex::new(r"flowctl\s+codex|FLOWCTL.*codex")
            .expect("static regex must compile")
            .is_match(&command);
        if !is_wrapper {
            if Regex::new(r"\bcodex\s+exec\b")
                .expect("static regex must compile")
                .is_match(&command)
            {
                output_block(
                    "BLOCKED: Do not call 'codex exec' directly. \
                     Use 'flowctl codex impl-review' or 'flowctl codex plan-review' \
                     to ensure proper receipt handling and session continuity.",
                );
            }
            if Regex::new(r"\bcodex\s+review\b")
                .expect("static regex must compile")
                .is_match(&command)
            {
                output_block(
                    "BLOCKED: Do not call 'codex review' directly. \
                     Use 'flowctl codex impl-review' or 'flowctl codex plan-review'.",
                );
            }
        }
        if Regex::new(r"--last\b").expect("static regex must compile").is_match(&command) {
            output_block(
                "BLOCKED: Do not use '--last' with codex. \
                 Session continuity is managed via session_id in receipts.",
            );
        }
    }

    // Validate setup-review usage
    if command.contains("setup-review") {
        if !command.contains("--repo-root") {
            output_block(
                "BLOCKED: setup-review requires --repo-root flag. \
                 Use: setup-review --repo-root \"$REPO_ROOT\" --summary \"...\"",
            );
        }
        if !command.contains("--summary") {
            output_block(
                "BLOCKED: setup-review requires --summary flag. \
                 Use: setup-review --repo-root \"$REPO_ROOT\" --summary \"...\"",
            );
        }
    }

    // Validate select-add has --window
    if command.contains("select-add") && !command.contains("--window") {
        output_block(
            "BLOCKED: select-add requires --window flag. \
             Use: select-add --window \"$W\" --tab \"$T\" <path>",
        );
    }

    // Enforce flowctl done requires --evidence-json and --summary-file
    if command.contains(" done ")
        && (command.contains("flowctl") || command.contains("FLOWCTL"))
        && !Regex::new(r"--help|-h").expect("static regex must compile").is_match(&command)
    {
        if !Regex::new(r"--evidence-json|--evidence")
            .expect("static regex must compile")
            .is_match(&command)
        {
            output_block(
                "BLOCKED: flowctl done requires --evidence-json flag. \
                 You must capture commit SHAs and test commands. \
                 Use: flowctl done <task> --summary-file <s.md> --evidence-json <e.json>",
            );
        }
        if !Regex::new(r"--summary-file|--summary")
            .expect("static regex must compile")
            .is_match(&command)
        {
            output_block(
                "BLOCKED: flowctl done requires --summary-file flag. \
                 You must write a done summary. \
                 Use: flowctl done <task> --summary-file <s.md> --evidence-json <e.json>",
            );
        }
    }

    // Block receipt writes unless chat-send has succeeded + validate format
    let receipt_path = env::var("REVIEW_RECEIPT_PATH").unwrap_or_default();
    if !receipt_path.is_empty() {
        let receipt_dir = Path::new(&receipt_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !receipt_dir.is_empty() {
            let is_receipt_write = Regex::new(&format!(r#">\s*['"]?{}"#, regex::escape(&receipt_dir)))
                .map(|re| re.is_match(&command))
                .unwrap_or(false)
                || Regex::new(r#">\s*['"]?.*receipts/.*\.json"#)
                    .expect("static regex must compile")
                    .is_match(&command)
                || Regex::new(r"(?i)cat\s*>\s*.*receipt")
                    .expect("static regex must compile")
                    .is_match(&command);

            if is_receipt_write {
                let state = load_state(session_id);
                if !state.chat_send_succeeded && !state.codex_review_succeeded {
                    output_block(
                        "BLOCKED: Cannot write receipt before review completes. \
                         You must run 'flowctl rp chat-send' or 'flowctl codex impl-review/plan-review' \
                         and receive a review response before writing the receipt.",
                    );
                }
                // Validate receipt has required 'id' field
                if !command.contains("\"id\"") && !command.contains("'id'") {
                    output_block(
                        "BLOCKED: Receipt JSON is missing required 'id' field. \
                         Receipt must include: {\"type\":\"...\",\"id\":\"<TASK_OR_EPIC_ID>\",...} \
                         Copy the exact command from the prompt template.",
                    );
                }
                // Validate completion_review receipts have verdict field
                if (command.contains("completion_review") || receipt_path.contains("completion-"))
                    && !command.contains("\"verdict\"")
                    && !command.contains("'verdict'")
                {
                    output_block(
                        "BLOCKED: Receipt JSON is missing required 'verdict' field. \
                         Completion review receipts must include: {\"verdict\":\"SHIP\",...} \
                         Copy the exact command from the prompt template.",
                    );
                }
                // For impl receipts, verify flowctl done was called
                if command.contains("impl_review") {
                    let id_re = Regex::new(r#""id"\s*:\s*"([^"]+)""#).expect("static regex must compile");
                    if let Some(caps) = id_re.captures(&command) {
                        let task_id = &caps[1];
                        if !state.flowctl_done_called.contains(task_id) {
                            output_block(&format!(
                                "BLOCKED: Cannot write impl receipt for {task_id} - flowctl done was not called. \
                                 You MUST run 'flowctl done {task_id} --evidence ...' BEFORE writing the receipt. \
                                 The task is NOT complete until flowctl done succeeds."
                            ));
                        }
                    }
                }
            }
        }
    }

    // All checks passed
    std::process::exit(0);
}

// ── PostToolUse handler ───────────────────────────────────────────────

fn handle_post_tool_use(data: &Value) {
    let command = normalize_command(
        data.get("tool_input")
            .and_then(|v| v.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Get response text
    let response_text = match data.get("tool_response") {
        Some(Value::Object(map)) => map
            .get("stdout")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| serde_json::to_string(&Value::Object(map.clone())).unwrap_or_default()),
        Some(Value::String(s)) => s.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => String::new(),
    };

    let mut state = load_state(session_id);
    let debug_file = debug_log_path();

    // Track chat-send calls
    if command.contains("chat-send") {
        if response_text.contains("Chat Send") && !response_text.contains("{\"chat\": null}") {
            state.chats_sent += 1;
            state.chat_send_succeeded = true;
            save_state(session_id, &state);
        } else if response_text.contains("{\"chat\": null}")
            || response_text.contains("{\"chat\":null}")
        {
            state.chat_send_succeeded = false;
            save_state(session_id, &state);
        }
    }

    // Track codex review calls
    if command.contains("flowctl")
        && command.contains("codex")
        && (command.contains("impl-review")
            || command.contains("plan-review")
            || command.contains("completion-review"))
    {
        let verdict_re =
            Regex::new(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>").expect("static regex must compile");
        if let Some(caps) = verdict_re.captures(&response_text) {
            state.codex_review_succeeded = true;
            state.last_verdict = Some(caps[1].to_string());
            save_state(session_id, &state);
        }
    }

    // Track flowctl done calls
    if command.contains(" done ")
        && (command.contains("flowctl") || command.contains("FLOWCTL"))
    {
        append_log(
            &debug_file,
            &format!(
                "  -> flowctl done detected in: {}...\n",
                &command[..command.len().min(100)]
            ),
        );

        let done_re = Regex::new(r"\bdone\s+([a-zA-Z0-9][a-zA-Z0-9._-]*)").expect("static regex must compile");
        if let Some(caps) = done_re.captures(&command) {
            let task_id = caps[1].to_string();
            append_log(
                &debug_file,
                &format!(
                    "  -> Extracted task_id: {task_id}, response has 'status': {}\n",
                    response_text.to_lowercase().contains("status")
                ),
            );

            let response_lower = response_text.to_lowercase();
            if response_lower.contains("status")
                || response_lower.contains("done")
                || response_lower.contains("updated")
                || response_lower.contains("completed")
            {
                state.flowctl_done_called.insert(task_id.clone());
                save_state(session_id, &state);
                append_log(
                    &debug_file,
                    &format!(
                        "  -> Added {task_id} to flowctl_done_called: {:?}\n",
                        state.flowctl_done_called
                    ),
                );
            }
        }
    }

    // Track receipt writes - reset review state after write
    let receipt_path = env::var("REVIEW_RECEIPT_PATH").unwrap_or_default();
    if !receipt_path.is_empty() {
        let receipt_dir = Path::new(&receipt_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !receipt_dir.is_empty() {
            let is_receipt_write = Regex::new(&format!(
                r#">\s*['"]?{}"#,
                regex::escape(&receipt_dir)
            ))
            .map(|re| re.is_match(&command))
            .unwrap_or(false)
                || Regex::new(r#">\s*['"]?.*receipts/.*\.json"#)
                    .expect("static regex must compile")
                    .is_match(&command)
                || Regex::new(r"(?i)cat\s*>\s*.*receipt")
                    .expect("static regex must compile")
                    .is_match(&command);

            if is_receipt_write {
                state.chat_send_succeeded = false;
                state.codex_review_succeeded = false;
                save_state(session_id, &state);
            }
        }
    }

    // Track setup-review output (W= T=)
    if command.contains("setup-review") {
        let w_re = Regex::new(r"W=(\d+)").expect("static regex must compile");
        let t_re = Regex::new(r"(?i)T=([A-F0-9-]+)").expect("static regex must compile");
        if let Some(caps) = w_re.captures(&response_text) {
            state.window = Some(caps[1].to_string());
        }
        if let Some(caps) = t_re.captures(&response_text) {
            state.tab = Some(caps[1].to_string());
        }
        save_state(session_id, &state);
    }

    // Check for verdict in response
    let verdict_re =
        Regex::new(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>").expect("static regex must compile");
    if let Some(caps) = verdict_re.captures(&response_text) {
        let verdict = caps[1].to_string();
        state.last_verdict = Some(verdict.clone());
        save_state(session_id, &state);

        if verdict == "SHIP" {
            let receipt_path = env::var("REVIEW_RECEIPT_PATH").unwrap_or_default();
            if !receipt_path.is_empty()
                && !Path::new(&receipt_path).exists()
                && state.chat_send_succeeded
            {
                let (receipt_type, item_id) = parse_receipt_path(&receipt_path);
                let cmd = format!(
                    "mkdir -p \"$(dirname '{receipt_path}')\"\n\
                     ts=\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"\n\
                     cat > '{receipt_path}' <<EOF\n\
                     {{\"type\":\"{receipt_type}\",\"id\":\"{item_id}\",\"mode\":\"rp\",\"verdict\":\"SHIP\",\"timestamp\":\"$ts\"}}\n\
                     EOF"
                );
                output_json_and_exit(&json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "additionalContext": format!(
                            "IMPORTANT: SHIP verdict received. You MUST now write the receipt. \
                             Run this command:\n{cmd}"
                        ),
                    }
                }));
            }
        } else if (verdict == "NEEDS_WORK" || verdict == "MAJOR_RETHINK")
            && is_memory_enabled()
        {
            output_json_and_exit(&json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext":
                        "MEMORY: Review returned NEEDS_WORK. After fixing, consider if any lessons are \
                         GENERALIZABLE (apply beyond this task). If so, capture with:\n\
                           flowctl memory add --type <type> \"<one-line lesson>\"\n\
                         Types: pitfall (gotchas/mistakes), convention (patterns to follow), decision (architectural choices)\n\
                         Skip: task-specific fixes, typos, style issues, or 'fine as-is' explanations.",
                }
            }));
        }
    } else if command.contains("chat-send") && response_text.contains("Chat Send") {
        // chat-send returned but no verdict tag found
        let informal_re =
            Regex::new(r"(?i)\bLGTM\b|\bLooks good\b|\bApproved\b|\bNo issues\b").expect("static regex must compile");
        if informal_re.is_match(&response_text) {
            output_json_and_exit(&json!({
                "hookSpecificOutput": {
                    "hookEventName": "PostToolUse",
                    "additionalContext":
                        "WARNING: Reviewer responded with informal approval (LGTM/Looks good) \
                         but did NOT use the required <verdict>SHIP</verdict> tag. \
                         This means your review prompt was incorrect. \
                         You MUST use /flow-code:impl-review skill which has the correct prompt format. \
                         Do NOT improvise review prompts. Re-invoke the skill and try again.",
                }
            }));
        }
    }

    // Check for {"chat": null}
    if (response_text.contains("{\"chat\":") || response_text.contains("{\"chat\": "))
        && response_text.contains("null")
    {
        output_json_and_exit(&json!({
            "decision": "block",
            "reason": "ERROR: chat-send returned {\"chat\": null} which means --json was used. \
                       This suppresses the review text. Re-run without --json flag.",
        }));
    }

    std::process::exit(0);
}

// ── Stop handler ──────────────────────────────────────────────────────

fn handle_stop(data: &Value) {
    let session_id = data
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let stop_hook_active = data
        .get("stop_hook_active")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Prevent infinite loops
    if stop_hook_active {
        std::process::exit(0);
    }

    let receipt_path = env::var("REVIEW_RECEIPT_PATH").unwrap_or_default();

    if !receipt_path.is_empty() && !Path::new(&receipt_path).exists() {
        let (receipt_type, item_id) = parse_receipt_path(&receipt_path);
        let (skill, skill_desc) = match receipt_type.as_str() {
            "impl_review" => ("/flow-code:impl-review", "implementation review"),
            "completion_review" => ("/flow-code:epic-review", "epic completion review"),
            _ => ("/flow-code:plan-review", "plan review"),
        };
        output_json_and_exit(&json!({
            "decision": "block",
            "reason": format!(
                "Cannot stop: {skill_desc} not completed.\n\
                 You MUST invoke `{skill} {item_id}` to complete the review.\n\
                 The skill writes the receipt on SHIP verdict.\n\
                 Do NOT write the receipt manually - that skips the actual review."
            ),
        }));
    }

    // Clean up state file
    let state_file = get_state_file(session_id);
    if state_file.exists() {
        let _ = fs::remove_file(&state_file);
    }

    std::process::exit(0);
}

// ── Receipt path parsing ──────────────────────────────────────────────

fn parse_receipt_path(receipt_path: &str) -> (String, String) {
    let basename = Path::new(receipt_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let suffix_pattern = r"(?:-[a-z0-9][a-z0-9-]*[a-z0-9]|-[a-z0-9]{1,3})?";

    // Try plan pattern: plan-fn-N.json, plan-fn-N-xxx.json, plan-fn-N-slug.json
    let plan_re = Regex::new(&format!(r"^plan-(fn-\d+{suffix_pattern})\.json$")).expect("static regex must compile");
    if let Some(caps) = plan_re.captures(&basename) {
        return ("plan_review".into(), caps[1].to_string());
    }

    // Try impl pattern: impl-fn-N.M.json, impl-fn-N-xxx.M.json
    let impl_re =
        Regex::new(&format!(r"^impl-(fn-\d+{suffix_pattern}\.\d+)\.json$")).expect("static regex must compile");
    if let Some(caps) = impl_re.captures(&basename) {
        return ("impl_review".into(), caps[1].to_string());
    }

    // Try completion pattern
    let completion_re =
        Regex::new(&format!(r"^completion-(fn-\d+{suffix_pattern})\.json$")).expect("static regex must compile");
    if let Some(caps) = completion_re.captures(&basename) {
        return ("completion_review".into(), caps[1].to_string());
    }

    // Fallback
    ("impl_review".into(), "UNKNOWN".into())
}
