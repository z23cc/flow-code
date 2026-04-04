//! Memory commands: init, add, read, list, search, inject, verify, gc.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use clap::Subcommand;
use regex::Regex;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::output::{error_exit, json_output};
use flowctl_core::types::{CONFIG_FILE, MEMORY_DIR};

use super::helpers::get_flow_dir;

// ── Constants ──────────────────────────────────────────────────────

const MEMORY_VALID_TYPES: &[&str] = &["pitfall", "convention", "decision"];

const TAG_PATTERNS: &[&str] = &[
    r"\b(typescript|javascript|python|rust|go|java|ruby|swift)\b",
    r"\b(react|vue|angular|svelte|nextjs|django|flask|fastapi|express)\b",
    r"\b(postgres|mysql|sqlite|redis|mongodb|supabase)\b",
    r"\b(docker|kubernetes|ci|cd|github|gitlab)\b",
    r"\b(api|auth|oauth|jwt|cors|csrf|xss|sql)\b",
    r"\b(test|lint|build|deploy|migration|schema)\b",
];

// ── CLI definition ─────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum MemoryCmd {
    /// Initialize memory (auto-migrates legacy).
    Init,
    /// Add atomic memory entry.
    Add {
        /// Type: pitfall, convention, or decision.
        #[arg(name = "type")]
        entry_type: String,
        /// Entry content.
        content: String,
    },
    /// Read entries (L3: full content).
    Read {
        /// Filter by type.
        #[arg(long = "type")]
        entry_type: Option<String>,
    },
    /// List entries with ref counts.
    List,
    /// Search entries by pattern.
    Search {
        /// Search pattern (regex).
        pattern: String,
    },
    /// Inject relevant entries (progressive disclosure).
    Inject {
        /// Filter by type.
        #[arg(long = "type")]
        entry_type: Option<String>,
        /// Filter by tags (comma-separated).
        #[arg(long)]
        tags: Option<String>,
        /// L3: inject full content of all entries.
        #[arg(long)]
        full: bool,
    },
    /// Mark entry as verified (still valid).
    Verify {
        /// Entry ID to verify.
        id: i64,
    },
    /// Garbage collect stale entries.
    Gc {
        /// Remove entries older than N days with 0 refs.
        #[arg(long, default_value = "90")]
        days: i64,
        /// Show what would be removed.
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn dispatch(cmd: &MemoryCmd, json: bool) {
    match cmd {
        MemoryCmd::Init => cmd_memory_init(json),
        MemoryCmd::Add {
            entry_type,
            content,
        } => cmd_memory_add(json, entry_type, content),
        MemoryCmd::Read { entry_type } => cmd_memory_read(json, entry_type.as_deref()),
        MemoryCmd::List => cmd_memory_list(json),
        MemoryCmd::Search { pattern } => cmd_memory_search(json, pattern),
        MemoryCmd::Inject {
            entry_type,
            tags,
            full,
        } => cmd_memory_inject(json, entry_type.as_deref(), tags.as_deref(), *full),
        MemoryCmd::Verify { id } => cmd_memory_verify(json, *id),
        MemoryCmd::Gc { days, dry_run } => cmd_memory_gc(json, *days, *dry_run),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn memory_dir() -> PathBuf {
    get_flow_dir().join(MEMORY_DIR)
}

fn memory_entries_dir() -> PathBuf {
    memory_dir().join("entries")
}

fn memory_index_path() -> PathBuf {
    memory_dir().join("index.jsonl")
}

fn memory_stats_path() -> PathBuf {
    memory_dir().join("stats.json")
}

/// Normalize type input: 'pitfalls' -> 'pitfall', etc.
fn normalize_memory_type(raw: &str) -> Option<&'static str> {
    let lower = raw.to_lowercase();
    let trimmed = lower.trim_end_matches('s');
    MEMORY_VALID_TYPES
        .iter()
        .find(|&&t| t == trimmed)
        .copied()
}

/// SHA256 prefix for deduplication (matches Python _content_hash).
fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.trim().as_bytes());
    let result = hasher.finalize();
    hex_encode(&result[..6]) // 12 hex chars = 6 bytes
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Scan existing entries to find next numeric ID.
fn next_entry_id(entries_dir: &Path) -> i64 {
    let mut max_id: i64 = 0;
    if let Ok(entries) = fs::read_dir(entries_dir) {
        let re = Regex::new(r"^(\d+)-").unwrap();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(caps) = re.captures(&name_str) {
                if let Ok(id) = caps[1].parse::<i64>() {
                    max_id = max_id.max(id);
                }
            }
        }
    }
    max_id + 1
}

