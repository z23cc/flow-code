//! Gap registry commands: gap add, list, resolve, check.
//!
//! Gaps track requirement deficiencies in an epic. They are stored in
//! the DB `gaps` table (sole source of truth). Blocking gaps
//! (required/important) prevent epic closure.

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::id::is_epic_id;

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

/// Open DB connection (hard error if unavailable).
fn require_db() -> crate::commands::db_shim::Connection {
    crate::commands::db_shim::require_db()
        .unwrap_or_else(|e| error_exit(&format!("DB required: {e}")))
}

/// Verify .flow/ exists, epic ID is valid, and epic exists (DB or JSON).
fn validate_epic(_json: bool, epic_id: &str) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    if !is_epic_id(epic_id) {
        error_exit(&format!("Invalid epic ID: {}", epic_id));
    }
    // Check DB first, fall back to JSON file existence.
    // Epic may exist only in JSON if DB upsert hasn't run yet.
    let conn = require_db();
    let repo = crate::commands::db_shim::EpicRepo::new(&conn);
    if repo.get(epic_id).is_ok() {
        return;
    }
    let json_path = flow_dir.join("epics").join(format!("{epic_id}.json"));
    if json_path.exists() {
        return;
    }
    error_exit(&format!("Epic not found: {}", epic_id));
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
    validate_epic(json_mode, epic_id);
    let conn = require_db();
    let gap_repo = crate::commands::db_shim::GapRepo::new(&conn);

    // Check for existing gap with same capability (idempotent)
    if let Ok(existing) = gap_repo.list(epic_id, None) {
        let cap_lower = capability.trim().to_lowercase();
        if let Some(gap) = existing.iter().find(|g| g.capability.trim().to_lowercase() == cap_lower) {
            if json_mode {
                json_output(json!({
                    "id": gap.id,
                    "created": false,
                    "gap": {
                        "id": gap.id,
                        "capability": gap.capability,
                        "priority": gap.priority,
                        "status": gap.status,
                        "source": gap.source,
                        "task": gap.task_id,
                    },
                    "message": format!("Gap already exists: {}", gap.id),
                }));
            } else {
                println!(
                    "Gap already exists: {} \u{2014} {}",
                    gap.id, gap.capability
                );
            }
            return;
        }
    }

    match gap_repo.add(epic_id, capability.trim(), priority, Some(source), task) {
        Ok(gap_id) => {
            if json_mode {
                json_output(json!({
                    "id": gap_id,
                    "created": true,
                    "gap": {
                        "id": gap_id,
                        "capability": capability.trim(),
                        "priority": priority,
                        "status": "open",
                        "source": source,
                        "task": task,
                    },
                    "message": format!("Gap {} added to {}", gap_id, epic_id),
                }));
            } else {
                println!("Gap {} added: [{}] {}", gap_id, priority, capability.trim());
            }
        }
        Err(e) => {
            error_exit(&format!("Failed to add gap: {e}"));
        }
    }
}

fn cmd_gap_list(json_mode: bool, epic_id: &str, status_filter: Option<&str>) {
    validate_epic(json_mode, epic_id);
    let conn = require_db();
    let gap_repo = crate::commands::db_shim::GapRepo::new(&conn);

    let gaps = gap_repo.list(epic_id, status_filter).unwrap_or_default();

    if json_mode {
        let gap_values: Vec<serde_json::Value> = gaps
            .iter()
            .map(|g| {
                json!({
                    "id": g.id,
                    "capability": g.capability,
                    "priority": g.priority,
                    "status": g.status,
                    "source": g.source,
                    "task": g.task_id,
                    "added_at": g.created_at,
                    "resolved_at": g.resolved_at,
                    "evidence": g.evidence,
                })
            })
            .collect();
        json_output(json!({
            "epic": epic_id,
            "count": gaps.len(),
            "gaps": gap_values,
        }));
    } else if gaps.is_empty() {
        let suffix = status_filter
            .map(|s| format!(" (status={})", s))
            .unwrap_or_default();
        let msg = format!("No gaps for {}{}", epic_id, suffix);
        pretty_output("gap", &msg);
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        for g in &gaps {
            let marker = if g.status == "resolved" {
                "\u{2713}"
            } else {
                "\u{2717}"
            };
            writeln!(
                buf,
                "  {} {} [{}] {}",
                marker,
                g.id,
                g.priority,
                g.capability,
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
    evidence: &str,
) {
    validate_epic(json_mode, epic_id);
    let conn = require_db();
    let gap_repo = crate::commands::db_shim::GapRepo::new(&conn);

    if let Some(direct_id) = gap_id_direct {
        // Resolve by numeric ID
        let gap_id: i64 = direct_id
            .parse()
            .unwrap_or_else(|_| error_exit(&format!("Invalid gap ID: {}", direct_id)));

        if let Err(e) = gap_repo.resolve(gap_id, evidence) {
            error_exit(&format!("Failed to resolve gap {}: {e}", gap_id));
        }

        if json_mode {
            json_output(json!({
                "id": gap_id,
                "changed": true,
                "message": format!("Gap {} resolved", gap_id),
            }));
        } else {
            println!("Gap {} resolved: {}", gap_id, evidence);
        }
    } else if let Some(cap) = capability {
        // Resolve by capability name
        if let Err(e) = gap_repo.resolve_by_capability(epic_id, cap, evidence) {
            error_exit(&format!("Failed to resolve gap by capability '{}': {e}", cap));
        }

        if json_mode {
            json_output(json!({
                "capability": cap,
                "changed": true,
                "message": format!("Gap for '{}' resolved", cap),
            }));
        } else {
            println!("Gap for '{}' resolved: {}", cap, evidence);
        }
    } else {
        error_exit("Either --capability or --id is required");
    }
}

fn cmd_gap_check(json_mode: bool, epic_id: &str) {
    validate_epic(json_mode, epic_id);
    let conn = require_db();
    let gap_repo = crate::commands::db_shim::GapRepo::new(&conn);

    let all_gaps = gap_repo.list(epic_id, None).unwrap_or_default();

    let open_blocking: Vec<&crate::commands::db_shim::GapRow> = all_gaps
        .iter()
        .filter(|g| g.status == "open" && GAP_BLOCKING_PRIORITIES.contains(&g.priority.as_str()))
        .collect();

    let open_non_blocking: Vec<&crate::commands::db_shim::GapRow> = all_gaps
        .iter()
        .filter(|g| g.status == "open" && !GAP_BLOCKING_PRIORITIES.contains(&g.priority.as_str()))
        .collect();

    let resolved: Vec<&crate::commands::db_shim::GapRow> = all_gaps
        .iter()
        .filter(|g| g.status == "resolved")
        .collect();

    let gate = if open_blocking.is_empty() {
        "pass"
    } else {
        "fail"
    };

    if json_mode {
        let to_json = |gaps: &[&crate::commands::db_shim::GapRow]| -> Vec<serde_json::Value> {
            gaps.iter()
                .map(|g| {
                    json!({
                        "id": g.id,
                        "capability": g.capability,
                        "priority": g.priority,
                        "status": g.status,
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
            println!(
                "  \u{2717} [{}] {}",
                g.priority,
                g.capability,
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
    fn test_blocking_priorities() {
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"required"));
        assert!(GAP_BLOCKING_PRIORITIES.contains(&"important"));
        assert!(!GAP_BLOCKING_PRIORITIES.contains(&"nice-to-have"));
    }
}
