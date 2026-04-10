//! Gap registry commands: gap add, list, resolve, check.
//!
//! Gaps track requirement deficiencies in an epic. They are stored in
//! JSON files under `gaps/<epic-id>.json`. Blocking gaps
//! (required/important) prevent epic closure.

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::id::is_epic_id;
use flowctl_core::json_store::{self, GapEntry};

use super::helpers::get_flow_dir;

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
            task: _,
        } => cmd_gap_add(json, epic, capability, priority, source),
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

/// Verify .flow/ exists, epic ID is valid, and epic exists.
fn validate_epic(_json: bool, epic_id: &str) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    if !is_epic_id(epic_id) {
        error_exit(&format!("Invalid epic ID: {}", epic_id));
    }
    // Check JSON file existence
    if flowctl_core::json_store::epic_read(&flow_dir, epic_id).is_ok() {
        return;
    }
    let json_path = flow_dir.join("epics").join(format!("{epic_id}.json"));
    if json_path.exists() {
        return;
    }
    error_exit(&format!("Epic not found: {}", epic_id));
}

fn gap_flow_dir() -> std::path::PathBuf {
    get_flow_dir()
}

// ── Commands ───────────────────────────────────────────────────────

fn cmd_gap_add(json_mode: bool, epic_id: &str, capability: &str, priority: &str, source: &str) {
    validate_epic(json_mode, epic_id);
    let flow_dir = gap_flow_dir();

    let mut gaps = json_store::gaps_read(&flow_dir, epic_id).unwrap_or_default();

    // Check for existing gap with same capability (idempotent)
    let cap_lower = capability.trim().to_lowercase();
    if let Some(existing) = gaps
        .iter()
        .find(|g| g.capability.trim().to_lowercase() == cap_lower)
    {
        let status = if existing.resolved {
            "resolved"
        } else {
            "open"
        };
        if json_mode {
            json_output(json!({
                "id": existing.id,
                "created": false,
                "gap": {
                    "id": existing.id,
                    "capability": existing.capability,
                    "priority": existing.priority,
                    "status": status,
                    "source": existing.source,
                },
                "message": format!("Gap already exists: {}", existing.id),
            }));
        } else {
            println!(
                "Gap already exists: {} \u{2014} {}",
                existing.id, existing.capability
            );
        }
        return;
    }

    let next_id = gaps.iter().map(|g| g.id).max().unwrap_or(0) + 1;
    gaps.push(GapEntry {
        id: next_id,
        capability: capability.trim().to_string(),
        priority: priority.to_string(),
        source: source.to_string(),
        resolved: false,
    });

    if let Err(e) = json_store::gaps_write(&flow_dir, epic_id, &gaps) {
        error_exit(&format!("Failed to add gap: {e}"));
    }

    if json_mode {
        json_output(json!({
            "id": next_id,
            "created": true,
            "gap": {
                "id": next_id,
                "capability": capability.trim(),
                "priority": priority,
                "status": "open",
                "source": source,
            },
            "message": format!("Gap {} added to {}", next_id, epic_id),
        }));
    } else {
        println!(
            "Gap {} added: [{}] {}",
            next_id,
            priority,
            capability.trim()
        );
    }
}

fn cmd_gap_list(json_mode: bool, epic_id: &str, status_filter: Option<&str>) {
    validate_epic(json_mode, epic_id);
    let flow_dir = gap_flow_dir();
    let gaps = json_store::gaps_read(&flow_dir, epic_id).unwrap_or_default();

    let filtered: Vec<&GapEntry> = gaps
        .iter()
        .filter(|g| match status_filter {
            Some("open") => !g.resolved,
            Some("resolved") => g.resolved,
            _ => true,
        })
        .collect();

    if json_mode {
        let gap_values: Vec<serde_json::Value> = filtered
            .iter()
            .map(|g| {
                json!({
                    "id": g.id,
                    "capability": g.capability,
                    "priority": g.priority,
                    "status": if g.resolved { "resolved" } else { "open" },
                    "source": g.source,
                })
            })
            .collect();
        json_output(json!({
            "epic": epic_id,
            "count": filtered.len(),
            "gaps": gap_values,
        }));
    } else if filtered.is_empty() {
        let suffix = status_filter
            .map(|s| format!(" (status={})", s))
            .unwrap_or_default();
        let msg = format!("No gaps for {}{}", epic_id, suffix);
        pretty_output("gap", &msg);
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        for g in &filtered {
            let marker = if g.resolved { "\u{2713}" } else { "\u{2717}" };
            writeln!(
                buf,
                "  {} {} [{}] {}",
                marker, g.id, g.priority, g.capability
            )
            .ok();
        }
        pretty_output("gap", &buf);
    }
}