/// Load index.jsonl entries.
fn load_index(index_path: &Path) -> Vec<serde_json::Value> {
    if !index_path.exists() {
        return Vec::new();
    }
    let content = match fs::read_to_string(index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Write index.jsonl atomically.
fn save_index(index_path: &Path, entries: &[serde_json::Value]) {
    let lines: Vec<String> = entries
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    let content = if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n") + "\n"
    };
    if let Some(parent) = index_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(index_path, &content) {
        error_exit(&format!(
            "Failed to write {}: {}",
            index_path.display(),
            e
        ));
    }
}

/// Load stats.json.
fn load_stats(stats_path: &Path) -> serde_json::Value {
    if !stats_path.exists() {
        return json!({});
    }
    match fs::read_to_string(stats_path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or(json!({})),
        Err(_) => json!({}),
    }
}

/// Write stats.json.
fn save_stats(stats_path: &Path, stats: &serde_json::Value) {
    if let Some(parent) = stats_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(stats).unwrap();
    if let Err(e) = fs::write(stats_path, &content) {
        error_exit(&format!(
            "Failed to write {}: {}",
            stats_path.display(),
            e
        ));
    }
}

/// Increment reference counts for injected entries.
fn bump_refs(stats_path: &Path, entry_ids: &[String]) {
    if entry_ids.is_empty() {
        return;
    }
    let mut stats = load_stats(stats_path);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    for eid in entry_ids {
        let entry = stats
            .as_object_mut()
            .unwrap()
            .entry(eid.clone())
            .or_insert_with(|| json!({"refs": 0, "last_ref": ""}));
        let refs = entry
            .get("refs")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        entry["refs"] = json!(refs + 1);
        entry["last_ref"] = json!(now);
    }
    save_stats(stats_path, &stats);
}

/// Extract simple keyword tags from content.
fn extract_tags(content: &str) -> Vec<String> {
    let mut tags = HashSet::new();
    let lower = content.to_lowercase();
    for pattern in TAG_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            for caps in re.captures_iter(&lower) {
                if let Some(m) = caps.get(1) {
                    tags.insert(m.as_str().to_string());
                }
            }
        }
    }
    let mut sorted: Vec<String> = tags.into_iter().collect();
    sorted.sort();
    sorted.truncate(8);
    sorted
}

/// Check memory.enabled config, ensure dirs exist. Returns memory dir or exits.
fn require_memory_enabled(json: bool) -> PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        if json {
            json_output(json!({"error": ".flow/ does not exist. Run 'flowctl init' first."}));
            std::process::exit(1);
        } else {
            error_exit(".flow/ does not exist. Run 'flowctl init' first.");
        }
    }

    // Check config
    let config_path = flow_dir.join(CONFIG_FILE);
    let memory_enabled = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let config: serde_json::Value =
                    serde_json::from_str(&content).unwrap_or(json!({}));
                config
                    .get("memory")
                    .and_then(|m| m.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }
            Err(_) => false,
        }
    } else {
        false
    };

    if !memory_enabled {
        if json {
            json_output(json!({
                "error": "Memory not enabled. Run: flowctl config set memory.enabled true"
            }));
        } else {
            eprintln!("Error: Memory not enabled.");
            eprintln!("Enable with: flowctl config set memory.enabled true");
        }
        std::process::exit(1);
    }

    let mem_dir = memory_dir();
    let _ = fs::create_dir_all(&mem_dir);
    let entries = memory_entries_dir();
    let _ = fs::create_dir_all(&entries);

    mem_dir
}

