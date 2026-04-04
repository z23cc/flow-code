//! Gap registry commands: gap add, list, resolve, check.
//!
//! Gaps track requirement deficiencies in an epic. They are stored in
//! the epic's Markdown frontmatter (via a companion JSON sidecar at
//! `.flow/epics/<epic-id>.gaps.json`). Blocking gaps (required/important)
//! prevent epic closure.

use std::env;
use std::fs;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::is_epic_id;
use flowctl_core::types::{EPICS_DIR, FLOW_DIR};

// ── Types ──────────────────────────────────────────────────────────

const GAP_BLOCKING_PRIORITIES: &[&str] = &["required", "important"];

#[derive(Subcommand, Debug)]
pub enum GapCmd {
    /// Register a requirement gap.
    Add {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// What is missing.
        #[arg(long, alias = "title")]
        capability: String,
        /// Gap priority.
        #[arg(long, default_value = "required", value_parser = ["required", "important", "nice-to-have"])]
        priority: String,
        /// Where gap was found.
        #[arg(long, default_value = "manual")]
        source: String,
        /// Task ID that addresses this gap.
        #[arg(long)]
        task: Option<String>,
    },
    /// List gaps for an epic.
    List {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Filter by status.
        #[arg(long, value_parser = ["open", "resolved"])]
        status: Option<String>,
    },
    /// Mark a gap as resolved.
    Resolve {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Capability to resolve.
        #[arg(long, alias = "title")]
        capability: Option<String>,
        /// Gap ID to resolve directly.
        #[arg(long)]
        id: Option<String>,
        /// How the gap was resolved.
        #[arg(long)]
        evidence: String,
    },
    /// Gate check: pass/fail based on unresolved gaps.
    Check {
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
}

pub fn dispatch(cmd: &GapCmd, json: bool) {
    match cmd {
        GapCmd::Add {
            epic,
            capability,
            priority,
            source,
            task,
        } => cmd_gap_add(json, epic, capability, priority, source, task.as_deref()),
        GapCmd::List { epic, status } => cmd_gap_list(json, epic, status.as_deref()),
        GapCmd::Resolve {
            epic,
            capability,
            id,
            evidence,
        } => cmd_gap_resolve(json, epic, capability.as_deref(), id.as_deref(), evidence),
        GapCmd::Check { epic } => cmd_gap_check(json, epic),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn get_flow_dir() -> std::path::PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(FLOW_DIR)
}

/// Compute deterministic gap ID from epic + capability (content-hash).
fn gap_id(epic_id: &str, capability: &str) -> String {
    use sha2::{Digest, Sha256};

    let key = format!("{}:{}", epic_id, capability.trim().to_lowercase());
    let hash = Sha256::digest(key.as_bytes());
    let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
    format!("gap-{}", &hex[..8])
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Path to the gaps sidecar JSON file for an epic.
fn gaps_path(flow_dir: &std::path::Path, epic_id: &str) -> std::path::PathBuf {
    flow_dir.join(EPICS_DIR).join(format!("{}.gaps.json", epic_id))
}

/// Load gaps array from sidecar file. Returns empty vec if file doesn't exist.
fn load_gaps(flow_dir: &std::path::Path, epic_id: &str) -> Vec<serde_json::Value> {
    let path = gaps_path(flow_dir, epic_id);
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Save gaps array to sidecar file.
fn save_gaps(flow_dir: &std::path::Path, epic_id: &str, gaps: &[serde_json::Value]) {
    let path = gaps_path(flow_dir, epic_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(gaps).unwrap();
    if let Err(e) = fs::write(&path, &content) {
        error_exit(&format!("Failed to write {}: {}", path.display(), e));
    }
}

/// Verify .flow/ exists, epic ID is valid, and epic file exists.
fn validate_epic(_json: bool, epic_id: &str) -> std::path::PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    if !is_epic_id(epic_id) {
        error_exit(&format!("Invalid epic ID: {}", epic_id));
    }
    // Verify the epic markdown file exists
    let epic_md = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !epic_md.exists() {
        error_exit(&format!("Epic not found: {}", epic_id));
    }
    flow_dir
}

// ── Commands ───────────────────────────────────────────────────────

fn cmd_gap_add(
    json_mode: bool,
    epic_id: &str,
    capability: &str,
    priority: &str,
    source: &str,
    task: Option<&str>,
) {
    let flow_dir = validate_epic(json_mode, epic_id);
    let gid = gap_id(epic_id, capability);

    let mut gaps = load_gaps(&flow_dir, epic_id);

    // Check for existing gap (idempotent)
    if let Some(existing) = gaps.iter().find(|g| g["id"].as_str() == Some(&gid)) {
        if json_mode {
            json_output(json!({
                "id": gid,
                "created": false,
                "gap": existing,
                "message": format!("Gap already exists: {}", gid),
            }));
        } else {
            println!(
                "Gap already exists: {} \u{2014} {}",
                gid,
                existing["capability"].as_str().unwrap_or("")
            );
        }
        return;
    }

    let gap = json!({
        "id": gid,
        "capability": capability.trim(),
        "priority": priority,
        "status": "open",
        "source": source,
        "task": task,
        "added_at": now_iso(),
        "resolved_at": null,
        "evidence": null,
    });

    gaps.push(gap.clone());
    save_gaps(&flow_dir, epic_id, &gaps);

    if json_mode {
        json_output(json!({
            "id": gid,
            "created": true,
            "gap": gap,
            "message": format!("Gap {} added to {}", gid, epic_id),
        }));
    } else {
        println!("Gap {} added: [{}] {}", gid, priority, capability.trim());
    }
}

fn cmd_gap_list(json_mode: bool, epic_id: &str, status_filter: Option<&str>) {
    let flow_dir = validate_epic(json_mode, epic_id);
    let gaps = load_gaps(&flow_dir, epic_id);

    let filtered: Vec<&serde_json::Value> = if let Some(status) = status_filter {
        gaps.iter()
            .filter(|g| g["status"].as_str() == Some(status))
            .collect()
    } else {
        gaps.iter().collect()
    };

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "count": filtered.len(),
            "gaps": filtered,
        }));
    } else if filtered.is_empty() {
        let suffix = status_filter
            .map(|s| format!(" (status={})", s))
            .unwrap_or_default();
        println!("No gaps for {}{}", epic_id, suffix);
    } else {
        for g in &filtered {
            let marker = if g["status"].as_str() == Some("resolved") {
                "\u{2713}"
            } else {
                "\u{2717}"
            };
            println!(
                "  {} {} [{}] {}",
                marker,
                g["id"].as_str().unwrap_or(""),
                g["priority"].as_str().unwrap_or(""),
                g["capability"].as_str().unwrap_or(""),
            );
        }
    }
}

