//! Auto-memory hook: extract session memories from transcript (Stop hook).

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::Value;

use super::common::{get_flow_dir, read_stdin_json};

pub fn cmd_auto_memory() {
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
