//! Checkpoint commands: save, restore, delete.
//!
//! Checkpoints snapshot the SQLite database state for crash recovery.
//! Each checkpoint is a copy of the flowctl.db file stored alongside it
//! with an epic-specific suffix.

use std::env;
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
    let cwd = env::current_dir().map_err(|e| format!("Cannot get cwd: {}", e))?;
    let state_dir = crate::commands::db_shim::resolve_state_dir(&cwd)
        .map_err(|e| format!("Cannot resolve state dir: {}", e))?;
    Ok(state_dir.join(format!("checkpoint-{}.db", epic_id)))
}

/// Resolve the main database path.
fn db_path() -> Result<std::path::PathBuf, String> {
    let cwd = env::current_dir().map_err(|e| format!("Cannot get cwd: {}", e))?;
    crate::commands::db_shim::resolve_db_path(&cwd).map_err(|e| format!("Cannot resolve db path: {}", e))
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

    let src = match db_path() {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    if !src.exists() {
        error_exit("No database found. Run 'flowctl init' and index first.");
    }

    let dst = match checkpoint_path(epic_id) {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    // Ensure parent directory exists
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Copy the database file (SQLite WAL-safe: we copy the main file;
    // for a fully safe checkpoint we'd use the backup API, but a file
    // copy is sufficient for crash recovery purposes).
    if let Err(e) = fs::copy(&src, &dst) {
        error_exit(&format!(
            "Failed to save checkpoint: {}",
            e
        ));
    }

    let size = fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "checkpoint": dst.to_string_lossy(),
            "size_bytes": size,
            "message": format!("Checkpoint saved for {}", epic_id),
        }));
    } else {
        println!(
            "Checkpoint saved for {} ({} bytes)",
            epic_id, size
        );
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

    let dst = match db_path() {
        Ok(p) => p,
        Err(e) => error_exit(&e),
    };

    // Ensure parent directory exists
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Err(e) = fs::copy(&src, &dst) {
        error_exit(&format!(
            "Failed to restore checkpoint: {}",
            e
        ));
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