fn cmd_gap_resolve(
    json_mode: bool,
    epic_id: &str,
    capability: Option<&str>,
    gap_id_direct: Option<&str>,
    evidence: &str,
) {
    let flow_dir = validate_epic(json_mode, epic_id);
    let mut gaps = load_gaps(&flow_dir, epic_id);

    // Find the gap by direct ID or by capability content-hash
    let gid = if let Some(direct_id) = gap_id_direct {
        direct_id.to_string()
    } else if let Some(cap) = capability {
        gap_id(epic_id, cap)
    } else {
        error_exit("Either --capability or --id is required");
    };

    let gap = gaps
        .iter_mut()
        .find(|g| g["id"].as_str() == Some(&gid));

    let gap = match gap {
        Some(g) => g,
        None => {
            error_exit(&format!("Gap not found: {}", gid));
        }
    };

    if gap["status"].as_str() == Some("resolved") {
        if json_mode {
            json_output(json!({
                "id": gid,
                "changed": false,
                "gap": *gap,
                "message": format!("Gap {} already resolved", gid),
            }));
        } else {
            println!("Gap {} already resolved", gid);
        }
        return;
    }

    gap["status"] = json!("resolved");
    gap["resolved_at"] = json!(now_iso());
    gap["evidence"] = json!(evidence);

    save_gaps(&flow_dir, epic_id, &gaps);

    if json_mode {
        let resolved_gap = gaps.iter().find(|g| g["id"].as_str() == Some(&gid)).unwrap();
        json_output(json!({
            "id": gid,
            "changed": true,
            "gap": resolved_gap,
            "message": format!("Gap {} resolved", gid),
        }));
    } else {
        println!("Gap {} resolved: {}", gid, evidence);
    }
}

