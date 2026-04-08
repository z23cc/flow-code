//! Checklist commands: init, check, uncheck, verify, show.
//!
//! Structured Definition of Done checklists for tasks. Stored in
//! JSON files under `.flow/checklists/<task-id>.json`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use super::helpers::get_flow_dir;

// ── Constants ─────────────────────────────────────────────────────

const CHECKLISTS_DIR: &str = "checklists";

// ── Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub label: String,
    pub checked: bool,
}

/// Category → item-key → item.
pub type ChecklistCategory = BTreeMap<String, ChecklistItem>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checklist {
    pub task_id: String,
    /// category → { key → ChecklistItem }
    pub items: BTreeMap<String, ChecklistCategory>,
}

#[derive(Subcommand, Debug)]
pub enum ChecklistCmd {
    /// Create a default DoD checklist for a task.
    Init {
        /// Task ID.
        #[arg(long)]
        task: String,
    },
    /// Mark a checklist item as checked.
    Check {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Item key (e.g. "spec_read", "lint_pass").
        #[arg(long)]
        item: String,
    },
    /// Unmark a checklist item.
    Uncheck {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Item key.
        #[arg(long)]
        item: String,
    },
    /// Verify all items are checked; exit 1 if any are missing.
    Verify {
        /// Task ID.
        #[arg(long)]
        task: String,
    },
    /// Display current checklist state.
    Show {
        /// Task ID.
        #[arg(long)]
        task: String,
    },
}

pub fn dispatch(cmd: &ChecklistCmd, json: bool) {
    match cmd {
        ChecklistCmd::Init { task } => cmd_init(json, task),
        ChecklistCmd::Check { task, item } => cmd_check(json, task, item),
        ChecklistCmd::Uncheck { task, item } => cmd_uncheck(json, task, item),
        ChecklistCmd::Verify { task } => cmd_verify(json, task),
        ChecklistCmd::Show { task } => cmd_show(json, task),
    }
}

// ── Helpers ───────────────────────────────────────────────────────

fn checklists_dir() -> PathBuf {
    get_flow_dir().join(CHECKLISTS_DIR)
}

fn checklist_path(task_id: &str) -> PathBuf {
    checklists_dir().join(format!("{}.json", task_id))
}

fn ensure_flow() {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
}

fn read_checklist(task_id: &str) -> Checklist {
    let path = checklist_path(task_id);
    if !path.exists() {
        error_exit(&format!(
            "No checklist for task {}. Run 'flowctl checklist init --task {}' first.",
            task_id, task_id
        ));
    }
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| error_exit(&format!("Failed to read checklist: {e}")));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to parse checklist JSON: {e}")))
}

fn write_checklist(cl: &Checklist) {
    let dir = checklists_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .unwrap_or_else(|e| error_exit(&format!("Failed to create checklists dir: {e}")));
    }
    let path = checklist_path(&cl.task_id);
    let json_str = serde_json::to_string_pretty(cl)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize checklist: {e}")));
    fs::write(&path, json_str)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write checklist: {e}")));
}

/// Find an item by key across all categories. Returns (category, key) if found.
fn find_item<'a>(cl: &'a Checklist, item_key: &'a str) -> Option<(&'a str, &'a str)> {
    for (cat, items) in &cl.items {
        if items.contains_key(item_key) {
            return Some((cat.as_str(), item_key));
        }
    }
    None
}

/// Mutably find and toggle an item. Returns the item label on success.
fn set_item_checked(cl: &mut Checklist, item_key: &str, checked: bool) -> String {
    for (_cat, items) in cl.items.iter_mut() {
        if let Some(item) = items.get_mut(item_key) {
            item.checked = checked;
            return item.label.clone();
        }
    }
    error_exit(&format!("Item '{}' not found in checklist for task {}", item_key, cl.task_id));
}

