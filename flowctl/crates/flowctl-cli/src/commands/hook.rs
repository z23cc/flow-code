//! Hook subcommands: auto-memory and ralph-guard.
//!
//! These are Claude Code hook scripts ported from Python to Rust.
//! They read JSON from stdin, perform validation/extraction, and use
//! exit codes 0 (allow) and 2 (block) per the hook protocol.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Subcommand;
use regex::Regex;
use serde_json::{json, Value};

use crate::output::pretty_output;

#[derive(Subcommand, Debug)]
pub enum HookCmd {
    /// Extract session memories from transcript (Stop hook).
    AutoMemory,
    /// Enforce Ralph workflow rules (Pre/PostToolUse, Stop hooks).
    RalphGuard,
    /// Gate git commit on flowctl guard pass (Pre/PostToolUse hook).
    CommitGate,
    /// Inject .flow/ state into compaction context (PreCompact hook).
    PreCompact,
    /// Inject active task context for subagents (SubagentStart hook).
    SubagentContext,
    /// Sync Claude task completion with .flow/ state (TaskCompleted hook).
    TaskCompleted,
    /// Rewrite Bash commands via rtk token optimizer (PreToolUse hook).
    RtkRewrite,
}

pub fn dispatch(cmd: &HookCmd) {
    match cmd {
        HookCmd::AutoMemory => cmd_auto_memory(),
        HookCmd::RalphGuard => cmd_ralph_guard(),
        HookCmd::CommitGate => cmd_commit_gate(),
        HookCmd::PreCompact => cmd_pre_compact(),
        HookCmd::SubagentContext => cmd_subagent_context(),
        HookCmd::TaskCompleted => cmd_task_completed(),
        HookCmd::RtkRewrite => cmd_rtk_rewrite(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Auto-memory
// ═══════════════════════════════════════════════════════════════════════

fn cmd_auto_memory() {
    let hook_input = read_stdin_json();

    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }
    if is_auto_memory_disabled(&flow_dir) {
        std::process::exit(0);
    }

    // Auto-init memory dir if missing
    let memory_dir = flow_dir.join("memory");
    if !memory_dir.exists() {
        let _ = fs::create_dir_all(&memory_dir);
        for (fname, header) in [
            ("pitfalls.md", "# Pitfalls\n\n<!-- Auto-captured by auto-memory hook -->\n"),
            ("conventions.md", "# Conventions\n\n<!-- Auto-captured by auto-memory hook -->\n"),
            ("decisions.md", "# Decisions\n\n<!-- Auto-captured by auto-memory hook -->\n"),
        ] {
            let _ = fs::write(memory_dir.join(fname), header);
        }
    }

    let text = read_transcript(&hook_input);
    if text.len() < 200 {
        std::process::exit(0);
    }

    // Default: AI summarization via gemini; fallback: pattern matching
    let (memories, method) = {
        let gemini_result = summarize_with_gemini(&text);
        if !gemini_result.is_empty() {
            (gemini_result, "gemini")
        } else {
            (extract_by_pattern(&text), "pattern")
        }
    };

    let saved = save_memories(&memories, &flow_dir);

    if saved > 0 {
        eprintln!("auto-memory: captured {saved} entries via {method}");
    }

    std::process::exit(0);
}

/// Find flow directory — delegates to shared resolver (git-common-dir aware).
fn get_flow_dir() -> PathBuf {
    super::helpers::get_flow_dir()
}

/// Check if auto-memory is explicitly disabled in config.
/// Default is ON -- only returns true if memory.auto is explicitly false.
fn is_auto_memory_disabled(flow_dir: &Path) -> bool {
    let config_path = flow_dir.join("config.json");
    if !config_path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(&config_path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    if let Some(mem) = config.get("memory") {
        if let Some(auto_val) = mem.get("auto") {
            return auto_val == &Value::Bool(false);
        }
    }
    false
}

/// Read assistant text from transcript JSONL file.
fn read_transcript(hook_input: &Value) -> String {
    let transcript_path = hook_input
        .get("transcript_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if transcript_path.is_empty() {
        return String::new();
    }
    let path = Path::new(transcript_path);
    if !path.exists() {
        return String::new();
    }

    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut texts = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(ev) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if ev.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        if let Some(blocks) = ev
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        {
            for blk in blocks {
                if blk.get("type").and_then(|v| v.as_str()) == Some("text") {
                    if let Some(t) = blk.get("text").and_then(|v| v.as_str()) {
                        texts.push(t.to_string());
                    }
                }
            }
        }
    }

    texts.join("\n")
}

// ── AI summarization ──────────────────────────────────────────────────

const SUMMARIZE_PROMPT: &str = r#"Analyze this AI coding session transcript and extract the most important learnings.

Output ONLY a JSON array of objects, each with "type" and "content" fields. No markdown, no explanation.

Types:
- "pitfall": things that went wrong, bugs found, things to avoid
- "convention": project patterns discovered, coding conventions learned
- "decision": architectural or design decisions made and why

Rules:
- Max 5 entries (only the most important)
- Each "content" should be one concise sentence (under 150 chars)
- Skip trivial things (file reads, git commands, routine operations)
- Focus on what would help a FUTURE session avoid mistakes or follow decisions
- If nothing important happened, return empty array: []

Example output:
[{"type":"pitfall","content":"Django select_related needed on UserProfile queries to avoid N+1"},{"type":"decision","content":"Chose per-epic review mode over per-task for faster Ralph runs"}]

Transcript:
"#;

const VALID_MEMORY_TYPES: [&str; 3] = ["pitfall", "convention", "decision"];

fn summarize_with_gemini(text: &str) -> Vec<Memory> {
    // Truncate to ~50k chars to fit context
    let truncated = if text.len() > 50000 {
        format!(
            "{}\n...[truncated]...\n{}",
            &text[..25000],
            &text[text.len() - 25000..]
        )
    } else {
        text.to_string()
    };

    let prompt = format!("{SUMMARIZE_PROMPT}{truncated}");

    let result = Command::new("gemini")
        .args(["-p", &prompt])
        .output();

    let output = match result {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return Vec::new(),
    };

    // Extract JSON array from output (may have surrounding text)
    let re = Regex::new(r"(?s)\[.*\]").unwrap();
    let json_str = match re.find(&output) {
        Some(m) => m.as_str(),
        None => return Vec::new(),
    };

    let arr: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };

    let mut valid = Vec::new();
    for m in &arr {
        if let (Some(t), Some(c)) = (
            m.get("type").and_then(|v| v.as_str()),
            m.get("content").and_then(|v| v.as_str()),
        ) {
            if VALID_MEMORY_TYPES.contains(&t) {
                let content = if c.len() > 200 { &c[..200] } else { c };
                valid.push(Memory {
                    mem_type: t.to_string(),
                    content: content.to_string(),
                });
            }
        }
        if valid.len() >= 5 {
            break;
        }
    }
    valid
}

// ── Pattern matching fallback ─────────────────────────────────────────

fn extract_by_pattern(text: &str) -> Vec<Memory> {
    if text.len() < 100 {
        return Vec::new();
    }

    let patterns: Vec<(Regex, &str)> = vec![
        (
            Regex::new(r"(?i)(?:decided|chose|chose to|went with|using .+ instead of|switched to)\s+(.{20,150})").unwrap(),
            "decision",
        ),
        (
            Regex::new(r"(?i)(?:found that|discovered|turns out|learned that|realized)\s+(.{20,150})").unwrap(),
            "convention",
        ),
        (
            Regex::new(r#"(?i)(?:don'?t|avoid|careful with|gotcha|warning|bug:|issue:|never)\s+(.{20,150})"#).unwrap(),
            "pitfall",
        ),
        (
            Regex::new(r"(?i)(?:fixed by|solved by|the (?:issue|problem|bug) was|root cause)\s+(.{20,150})").unwrap(),
            "pitfall",
        ),
    ];

    let mut memories = Vec::new();
    let mut seen = HashSet::new();
    let ws_re = Regex::new(r"\s+").unwrap();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.len() < 20 {
            continue;
        }
        let line_lower = trimmed.to_lowercase();
        for (pattern, mem_type) in &patterns {
            if let Some(m) = pattern.find(&line_lower) {
                let content_raw = m.as_str().trim();
                // Collapse whitespace
                let collapsed = ws_re.replace_all(content_raw, " ");
                let key: String = collapsed.chars().take(50).collect();
                if !seen.contains(&key) {
                    seen.insert(key);
                    let content: String = trimmed.chars().take(200).collect();
                    memories.push(Memory {
                        mem_type: mem_type.to_string(),
                        content,
                    });
                }
            }
            if memories.len() >= 5 {
                break;
            }
        }
        if memories.len() >= 5 {
            break;
        }
    }

    memories
}

// ── Save memories ─────────────────────────────────────────────────────

struct Memory {
    mem_type: String,
    content: String,
}

fn save_memories(memories: &[Memory], flow_dir: &Path) -> usize {
    if memories.is_empty() {
        return 0;
    }

    // Find flowctl binary
    let flowctl = find_flowctl(flow_dir);

    match flowctl {
        Some(bin) => save_via_flowctl(memories, &bin),
        None => save_memories_direct(memories, flow_dir),
    }
}

fn find_flowctl(flow_dir: &Path) -> Option<PathBuf> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let candidates = [
        flow_dir.join("bin").join("flowctl"),
        cwd.join("scripts").join("ralph").join("flowctl"),
        cwd.join("scripts").join("auto-improve").join("flowctl"),
    ];
    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }
    None
}