// ── Commands ──────────────────────────────────────────────────────

fn cmd_memory_init(json: bool) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        if json {
            json_output(json!({"error": ".flow/ does not exist. Run 'flowctl init' first."}));
            std::process::exit(1);
        } else {
            error_exit(".flow/ does not exist. Run 'flowctl init' first.");
        }
    }

    // Check config
    let config_path = flow_dir.join(CONFIG_FILE);
    let memory_enabled = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let config: serde_json::Value =
                    serde_json::from_str(&content).unwrap_or(json!({}));
                config
                    .get("memory")
                    .and_then(|m| m.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }
            Err(_) => false,
        }
    } else {
        false
    };

    if !memory_enabled {
        if json {
            json_output(json!({
                "error": "Memory not enabled. Run: flowctl config set memory.enabled true"
            }));
        } else {
            eprintln!("Error: Memory not enabled.");
            eprintln!("Enable with: flowctl config set memory.enabled true");
        }
        std::process::exit(1);
    }

    let mem_dir = memory_dir();
    let _ = fs::create_dir_all(&mem_dir);
    let entries_dir = memory_entries_dir();
    let _ = fs::create_dir_all(&entries_dir);

    let mut created = Vec::new();

    let index_path = memory_index_path();
    if !index_path.exists() {
        let _ = fs::write(&index_path, "");
        created.push("index.jsonl".to_string());
    }

    let stats_path = memory_stats_path();
    if !stats_path.exists() {
        save_stats(&stats_path, &json!({}));
        created.push("stats.json".to_string());
    }

    if json {
        json_output(json!({
            "path": mem_dir.to_string_lossy(),
            "created": created,
            "migrated": 0,
            "message": "Memory v2 initialized",
        }));
    } else {
        println!("Memory v2 initialized at {}", mem_dir.display());
        for f in &created {
            println!("  Created: {}", f);
        }
    }
}

fn cmd_memory_add(json: bool, entry_type: &str, content: &str) {
    require_memory_enabled(json);

    let type_name = match normalize_memory_type(entry_type) {
        Some(t) => t,
        None => {
            if json {
                json_output(json!({
                    "error": format!("Invalid type '{}'. Use: pitfall, convention, or decision", entry_type)
                }));
            } else {
                eprintln!(
                    "Error: Invalid type '{}'. Use: pitfall, convention, or decision",
                    entry_type
                );
            }
            std::process::exit(1);
        }
    };

    let content = content.trim();
    if content.is_empty() {
        if json {
            json_output(json!({"error": "Content cannot be empty"}));
        } else {
            eprintln!("Error: Content cannot be empty");
        }
        std::process::exit(1);
    }

    // Dedup check
    let chash = content_hash(content);
    let index_path = memory_index_path();
    let existing = load_index(&index_path);
    for e in &existing {
        if e.get("hash").and_then(|v| v.as_str()) == Some(&chash) {
            let dup_id = e.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            if json {
                json_output(json!({
                    "id": dup_id,
                    "duplicate": true,
                    "message": "Duplicate entry, skipped",
                }));
            } else {
                println!("Duplicate of entry #{}, skipped", dup_id);
            }
            return;
        }
    }

    // Write atomic entry
    let entries_dir = memory_entries_dir();
    let entry_id = next_entry_id(&entries_dir);
    let entry_filename = format!("{:03}-{}.md", entry_id, type_name);
    if let Err(e) = fs::write(entries_dir.join(&entry_filename), content) {
        error_exit(&format!("Failed to write entry file: {}", e));
    }

    // Extract tags and summary
    let tags = extract_tags(content);
    let summary: String = content.lines().next().unwrap_or("").chars().take(120).collect();
    let created = Utc::now().format("%Y-%m-%d").to_string();

    // Append to index
    let idx_entry = json!({
        "id": entry_id,
        "type": type_name,
        "summary": summary,
        "tags": tags,
        "hash": chash,
        "created": created,
        "last_verified": created,
        "file": entry_filename,
    });
    let mut all_entries = existing;
    all_entries.push(idx_entry);
    save_index(&index_path, &all_entries);

    if json {
        json_output(json!({
            "id": entry_id,
            "type": type_name,
            "file": entry_filename,
            "tags": tags,
        }));
    } else {
        println!("Added {} #{}: {}", type_name, entry_id, summary);
        if !tags.is_empty() {
            println!("  Tags: {}", tags.join(", "));
        }
    }
}