fn build_default_checklist(task_id: &str) -> Checklist {
    let mut items = BTreeMap::new();

    let mut context = BTreeMap::new();
    context.insert("spec_read".to_string(), ChecklistItem {
        label: "Task spec read and understood".to_string(),
        checked: false,
    });
    context.insert("architecture_compliant".to_string(), ChecklistItem {
        label: "Compliant with project architecture".to_string(),
        checked: false,
    });
    items.insert("context".to_string(), context);

    let mut implementation = BTreeMap::new();
    implementation.insert("all_ac_satisfied".to_string(), ChecklistItem {
        label: "All acceptance criteria satisfied".to_string(),
        checked: false,
    });
    implementation.insert("edge_cases_handled".to_string(), ChecklistItem {
        label: "Edge cases handled".to_string(),
        checked: false,
    });
    items.insert("implementation".to_string(), implementation);

    let mut testing = BTreeMap::new();
    testing.insert("unit_tests_added".to_string(), ChecklistItem {
        label: "Core functionality unit tests added".to_string(),
        checked: false,
    });
    testing.insert("existing_tests_pass".to_string(), ChecklistItem {
        label: "Existing tests pass (no regression)".to_string(),
        checked: false,
    });
    testing.insert("lint_pass".to_string(), ChecklistItem {
        label: "Lint and type checks pass".to_string(),
        checked: false,
    });
    items.insert("testing".to_string(), testing);

    let mut documentation = BTreeMap::new();
    documentation.insert("files_listed".to_string(), ChecklistItem {
        label: "Changed files list complete".to_string(),
        checked: false,
    });
    items.insert("documentation".to_string(), documentation);

    Checklist {
        task_id: task_id.to_string(),
        items,
    }
}

// ── Commands ──────────────────────────────────────────────────────

fn cmd_init(json_mode: bool, task_id: &str) {
    ensure_flow();

    let path = checklist_path(task_id);
    if path.exists() {
        // Idempotent: return existing checklist
        let cl = read_checklist(task_id);
        if json_mode {
            json_output(json!({
                "task_id": task_id,
                "created": false,
                "message": format!("Checklist already exists for {}", task_id),
                "checklist": cl,
            }));
        } else {
            println!("Checklist already exists for {}", task_id);
        }
        return;
    }

    let cl = build_default_checklist(task_id);
    write_checklist(&cl);

    if json_mode {
        json_output(json!({
            "task_id": task_id,
            "created": true,
            "message": format!("Checklist created for {}", task_id),
            "checklist": cl,
        }));
    } else {
        println!("Checklist created for {} (8 items in 4 categories)", task_id);
    }
}

fn cmd_check(json_mode: bool, task_id: &str, item_key: &str) {
    ensure_flow();
    let mut cl = read_checklist(task_id);

    // Check if item exists first
    if find_item(&cl, item_key).is_none() {
        error_exit(&format!("Item '{}' not found in checklist for task {}", item_key, task_id));
    }

    let label = set_item_checked(&mut cl, item_key, true);
    write_checklist(&cl);

    if json_mode {
        json_output(json!({
            "task_id": task_id,
            "item": item_key,
            "checked": true,
            "label": label,
            "message": format!("Checked: {}", label),
        }));
    } else {
        println!("\u{2713} {}", label);
    }
}

fn cmd_uncheck(json_mode: bool, task_id: &str, item_key: &str) {
    ensure_flow();
    let mut cl = read_checklist(task_id);

    if find_item(&cl, item_key).is_none() {
        error_exit(&format!("Item '{}' not found in checklist for task {}", item_key, task_id));
    }

    let label = set_item_checked(&mut cl, item_key, false);
    write_checklist(&cl);

    if json_mode {
        json_output(json!({
            "task_id": task_id,
            "item": item_key,
            "checked": false,
            "label": label,
            "message": format!("Unchecked: {}", label),
        }));
    } else {
        println!("\u{2717} {}", label);
    }
}

