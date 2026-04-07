//! Checkpoint commands: save, restore, delete.
//!
//! Checkpoints snapshot the SQLite database state for crash recovery.
//! Each checkpoint is a copy of the flowctl.db file stored alongside it
//! with an epic-specific suffix.

use std::fs;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::is_epic_id;
use super::helpers::get_flow_dir;

#[derive(Subcommand, Debug)]
pub enum CheckpointCmd {
    /// Save epic state to checkpoint.
    Save {
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
    /// Restore epic state from checkpoint.
    Restore {
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
    /// Delete checkpoint for epic.
    Delete {
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
}

pub fn dispatch(cmd: &CheckpointCmd, json: bool) {
    match cmd {
        CheckpointCmd::Save { epic } => cmd_checkpoint_save(json, epic),
        CheckpointCmd::Restore { epic } => cmd_checkpoint_restore(json, epic),
        CheckpointCmd::Delete { epic } => cmd_checkpoint_delete(json, epic),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Resolve the checkpoint file path for a given epic.
/// Checkpoints are stored in the state directory alongside the main database.
fn checkpoint_path(epic_id: &str) -> Result<std::path::PathBuf, String> {
    let flow_dir = get_flow_dir();
    let state_dir = flow_dir.join(".state");
    std::fs::create_dir_all(&state_dir)
        .map_err(|e| format!("Cannot create state dir: {}", e))?;
    Ok(state_dir.join(format!("checkpoint-{}.json", epic_id)))
}

fn validate_prerequisites(epic_id: &str) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    if !is_epic_id(epic_id) {
        error_exit(&format!("Invalid epic ID: {}", epic_id));
    }
}

// ── Commands ───────────────────────────────────────────────────────

fn cmd_checkpoint_save(json_mode: bool, epic_id: &str) {
    validate_prerequisites(epic_id);

    let flow_dir = get_flow_dir();

    // Snapshot: serialize current epic + tasks state to a checkpoint JSON
    let epic = flowctl_core::json_store::epic_read(&flow_dir, epic_id).ok();
    let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, epic_id).unwrap_or_default();

    let mut task_states = Vec::new();
    for task in &tasks {
        let state = flowctl_core::json_store::state_read(&flow_dir, &task.id).ok();
        task_states.push(json!({"task": task, "state": state}));
    }

    let checkpoint = json!({
        "epic": epic,
        "tasks": task_states,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let dst = match checkpoint_path(epic_id) {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let content = serde_json::to_string_pretty(&checkpoint).unwrap_or_default();
    if let Err(e) = fs::write(&dst, &content) {
        error_exit(&format!("Failed to save checkpoint: {}", e));
    }

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "checkpoint": dst.to_string_lossy(),
            "size_bytes": content.len(),
            "message": format!("Checkpoint saved for {}", epic_id),
        }));
    } else {
        println!("Checkpoint saved for {} ({} bytes)", epic_id, content.len());
    }
}

fn cmd_checkpoint_restore(json_mode: bool, epic_id: &str) {
    validate_prerequisites(epic_id);

    let src = match checkpoint_path(epic_id) {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    if !src.exists() {
        error_exit(&format!(
            "No checkpoint found for {}. Save one first with 'flowctl checkpoint save'.",
            epic_id
        ));
    }

    // Read checkpoint, restore task states
    let content = fs::read_to_string(&src).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read checkpoint: {}", e));
    });
    let checkpoint: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|e| {
        error_exit(&format!("Invalid checkpoint JSON: {}", e));
    });

    let flow_dir = get_flow_dir();
    if let Some(tasks) = checkpoint.get("tasks").and_then(|t| t.as_array()) {
        for entry in tasks {
            if let (Some(task_id), Some(state)) = (
                entry.get("task").and_then(|t| t.get("id")).and_then(|i| i.as_str()),
                entry.get("state"),
            ) {
                if !state.is_null() {
                    if let Ok(task_state) = serde_json::from_value::<flowctl_core::json_store::TaskState>(state.clone()) {
                        let _ = flowctl_core::json_store::state_write(&flow_dir, task_id, &task_state);
                    }
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "restored_from": src.to_string_lossy(),
            "message": format!("Checkpoint restored for {}", epic_id),
        }));
    } else {
        println!("Checkpoint restored for {}", epic_id);
    }
}

fn cmd_checkpoint_delete(json_mode: bool, epic_id: &str) {
    validate_prerequisites(epic_id);

    let path = match checkpoint_path(epic_id) {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    if !path.exists() {
        if json_mode {
            json_output(json!({
                "epic": epic_id,
                "deleted": false,
                "message": format!("No checkpoint found for {}", epic_id),
            }));
        } else {
            println!("No checkpoint found for {}", epic_id);
        }
        return;
    }

    if let Err(e) = fs::remove_file(&path) {
        error_exit(&format!(
            "Failed to delete checkpoint: {}",
            e
        ));
    }

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "deleted": true,
            "message": format!("Checkpoint deleted for {}", epic_id),
        }));
    } else {
        println!("Checkpoint deleted for {}", epic_id);
    }
}
