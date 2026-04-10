//! Dependency management commands: add, remove.

use std::path::Path;

use chrono::Utc;
use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::changes::{Changes, Mutation};
use flowctl_core::id::{epic_id_from_task, is_task_id};

use super::helpers::get_flow_dir;

#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// Add a dependency.
    Add {
        /// Task ID.
        #[arg(required_unless_present = "input_json")]
        task: Option<String>,
        /// Dependency task ID.
        #[arg(required_unless_present = "input_json")]
        depends_on: Option<String>,
        /// JSON payload input: {"task": "...", "depends_on": "..."}
        #[arg(long)]
        input_json: Option<String>,
    },
    /// Remove a dependency.
    Rm {
        /// Task ID.
        task: String,
        /// Dependency to remove.
        depends_on: String,
    },
}

fn ensure_flow_exists() -> std::path::PathBuf {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    flow_dir
}

pub fn dispatch(cmd: &DepCmd, json: bool, dry_run: bool) {
    match cmd {
        DepCmd::Add {
            task,
            depends_on,
            input_json,
        } => {
            let (r_task, r_dep) = if let Some(ij) = input_json {
                use super::helpers::{json_str, parse_input_json, validate_json_fields};
                let val = parse_input_json(ij);
                validate_json_fields(&val, &["task", "depends_on"], &["task", "depends_on"]);
                (
                    json_str(&val, "task").unwrap_or_default(),
                    json_str(&val, "depends_on").unwrap_or_default(),
                )
            } else {
                (
                    task.clone().unwrap_or_default(),
                    depends_on.clone().unwrap_or_default(),
                )
            };
            cmd_dep_add(json, &r_task, &r_dep, dry_run)
        }
        DepCmd::Rm { task, depends_on } => cmd_dep_rm(json, task, depends_on, dry_run),
    }
}

/// Pure compute: build Changes to add a dependency.
/// Returns (updated task, whether dep was actually added).
fn compute_dep_add(
    flow_dir: &Path,
    task_id: &str,
    depends_on: &str,
) -> (flowctl_core::types::Task, bool, Changes) {
    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }
    if !is_task_id(depends_on) {
        error_exit(&format!(
            "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            depends_on
        ));
    }

    let task_epic = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from task ID: {}", task_id)));
    let dep_epic = epic_id_from_task(depends_on)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from dep ID: {}", depends_on)));
    if task_epic != dep_epic {
        error_exit(&format!(
            "Dependencies must be within the same epic. Task {} is in {}, dependency {} is in {}.\nHint: use the full task ID format: {}.N",
            task_id, task_epic, depends_on, dep_epic, task_epic
        ));
    }

    let mut task = flowctl_core::json_store::task_read(flow_dir, task_id)
        .unwrap_or_else(|_| error_exit(&format!("Task not found: {}", task_id)));

    let added = if !task.depends_on.contains(&depends_on.to_string()) {
        task.depends_on.push(depends_on.to_string());
        task.updated_at = Utc::now();
        true
    } else {
        false
    };

    let changes = if added {
        Changes::new().with(Mutation::UpdateTask { task: task.clone() })
    } else {
        Changes::new()
    };

    (task, added, changes)
}

fn cmd_dep_add(json: bool, task_id: &str, depends_on: &str, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    // Auto-expand short IDs: if task_id is a full ID, use its epic to expand depends_on
    let task_epic_str = epic_id_from_task(task_id).unwrap_or_default();
    let expanded_dep = if !task_epic_str.is_empty() {
        flowctl_core::id::expand_dep_id(depends_on, &task_epic_str)
    } else {
        depends_on.to_string()
    };
    let depends_on = expanded_dep.as_str();

    let (task, _added, changes) = compute_dep_add(&flow_dir, task_id, depends_on);

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    if json {
        json_output(json!({
            "task": task_id,
            "depends_on": task.depends_on,
            "message": format!("Dependency {} added to {}", depends_on, task_id),
        }));
    } else {
        println!("Dependency {} added to {}", depends_on, task_id);
    }
}

/// Pure compute: build Changes to remove a dependency.
/// Returns (updated task, whether dep was actually removed).
fn compute_dep_rm(
    flow_dir: &Path,
    task_id: &str,
    depends_on: &str,
) -> (flowctl_core::types::Task, bool, Changes) {
    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }
    if !is_task_id(depends_on) {
        error_exit(&format!("Invalid dependency ID: {}", depends_on));
    }

    let mut task = flowctl_core::json_store::task_read(flow_dir, task_id)
        .unwrap_or_else(|_| error_exit(&format!("Task not found: {}", task_id)));

    let removed = if let Some(pos) = task.depends_on.iter().position(|d| d == depends_on) {
        task.depends_on.remove(pos);
        task.updated_at = Utc::now();
        true
    } else {
        false
    };

    let changes = if removed {
        Changes::new().with(Mutation::UpdateTask { task: task.clone() })
    } else {
        Changes::new()
    };

    (task, removed, changes)
}

fn cmd_dep_rm(json: bool, task_id: &str, depends_on: &str, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    let (task, removed, changes) = compute_dep_rm(&flow_dir, task_id, depends_on);

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    if removed {
        if json {
            json_output(json!({
                "task": task_id,
                "depends_on": task.depends_on,
                "removed": true,
                "message": format!("Dependency {} removed from {}", depends_on, task_id),
            }));
        } else {
            println!("Dependency {} removed from {}", depends_on, task_id);
        }
    } else if json {
        json_output(json!({
            "task": task_id,
            "depends_on": task.depends_on,
            "removed": false,
            "message": format!("{} was not in {}'s dependencies", depends_on, task_id),
        }));
    } else {
        println!("{} was not in {}'s dependencies", depends_on, task_id);
    }
}
