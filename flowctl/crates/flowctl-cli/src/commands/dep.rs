//! Dependency management commands: add, remove.

use chrono::Utc;
use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::{epic_id_from_task, is_task_id};

use super::helpers::get_flow_dir;

#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// Add a dependency.
    Add {
        /// Task ID.
        task: String,
        /// Dependency task ID.
        depends_on: String,
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

pub fn dispatch(cmd: &DepCmd, json: bool) {
    match cmd {
        DepCmd::Add { task, depends_on } => cmd_dep_add(json, task, depends_on),
        DepCmd::Rm { task, depends_on } => cmd_dep_rm(json, task, depends_on),
    }
}

fn cmd_dep_add(json: bool, task_id: &str, depends_on: &str) {
    let flow_dir = ensure_flow_exists();

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

    // Validate same epic
    let task_epic = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from task ID: {}", task_id)));
    let dep_epic = epic_id_from_task(depends_on)
        .unwrap_or_else(|_| error_exit(&format!("Cannot parse epic from dep ID: {}", depends_on)));
    if task_epic != dep_epic {
        error_exit(&format!(
            "Dependencies must be within the same epic. Task {} is in {}, dependency {} is in {}",
            task_id, task_epic, depends_on, dep_epic
        ));
    }

    let mut task = flowctl_core::json_store::task_read(&flow_dir, task_id)
        .unwrap_or_else(|_| error_exit(&format!("Task not found: {}", task_id)));

    if !task.depends_on.contains(&depends_on.to_string()) {
        task.depends_on.push(depends_on.to_string());
        task.updated_at = Utc::now();
        if let Err(e) = flowctl_core::json_store::task_write_definition(&flow_dir, &task) {
            error_exit(&format!("Failed to write task: {e}"));
        }
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

fn cmd_dep_rm(json: bool, task_id: &str, depends_on: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }
    if !is_task_id(depends_on) {
        error_exit(&format!("Invalid dependency ID: {}", depends_on));
    }

    let mut task = flowctl_core::json_store::task_read(&flow_dir, task_id)
        .unwrap_or_else(|_| error_exit(&format!("Task not found: {}", task_id)));

    if let Some(pos) = task.depends_on.iter().position(|d| d == depends_on) {
        task.depends_on.remove(pos);
        task.updated_at = Utc::now();
        if let Err(e) = flowctl_core::json_store::task_write_definition(&flow_dir, &task) {
            error_exit(&format!("Failed to write task: {e}"));
        }

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