fn save_via_flowctl(memories: &[Memory], flowctl: &Path) -> usize {
    let mut saved = 0;
    for mem in memories {
        let result = Command::new(flowctl)
            .args(["memory", "add", "--type", &mem.mem_type, &mem.content])
            .output();
        if let Ok(o) = result {
            if o.status.success() {
                saved += 1;
            }
        }
    }
    saved
}

fn save_memories_direct(memories: &[Memory], flow_dir: &Path) -> usize {
    let memory_dir = flow_dir.join("memory");
    if !memory_dir.exists() {
        return 0;
    }

    let type_to_file: HashMap<&str, &str> = HashMap::from([
        ("pitfall", "pitfalls.md"),
        ("convention", "conventions.md"),
        ("decision", "decisions.md"),
    ]);

    let mut saved = 0;
    for mem in memories {
        let filename = type_to_file.get(mem.mem_type.as_str()).unwrap_or(&"conventions.md");
        let filepath = memory_dir.join(filename);
        if filepath.exists() {
            let content = fs::read_to_string(&filepath).unwrap_or_default();
            let entry = format!("- {}\n", mem.content);
            if !content.contains(&entry)
                && fs::OpenOptions::new()
                    .append(true)
                    .open(&filepath)
                    .and_then(|mut f| {
                        use std::io::Write;
                        f.write_all(entry.as_bytes())
                    })
                    .is_ok()
            {
                saved += 1;
            }
        }
    }
    saved
}