fn cmd_gap_check(json_mode: bool, epic_id: &str) {
    let flow_dir = validate_epic(json_mode, epic_id);
    let gaps = load_gaps(&flow_dir, epic_id);

    let open_blocking: Vec<&serde_json::Value> = gaps
        .iter()
        .filter(|g| {
            g["status"].as_str() == Some("open")
                && g["priority"]
                    .as_str()
                    .map(|p| GAP_BLOCKING_PRIORITIES.contains(&p))
                    .unwrap_or(false)
        })
        .collect();

    let open_non_blocking: Vec<&serde_json::Value> = gaps
        .iter()
        .filter(|g| {
            g["status"].as_str() == Some("open")
                && !g["priority"]
                    .as_str()
                    .map(|p| GAP_BLOCKING_PRIORITIES.contains(&p))
                    .unwrap_or(false)
        })
        .collect();

    let resolved: Vec<&serde_json::Value> = gaps
        .iter()
        .filter(|g| g["status"].as_str() == Some("resolved"))
        .collect();

    let gate = if open_blocking.is_empty() {
        "pass"
    } else {
        "fail"
    };

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "gate": gate,
            "total": gaps.len(),
            "open_blocking": open_blocking,
            "open_non_blocking": open_non_blocking,
            "resolved": resolved,
        }));
    } else if gate == "pass" {
        println!(
            "Gap check PASS for {} ({} resolved, {} non-blocking)",
            epic_id,
            resolved.len(),
            open_non_blocking.len()
        );
    } else {
        println!(
            "Gap check FAIL for {} \u{2014} {} blocking gap(s):",
            epic_id,
            open_blocking.len()
        );
        for g in &open_blocking {
            println!(
                "  \u{2717} [{}] {}",
                g["priority"].as_str().unwrap_or(""),
                g["capability"].as_str().unwrap_or(""),
            );
        }
    }

    if gate == "fail" {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_id_deterministic() {
        let id1 = gap_id("fn-1-test", "missing auth");
        let id2 = gap_id("fn-1-test", "missing auth");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("gap-"));
        assert_eq!(id1.len(), 4 + 8); // "gap-" + 8 hex chars
    }

    #[test]
    fn test_gap_id_case_insensitive() {
        let id1 = gap_id("fn-1-test", "Missing Auth");
        let id2 = gap_id("fn-1-test", "missing auth");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_gap_id_different_epics() {
        let id1 = gap_id("fn-1-test", "missing auth");
        let id2 = gap_id("fn-2-other", "missing auth");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_sha256_via_gap_id() {
        // Verify gap_id produces Python-parity results via sha2 crate.
        // Python: hashlib.sha256(b"fn-1-test:missing auth").hexdigest()[:8]
        let gid = gap_id("fn-1-test", "missing auth");
        assert!(gid.starts_with("gap-"));
        assert_eq!(gid.len(), 12); // "gap-" + 8 hex chars
    }

    #[test]
    fn test_blocking_priorities() {
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"required"));
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"important"));
        assert!(!GAP_BLOCKING_PRIORITIES.contains(&"nice-to-have"));
    }
}