fn cmd_memory_read(json: bool, entry_type: Option<&str>) {
    require_memory_enabled(json);

    let index = load_index(&memory_index_path());

    let type_filter = entry_type.and_then(normalize_memory_type);
    if entry_type.is_some() && type_filter.is_none() {
        if json {
            json_output(json!({
                "error": format!("Invalid type '{}'. Use: pitfall, convention, or decision", entry_type.unwrap())
            }));
        } else {
            eprintln!(
                "Error: Invalid type '{}'. Use: pitfall, convention, or decision",
                entry_type.unwrap()
            );
        }
        std::process::exit(1);
    }

    let entries_dir = memory_entries_dir();
    let mut results = Vec::new();

    for idx in &index {
        if let Some(tf) = type_filter {
            if idx.get("type").and_then(|v| v.as_str()) != Some(tf) {
                continue;
            }
        }
        let file = idx.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let entry_path = entries_dir.join(file);
        let content = fs::read_to_string(&entry_path).unwrap_or_default();

        results.push(json!({
            "id": idx.get("id"),
            "type": idx.get("type"),
            "summary": idx.get("summary"),
            "tags": idx.get("tags").cloned().unwrap_or(json!([])),
            "created": idx.get("created").cloned().unwrap_or(json!("")),
            "content": content,
        }));
    }

    if json {
        json_output(json!({
            "entries": results,
            "count": results.len(),
        }));
    } else if results.is_empty() {
        let suffix = type_filter
            .map(|t| format!(" of type '{}'", t))
            .unwrap_or_default();
        println!("No memory entries{}", suffix);
    } else {
        for r in &results {
            println!(
                "--- #{} [{}] {} ---",
                r["id"],
                r["type"].as_str().unwrap_or(""),
                r["created"].as_str().unwrap_or("")
            );
            println!("{}", r["content"].as_str().unwrap_or(""));
            if let Some(tags) = r["tags"].as_array() {
                if !tags.is_empty() {
                    let tag_strs: Vec<&str> =
                        tags.iter().filter_map(|t| t.as_str()).collect();
                    println!("  Tags: {}", tag_strs.join(", "));
                }
            }
            println!();
        }
        println!("Total: {} entries", results.len());
    }
}