fn cmd_gap_resolve(
    json_mode: bool,
    epic_id: &str,
    capability: Option<&str>,
    gap_id_direct: Option<&str>,
    _evidence: &str,
) {
    validate_epic(json_mode, epic_id);
    let flow_dir = gap_flow_dir();
    let mut gaps = json_store::gaps_read(&flow_dir, epic_id).unwrap_or_default();

    if let Some(direct_id) = gap_id_direct {
        let gap_id: u32 = direct_id
            .parse()
            .unwrap_or_else(|_| error_exit(&format!("Invalid gap ID: {}", direct_id)));

        if let Some(g) = gaps.iter_mut().find(|g| g.id == gap_id) {
            g.resolved = true;
        } else {
            error_exit(&format!("Gap {} not found", gap_id));
        }

        json_store::gaps_write(&flow_dir, epic_id, &gaps).unwrap_or_else(|e| {
            error_exit(&format!("Failed to resolve gap: {e}"));
        });

        if json_mode {
            json_output(json!({
                "id": gap_id,
                "changed": true,
                "message": format!("Gap {} resolved", gap_id),
            }));
        } else {
            println!("Gap {} resolved", gap_id);
        }
    } else if let Some(cap) = capability {
        let cap_lower = cap.trim().to_lowercase();
        let found = gaps
            .iter_mut()
            .find(|g| g.capability.trim().to_lowercase() == cap_lower);
        if let Some(g) = found {
            g.resolved = true;
        } else {
            error_exit(&format!("Gap for capability '{}' not found", cap));
        }

        json_store::gaps_write(&flow_dir, epic_id, &gaps).unwrap_or_else(|e| {
            error_exit(&format!("Failed to resolve gap: {e}"));
        });

        if json_mode {
            json_output(json!({
                "capability": cap,
                "changed": true,
                "message": format!("Gap for '{}' resolved", cap),
            }));
        } else {
            println!("Gap for '{}' resolved", cap);
        }
    } else {
        error_exit("Either --capability or --id is required");
    }
}

fn cmd_gap_check(json_mode: bool, epic_id: &str) {
    validate_epic(json_mode, epic_id);
    let flow_dir = gap_flow_dir();
    let all_gaps = json_store::gaps_read(&flow_dir, epic_id).unwrap_or_default();

    let open_blocking: Vec<&GapEntry> = all_gaps
        .iter()
        .filter(|g| !g.resolved && GAP_BLOCKING_PRIORITIES.contains(&g.priority.as_str()))
        .collect();

    let open_non_blocking: Vec<&GapEntry> = all_gaps
        .iter()
        .filter(|g| !g.resolved && !GAP_BLOCKING_PRIORITIES.contains(&g.priority.as_str()))
        .collect();

    let resolved: Vec<&GapEntry> = all_gaps.iter().filter(|g| g.resolved).collect();

    let gate = if open_blocking.is_empty() {
        "pass"
    } else {
        "fail"
    };

    if json_mode {
        let to_json = |gaps: &[&GapEntry]| -> Vec<serde_json::Value> {
            gaps.iter()
                .map(|g| {
                    json!({
                        "id": g.id,
                        "capability": g.capability,
                        "priority": g.priority,
                        "status": if g.resolved { "resolved" } else { "open" },
                    })
                })
                .collect()
        };
        json_output(json!({
            "epic": epic_id,
            "gate": gate,
            "total": all_gaps.len(),
            "open_blocking": to_json(&open_blocking),
            "open_non_blocking": to_json(&open_non_blocking),
            "resolved": to_json(&resolved),
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
            println!("  \u{2717} [{}] {}", g.priority, g.capability);
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
    fn test_blocking_priorities() {
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"required"));
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"important"));
        assert!(!GAP_BLOCKING_PRIORITIES.contains(&"nice-to-have"));
    }
}