fn cmd_verify(json_mode: bool, task_id: &str) {
    ensure_flow();
    let cl = read_checklist(task_id);

    let mut total = 0u32;
    let mut checked_count = 0u32;
    let mut missing: Vec<serde_json::Value> = Vec::new();

    for (cat, items) in &cl.items {
        for (key, item) in items {
            total += 1;
            if item.checked {
                checked_count += 1;
            } else {
                missing.push(json!({
                    "category": cat,
                    "key": key,
                    "label": item.label,
                }));
            }
        }
    }

    let gate = if missing.is_empty() { "pass" } else { "fail" };

    if json_mode {
        json_output(json!({
            "task_id": task_id,
            "gate": gate,
            "total": total,
            "checked": checked_count,
            "missing_count": missing.len(),
            "missing": missing,
        }));
    } else if gate == "pass" {
        println!("DoD check PASS for {} ({}/{} items checked)", task_id, checked_count, total);
    } else {
        println!(
            "DoD check FAIL for {} \u{2014} {}/{} items unchecked:",
            task_id,
            missing.len(),
            total
        );
        for m in &missing {
            println!(
                "  \u{2717} [{}] {}",
                m["category"].as_str().unwrap_or(""),
                m["label"].as_str().unwrap_or("")
            );
        }
    }

    if gate == "fail" {
        std::process::exit(1);
    }
}

fn cmd_show(json_mode: bool, task_id: &str) {
    ensure_flow();
    let cl = read_checklist(task_id);

    if json_mode {
        // Count stats
        let mut total = 0u32;
        let mut checked_count = 0u32;
        for items in cl.items.values() {
            for item in items.values() {
                total += 1;
                if item.checked {
                    checked_count += 1;
                }
            }
        }
        json_output(json!({
            "task_id": task_id,
            "total": total,
            "checked": checked_count,
            "checklist": cl,
        }));
    } else {
        use std::fmt::Write as _;
        let mut buf = String::new();
        writeln!(buf, "Checklist for {}:", task_id).ok();
        for (cat, items) in &cl.items {
            writeln!(buf, "  [{}]", cat).ok();
            for (key, item) in items {
                let marker = if item.checked { "\u{2713}" } else { "\u{2717}" };
                writeln!(buf, "    {} {} \u{2014} {}", marker, key, item.label).ok();
            }
        }
        pretty_output("checklist", &buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_checklist_structure() {
        let cl = build_default_checklist("fn-1.1");
        assert_eq!(cl.task_id, "fn-1.1");
        assert_eq!(cl.items.len(), 4);
        assert!(cl.items.contains_key("context"));
        assert!(cl.items.contains_key("implementation"));
        assert!(cl.items.contains_key("testing"));
        assert!(cl.items.contains_key("documentation"));

        // All items should start unchecked
        for items in cl.items.values() {
            for item in items.values() {
                assert!(!item.checked);
            }
        }
    }

    #[test]
    fn test_default_checklist_item_count() {
        let cl = build_default_checklist("fn-1.1");
        let total: usize = cl.items.values().map(|cat| cat.len()).sum();
        assert_eq!(total, 8);
    }

    #[test]
    fn test_set_item_checked() {
        let mut cl = build_default_checklist("fn-1.1");
        let label = set_item_checked(&mut cl, "spec_read", true);
        assert_eq!(label, "Task spec read and understood");
        assert!(cl.items["context"]["spec_read"].checked);
    }

    #[test]
    fn test_find_item() {
        let cl = build_default_checklist("fn-1.1");
        let found = find_item(&cl, "lint_pass");
        assert!(found.is_some());
        let (cat, key) = found.unwrap();
        assert_eq!(cat, "testing");
        assert_eq!(key, "lint_pass");

        assert!(find_item(&cl, "nonexistent").is_none());
    }

    #[test]
    fn test_checklist_serialization_roundtrip() {
        let cl = build_default_checklist("fn-2.3");
        let json_str = serde_json::to_string_pretty(&cl).unwrap();
        let parsed: Checklist = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.task_id, "fn-2.3");
        assert_eq!(parsed.items.len(), 4);
    }
}