// ═══════════════════════════════════════════════════════════════════════
// Ralph Guard
// ═══════════════════════════════════════════════════════════════════════

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

fn cmd_ralph_guard() {
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
        if let Some(n) = v.get("chats_sent").and_then(|v| v.as_u64()) {
            state.chats_sent = n;
        }
        state.last_verdict = v
            .get("last_verdict")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        state.window = v
            .get("window")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        state.tab = v
            .get("tab")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(b) = v.get("chat_send_succeeded").and_then(|v| v.as_bool()) {
            state.chat_send_succeeded = b;
        }
        if let Some(b) = v.get("codex_review_succeeded").and_then(|v| v.as_bool()) {
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
    let owner_re = Regex::new(r#"(?i)locked by ['"]?([^'"\s]+)"#).unwrap();
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

fn get_repo_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
        }
        _ => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

fn find_flowctl_for_guard() -> Option<PathBuf> {
    if let Ok(f) = env::var("FLOWCTL") {
        let p = PathBuf::from(&f);
        if p.exists() {
            return Some(p);
        }
    }
    let plugin_root = env::var("DROID_PLUGIN_ROOT")
        .or_else(|_| env::var("CLAUDE_PLUGIN_ROOT"))
        .unwrap_or_default();
    if !plugin_root.is_empty() {
        let p = PathBuf::from(&plugin_root).join("bin").join("flowctl");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn pathdiff_relative(file_path: &str, base: &Path) -> String {
    let fp = Path::new(file_path);
    if let Ok(stripped) = fp.strip_prefix(base) {
        stripped.to_string_lossy().to_string()
    } else {
        file_path.to_string()
    }
}

// ── Normalize command ─────────────────────────────────────────────────

fn normalize_command(cmd: &str) -> String {
    let mut s = cmd.replace('\t', " ");
    s = s.replace("\"\"", "").replace("''", "");
    let ws_re = Regex::new(r" {2,}").unwrap();
    ws_re.replace_all(&s, " ").trim().to_string()
}

// ── Memory helpers ────────────────────────────────────────────────────

fn is_memory_enabled() -> bool {
    let config_path = get_repo_root().join(".flow").join("config.json");
    if !config_path.exists() {
        return false;
    }
    let Ok(content) = fs::read_to_string(&config_path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    config
        .get("memory")
        .and_then(|m| m.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
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
            .unwrap()
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
    if Regex::new(r"\bcodex\b").unwrap().is_match(&command) {
        let is_wrapper = Regex::new(r"flowctl\s+codex|FLOWCTL.*codex")
            .unwrap()
            .is_match(&command);
        if !is_wrapper {
            if Regex::new(r"\bcodex\s+exec\b")
                .unwrap()
                .is_match(&command)
            {
                output_block(
                    "BLOCKED: Do not call 'codex exec' directly. \
                     Use 'flowctl codex impl-review' or 'flowctl codex plan-review' \
                     to ensure proper receipt handling and session continuity.",
                );
            }
            if Regex::new(r"\bcodex\s+review\b")
                .unwrap()
                .is_match(&command)
            {
                output_block(
                    "BLOCKED: Do not call 'codex review' directly. \
                     Use 'flowctl codex impl-review' or 'flowctl codex plan-review'.",
                );
            }
        }
        if Regex::new(r"--last\b").unwrap().is_match(&command) {
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
        && !Regex::new(r"--help|-h").unwrap().is_match(&command)
    {
        if !Regex::new(r"--evidence-json|--evidence")
            .unwrap()
            .is_match(&command)
        {
            output_block(
                "BLOCKED: flowctl done requires --evidence-json flag. \
                 You must capture commit SHAs and test commands. \
                 Use: flowctl done <task> --summary-file <s.md> --evidence-json <e.json>",
            );
        }
        if !Regex::new(r"--summary-file|--summary")
            .unwrap()
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
                    .unwrap()
                    .is_match(&command)
                || Regex::new(r"(?i)cat\s*>\s*.*receipt")
                    .unwrap()
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
                    let id_re = Regex::new(r#""id"\s*:\s*"([^"]+)""#).unwrap();
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
            .map(|s| s.to_string())
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
            Regex::new(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>").unwrap();
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

        let done_re = Regex::new(r"\bdone\s+([a-zA-Z0-9][a-zA-Z0-9._-]*)").unwrap();
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
                    .unwrap()
                    .is_match(&command)
                || Regex::new(r"(?i)cat\s*>\s*.*receipt")
                    .unwrap()
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
        let w_re = Regex::new(r"W=(\d+)").unwrap();
        let t_re = Regex::new(r"(?i)T=([A-F0-9-]+)").unwrap();
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
        Regex::new(r"<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>").unwrap();
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
            Regex::new(r"(?i)\bLGTM\b|\bLooks good\b|\bApproved\b|\bNo issues\b").unwrap();
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
        .and_then(|v| v.as_bool())
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
    let plan_re = Regex::new(&format!(r"^plan-(fn-\d+{suffix_pattern})\.json$")).unwrap();
    if let Some(caps) = plan_re.captures(&basename) {
        return ("plan_review".into(), caps[1].to_string());
    }

    // Try impl pattern: impl-fn-N.M.json, impl-fn-N-xxx.M.json
    let impl_re =
        Regex::new(&format!(r"^impl-(fn-\d+{suffix_pattern}\.\d+)\.json$")).unwrap();
    if let Some(caps) = impl_re.captures(&basename) {
        return ("impl_review".into(), caps[1].to_string());
    }

    // Try completion pattern
    let completion_re =
        Regex::new(&format!(r"^completion-(fn-\d+{suffix_pattern})\.json$")).unwrap();
    if let Some(caps) = completion_re.captures(&basename) {
        return ("completion_review".into(), caps[1].to_string());
    }

    // Fallback
    ("impl_review".into(), "UNKNOWN".into())
}

// ═══════════════════════════════════════════════════════════════════════
// Commit Gate
// ═══════════════════════════════════════════════════════════════════════

fn cmd_commit_gate() {
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
                    .map(|s| s.to_string())
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
    let git_commit_re = Regex::new(r"\bgit\s+commit\b").unwrap();
    if !git_commit_re.is_match(command) {
        std::process::exit(0);
    }

    // Check: any task in_progress?
    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };
    let result = Command::new(&flowctl)
        .args(["tasks", "--json"])
        .output();
    let has_active = match result {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if let Ok(val) = serde_json::from_str::<Value>(&stdout) {
                val.get("tasks")
                    .and_then(|v| v.as_array())
                    .map(|tasks| {
                        tasks
                            .iter()
                            .any(|t| t.get("status").and_then(|s| s.as_str()) == Some("in_progress"))
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
    let canonical = fs::canonicalize(flow_dir)
        .unwrap_or_else(|_| flow_dir.to_path_buf());
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
    let result = Command::new("md5")
        .args(["-qs", &input_str])
        .output();
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
            out.split_whitespace().next().unwrap_or("default").to_string()
        }
        _ => "default".into(),
    }
}

/// Get current executable path for subprocess calls.
fn self_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| {
        if p.exists() {
            Some(p)
        } else {
            find_flowctl_for_guard()
        }
    })
}

// ═══════════════════════════════════════════════════════════════════════
// Pre-Compact
// ═══════════════════════════════════════════════════════════════════════

fn cmd_pre_compact() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };

    let mut lines: Vec<String> = Vec::new();

    // 1. Active epics and their progress
    if let Some(epics_val) = run_flowctl(&flowctl, &["epics", "--json"]) {
        if let Some(epics) = epics_val.get("epics").and_then(|v| v.as_array()) {
            for e in epics {
                let eid = match e.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                let status = e
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("open");
                if status == "done" {
                    continue;
                }

                if let Some(tasks_val) =
                    run_flowctl(&flowctl, &["tasks", "--epic", eid, "--json"])
                {
                    if let Some(tasks) = tasks_val.get("tasks").and_then(|v| v.as_array()) {
                        let mut counts: HashMap<String, usize> = HashMap::new();
                        for t in tasks {
                            let s = t
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("todo");
                            *counts.entry(s.to_string()).or_insert(0) += 1;
                        }
                        let mut progress_parts: Vec<String> =
                            counts.iter().map(|(s, c)| format!("{s}={c}")).collect();
                        progress_parts.sort();
                        lines.push(format!("Epic {eid}: {}", progress_parts.join(" ")));

                        // Show in-progress tasks
                        for t in tasks {
                            if t.get("status").and_then(|v| v.as_str()) == Some("in_progress") {
                                let tid = t
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                let title = t
                                    .get("title")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let files = t
                                    .get("files")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .take(3)
                                            .filter_map(|f| f.as_str())
                                            .collect::<Vec<_>>()
                                            .join(",")
                                    })
                                    .unwrap_or_default();
                                let files_str = if files.is_empty() {
                                    String::new()
                                } else {
                                    format!(" files=[{files}]")
                                };
                                lines.push(format!(
                                    "  IN_PROGRESS: {tid} \"{title}\"{files_str}"
                                ));
                            }
                        }
                    }
                }
            }

            // 2. Active file locks
            if let Some(locks_val) = run_flowctl(&flowctl, &["lock-check", "--json"]) {
                let count = locks_val
                    .get("count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if count > 0 {
                    lines.push(format!("File locks ({count} active):"));
                    if let Some(locks) = locks_val.get("locks").and_then(|v| v.as_object()) {
                        let mut sorted_keys: Vec<&String> = locks.keys().collect();
                        sorted_keys.sort();
                        for f in sorted_keys {
                            if let Some(info) = locks.get(f) {
                                let task_id = info
                                    .get("task_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                lines.push(format!("  {f} -> {task_id}"));
                            }
                        }
                    }
                }
            }

            // 3. Ready tasks
            for e in epics {
                if e.get("status").and_then(|v| v.as_str()) == Some("done") {
                    continue;
                }
                let eid = match e.get("id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };
                if let Some(ready_val) =
                    run_flowctl(&flowctl, &["ready", "--epic", eid, "--json"])
                {
                    if let Some(ready) = ready_val.get("ready").and_then(|v| v.as_array()) {
                        if !ready.is_empty() {
                            let ids: Vec<&str> = ready
                                .iter()
                                .take(5)
                                .filter_map(|t| t.get("id").and_then(|v| v.as_str()))
                                .collect();
                            lines.push(format!("Ready: {}", ids.join(", ")));
                        }
                    }
                }
            }
        }
    }

    if !lines.is_empty() {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "[flow-code state]").ok();
        for line in &lines {
            writeln!(buf, "{line}").ok();
        }
        writeln!(buf, "[/flow-code state]").ok();
        pretty_output("hook_precompact", &buf);
    }

    std::process::exit(0);
}

fn run_flowctl(flowctl: &Path, args: &[&str]) -> Option<Value> {
    let result = Command::new(flowctl)
        .args(args)
        .output();
    match result {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            serde_json::from_str(stdout.trim()).ok()
        }
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Subagent Context
// ═══════════════════════════════════════════════════════════════════════

fn cmd_subagent_context() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let flowctl = match self_exe() {
        Some(f) => f,
        None => std::process::exit(0),
    };

    if let Some(val) = run_flowctl(&flowctl, &["tasks", "--status", "in_progress", "--json"]) {
        let json_str = serde_json::to_string(&val).unwrap_or_default();
        if json_str != "[]" && !json_str.is_empty() {
            let line = format!("Active flow-code tasks: {json_str}");
            pretty_output("hook_subagent", &line);
        }
    }

    std::process::exit(0);
}

// ═══════════════════════════════════════════════════════════════════════
// Task Completed
// ═══════════════════════════════════════════════════════════════════════

fn cmd_task_completed() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        std::process::exit(0);
    }

    let data = read_stdin_json();
    let teammate_name = data
        .get("teammate_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let team_name = data
        .get("team_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_subject = data
        .get("task_subject")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Extract flow task ID from teammate_name (e.g., "worker-fn-1-add-auth.2" -> "fn-1-add-auth.2")
    let mut flow_task_id = if !teammate_name.is_empty() {
        teammate_name.strip_prefix("worker-").unwrap_or(teammate_name).to_string()
    } else {
        String::new()
    };

    // Fallback: try to extract from task_subject
    if flow_task_id.is_empty() || !flow_task_id.starts_with("fn-") {
        let task_id_re = Regex::new(r"fn-[a-z0-9-]+\.\d+").unwrap();
        if let Some(m) = task_id_re.find(task_subject) {
            flow_task_id = m.as_str().to_string();
        }
    }

    // Ensure hooks-log directory exists
    let log_dir = flow_dir.join("hooks-log");
    let _ = fs::create_dir_all(&log_dir);

    // Log the event
    let timestamp = chrono_utc_now();
    let event_json = json!({
        "event": "task_completed",
        "time": timestamp,
        "teammate": teammate_name,
        "team": team_name,
        "flow_task": flow_task_id,
        "subject": task_subject,
    });
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("events.jsonl"))
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", serde_json::to_string(&event_json).unwrap_or_default())
        });

    // If we identified a flow task, unlock its files
    if !flow_task_id.is_empty() && flow_task_id.starts_with("fn-") {
        let flowctl = match self_exe() {
            Some(f) => f,
            None => std::process::exit(0),
        };

        // Check if task exists and is in_progress or done
        if let Some(show_val) = run_flowctl(&flowctl, &["show", &flow_task_id, "--json"]) {
            let status = show_val
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status == "in_progress" || status == "done" {
                let _ = Command::new(&flowctl)
                    .args(["unlock", "--task", &flow_task_id, "--json"])
                    .output();

                let unlock_json = json!({
                    "event": "files_unlocked",
                    "time": timestamp,
                    "task": flow_task_id,
                });
                let _ = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_dir.join("events.jsonl"))
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "{}", serde_json::to_string(&unlock_json).unwrap_or_default())
                    });
            }
        }
    }

    std::process::exit(0);
}

/// Simple UTC timestamp without pulling in chrono crate.
fn chrono_utc_now() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "1970-01-01T00:00:00Z".into(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RTK Rewrite
// ═══════════════════════════════════════════════════════════════════════

fn cmd_rtk_rewrite() {
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

// ── Shared helpers ────────────────────────────────────────────────────

fn read_stdin_json() -> Value {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    serde_json::from_str(&input).unwrap_or(json!({}))
}

fn output_block(reason: &str) -> ! {
    eprintln!("{reason}");
    std::process::exit(2);
}

fn output_json_and_exit(data: &Value) -> ! {
    println!("{}", serde_json::to_string(data).unwrap_or_default());
    std::process::exit(0);
}