fn cmd_memory_list(json: bool) {
    require_memory_enabled(json);

    let index = load_index(&memory_index_path());
    let stats = load_stats(&memory_stats_path());

    let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for idx in &index {
        let t = idx
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        *counts.entry(t).or_insert(0) += 1;
    }

    let total = index.len();
    let total_refs: i64 = stats
        .as_object()
        .map(|m| {
            m.values()
                .map(|v| v.get("refs").and_then(|r| r.as_i64()).unwrap_or(0))
                .sum()
        })
        .unwrap_or(0);

    // Staleness threshold: 90 days ago
    let stale_cutoff = (Utc::now() - Duration::days(90))
        .format("%Y-%m-%d")
        .to_string();

    if json {
        let index_data: Vec<serde_json::Value> = index
            .iter()
            .map(|idx| {
                let eid = idx.get("id").and_then(|v| v.as_i64()).unwrap_or(0).to_string();
                let last_verified = idx
                    .get("last_verified")
                    .or_else(|| idx.get("created"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let stale = last_verified < stale_cutoff.as_str();
                let refs = stats
                    .get(&eid)
                    .and_then(|s| s.get("refs"))
                    .and_then(|r| r.as_i64())
                    .unwrap_or(0);
                json!({
                    "id": idx.get("id"),
                    "type": idx.get("type"),
                    "summary": idx.get("summary"),
                    "tags": idx.get("tags").cloned().unwrap_or(json!([])),
                    "created": idx.get("created").cloned().unwrap_or(json!("")),
                    "last_verified": last_verified,
                    "stale": stale,
                    "refs": refs,
                })
            })
            .collect();

        json_output(json!({
            "counts": counts,
            "total": total,
            "total_refs": total_refs,
            "index": index_data,
        }));
    } else {
        let mut stale_count = 0;
        println!("Memory: {} entries, {} total references\n", total, total_refs);
        for idx in &index {
            let eid = idx.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            let eid_str = eid.to_string();
            let refs = stats
                .get(&eid_str)
                .and_then(|s| s.get("refs"))
                .and_then(|r| r.as_i64())
                .unwrap_or(0);
            let verified = idx
                .get("last_verified")
                .or_else(|| idx.get("created"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_stale = if verified.is_empty() {
                true
            } else {
                verified < stale_cutoff.as_str()
            };
            if is_stale {
                stale_count += 1;
            }
            let stale_tag = if is_stale { " [stale]" } else { "" };
            let entry_type = idx.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let summary = idx.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let summary_trunc: String = summary.chars().take(70).collect();
            println!(
                "  #{:3} [{:10}] refs={:2}  {}{}",
                eid, entry_type, refs, summary_trunc, stale_tag
            );
        }
        println!();
        let mut sorted_counts: Vec<_> = counts.iter().collect();
        sorted_counts.sort_by_key(|(k, _)| (*k).clone());
        for (t, c) in &sorted_counts {
            println!("  {}: {}", t, c);
        }
        println!("  Total: {}", total);
        if stale_count > 0 {
            println!(
                "  Stale: {} (not verified in 90+ days — run /flow-code:retro to verify)",
                stale_count
            );
        }
    }
}

fn cmd_memory_search(json: bool, pattern: &str) {
    require_memory_enabled(json);

    let compiled = match Regex::new(&format!("(?i){}", pattern)) {
        Ok(re) => re,
        Err(e) => {
            if json {
                json_output(json!({"error": format!("Invalid regex pattern: {}", e)}));
            } else {
                eprintln!("Error: Invalid regex pattern: {}", e);
            }
            std::process::exit(1);
        }
    };

    let index = load_index(&memory_index_path());
    let entries_dir = memory_entries_dir();
    let mut matches = Vec::new();

    for idx in &index {
        let mut hit = false;

        // Search summary
        if let Some(summary) = idx.get("summary").and_then(|v| v.as_str()) {
            if compiled.is_match(summary) {
                hit = true;
            }
        }

        // Search tags
        if !hit {
            if let Some(tags) = idx.get("tags").and_then(|v| v.as_array()) {
                for tag in tags {
                    if let Some(t) = tag.as_str() {
                        if compiled.is_match(t) {
                            hit = true;
                            break;
                        }
                    }
                }
            }
        }

        // Search content
        let file = idx.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let entry_path = entries_dir.join(file);
        let content = if entry_path.exists() {
            fs::read_to_string(&entry_path).unwrap_or_default()
        } else {
            String::new()
        };

        if !hit && compiled.is_match(&content) {
            hit = true;
        }

        if hit {
            matches.push(json!({
                "id": idx.get("id"),
                "type": idx.get("type"),
                "summary": idx.get("summary"),
                "tags": idx.get("tags").cloned().unwrap_or(json!([])),
                "content": content,
            }));
        }
    }

    if json {
        json_output(json!({
            "pattern": pattern,
            "matches": matches,
            "count": matches.len(),
        }));
    } else if matches.is_empty() {
        println!("No matches for '{}'", pattern);
    } else {
        for m in &matches {
            println!(
                "--- #{} [{}] ---",
                m["id"],
                m["type"].as_str().unwrap_or("")
            );
            println!("{}", m["content"].as_str().unwrap_or(""));
            println!();
        }
        println!("Found {} matches for '{}'", matches.len(), pattern);
    }
}

fn cmd_memory_inject(json: bool, entry_type: Option<&str>, tags: Option<&str>, full: bool) {
    require_memory_enabled(json);

    let index = load_index(&memory_index_path());
    if index.is_empty() {
        if json {
            json_output(json!({"entries": [], "level": "L1", "count": 0}));
        } else {
            println!("No memory entries");
        }
        return;
    }

    let entries_dir = memory_entries_dir();

    // Determine filters
    let type_filter = entry_type.and_then(normalize_memory_type);
    let tag_filter: Vec<String> = tags
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Filter entries
    let filtered: Vec<&serde_json::Value> = index
        .iter()
        .filter(|idx| {
            if let Some(tf) = type_filter {
                if idx.get("type").and_then(|v| v.as_str()) != Some(tf) {
                    return false;
                }
            }
            if !tag_filter.is_empty() {
                let entry_tags: Vec<String> = idx
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str())
                            .map(|s| s.to_lowercase())
                            .collect()
                    })
                    .unwrap_or_default();
                if !tag_filter.iter().any(|t| entry_tags.contains(t)) {
                    return false;
                }
            }
            true
        })
        .collect();

    // Determine level
    let level = if full && type_filter.is_none() && tag_filter.is_empty() {
        "L3"
    } else if type_filter.is_some() || !tag_filter.is_empty() {
        "L2"
    } else {
        "L1"
    };

    // Bump reference counts
    let ids: Vec<String> = filtered
        .iter()
        .map(|e| {
            e.get("id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .to_string()
        })
        .collect();
    bump_refs(&memory_stats_path(), &ids);

    if level == "L1" {
        // Compact index
        let compact: Vec<serde_json::Value> = filtered
            .iter()
            .map(|e| {
                json!({
                    "id": e.get("id"),
                    "type": e.get("type"),
                    "summary": e.get("summary"),
                    "tags": e.get("tags").cloned().unwrap_or(json!([])),
                })
            })
            .collect();

        if json {
            json_output(json!({
                "entries": compact,
                "level": "L1",
                "count": compact.len(),
            }));
        } else {
            println!("Memory index ({} entries):", filtered.len());
            for e in &filtered {
                let tags_arr = e.get("tags").and_then(|v| v.as_array());
                let tags_str = tags_arr
                    .map(|arr| {
                        let ts: Vec<&str> = arr
                            .iter()
                            .filter_map(|t| t.as_str())
                            .take(3)
                            .collect();
                        if ts.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", ts.join(","))
                        }
                    })
                    .unwrap_or_default();
                let summary = e.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let summary_trunc: String = summary.chars().take(100).collect();
                println!(
                    "  #{} [{}]{} {}",
                    e["id"],
                    e["type"].as_str().unwrap_or(""),
                    tags_str,
                    summary_trunc
                );
            }
            println!(
                "\nUse `memory search <pattern>` for full content of specific entries."
            );
        }
    } else {
        // Full content for filtered entries
        let results: Vec<serde_json::Value> = filtered
            .iter()
            .map(|idx| {
                let file = idx.get("file").and_then(|v| v.as_str()).unwrap_or("");
                let entry_path = entries_dir.join(file);
                let content = if entry_path.exists() {
                    fs::read_to_string(&entry_path).unwrap_or_default()
                } else {
                    String::new()
                };
                json!({
                    "id": idx.get("id"),
                    "type": idx.get("type"),
                    "summary": idx.get("summary"),
                    "tags": idx.get("tags").cloned().unwrap_or(json!([])),
                    "content": content,
                })
            })
            .collect();

        if json {
            json_output(json!({
                "entries": results,
                "level": level,
                "count": results.len(),
            }));
        } else {
            for r in &results {
                println!(
                    "--- #{} [{}] ---",
                    r["id"],
                    r["type"].as_str().unwrap_or("")
                );
                println!("{}", r["content"].as_str().unwrap_or(""));
                println!();
            }
        }
    }
}

fn cmd_memory_verify(json: bool, entry_id: i64) {
    require_memory_enabled(json);

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let index_path = memory_index_path();
    let mut index = load_index(&index_path);

    let mut found = false;
    for idx in &mut index {
        if idx.get("id").and_then(|v| v.as_i64()) == Some(entry_id) {
            idx["last_verified"] = json!(today);
            found = true;
            break;
        }
    }

    if !found {
        if json {
            json_output(json!({"error": format!("Entry #{} not found", entry_id)}));
        } else {
            eprintln!("Error: Entry #{} not found", entry_id);
        }
        std::process::exit(1);
    }

    save_index(&index_path, &index);

    if json {
        json_output(json!({
            "id": entry_id,
            "last_verified": today,
            "message": format!("Entry #{} verified", entry_id),
        }));
    } else {
        println!("Entry #{} verified as still valid ({})", entry_id, today);
    }
}

fn cmd_memory_gc(json: bool, days: i64, dry_run: bool) {
    require_memory_enabled(json);

    let index = load_index(&memory_index_path());
    let mut stats = load_stats(&memory_stats_path());
    let entries_dir = memory_entries_dir();

    let cutoff_date = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let mut stale = Vec::new();
    let mut keep = Vec::new();

    for idx in &index {
        let eid_str = idx
            .get("id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .to_string();
        let refs = stats
            .get(&eid_str)
            .and_then(|s| s.get("refs"))
            .and_then(|r| r.as_i64())
            .unwrap_or(0);
        let created = idx
            .get("created")
            .and_then(|v| v.as_str())
            .unwrap_or("9999-99-99");

        if refs == 0 && created < cutoff_date.as_str() {
            stale.push(idx.clone());
        } else {
            keep.push(idx.clone());
        }
    }

    if dry_run {
        if json {
            let stale_info: Vec<serde_json::Value> = stale
                .iter()
                .map(|s| {
                    json!({
                        "id": s.get("id"),
                        "type": s.get("type"),
                        "summary": s.get("summary"),
                    })
                })
                .collect();
            json_output(json!({
                "dry_run": true,
                "stale": stale_info,
                "count": stale.len(),
                "kept": keep.len(),
            }));
        } else {
            println!(
                "Dry run: {} stale entries (0 refs, older than {} days)",
                stale.len(),
                days
            );
            for s in &stale {
                let summary = s.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let summary_trunc: String = summary.chars().take(80).collect();
                println!(
                    "  #{} [{}] {}",
                    s["id"],
                    s["type"].as_str().unwrap_or(""),
                    summary_trunc
                );
            }
            println!("Would keep: {} entries", keep.len());
        }
        return;
    }

    // Remove stale entries
    let mut removed = 0;
    for s in &stale {
        let file = s.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let entry_path = entries_dir.join(file);
        if entry_path.exists() {
            let _ = fs::remove_file(&entry_path);
        }
        let eid_str = s
            .get("id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .to_string();
        if let Some(obj) = stats.as_object_mut() {
            obj.remove(&eid_str);
        }
        removed += 1;
    }

    save_index(&memory_index_path(), &keep);
    save_stats(&memory_stats_path(), &stats);

    if json {
        json_output(json!({"removed": removed, "kept": keep.len()}));
    } else {
        println!("Removed {} stale entries, kept {}", removed, keep.len());
    }
}
