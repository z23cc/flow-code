//! Outputs commands: write, list, show.
//!
//! Thin CLI wrapper over `flowctl_service::outputs::OutputsStore`. Provides
//! a lightweight narrative handoff layer at `.flow/outputs/<task-id>.md` that
//! workers populate in Phase 5c and read during Phase 1 re-anchor.

use std::fs;
use std::io::Read;

use clap::Subcommand;
use serde_json::json;

use flowctl_core::id::is_task_id;
use flowctl_service::outputs::OutputsStore;

use crate::output::{error_exit, json_output};

use super::helpers::get_flow_dir;

#[derive(Subcommand, Debug)]
pub enum OutputsCmd {
    /// Write output markdown for a task (from file or stdin via '-').
    Write {
        /// Task ID (e.g. fn-1.3).
        task_id: String,
        /// Path to content file, or '-' for stdin.
        #[arg(long)]
        file: String,
    },
    /// List prior outputs for an epic, newest-first.
    List {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Max entries to return.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Print the full markdown content for a task's output file.
    Show {
        /// Task ID (e.g. fn-1.3).
        task_id: String,
    },
}

pub fn dispatch(cmd: &OutputsCmd, json: bool) {
    match cmd {
        OutputsCmd::Write { task_id, file } => cmd_outputs_write(json, task_id, file),
        OutputsCmd::List { epic, limit } => cmd_outputs_list(json, epic, *limit),
        OutputsCmd::Show { task_id } => cmd_outputs_show(json, task_id),
    }
}

/// Open the store, exiting cleanly if `.flow/` is missing.
fn open_store(json: bool) -> OutputsStore {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        if json {
            json_output(json!({"error": ".flow/ does not exist. Run 'flowctl init' first."}));
            std::process::exit(1);
        } else {
            error_exit(".flow/ does not exist. Run 'flowctl init' first.");
        }
    }
    match OutputsStore::new(&flow_dir) {
        Ok(store) => store,
        Err(e) => {
            if json {
                json_output(json!({"error": format!("failed to open outputs store: {e}")}));
                std::process::exit(1);
            } else {
                error_exit(&format!("failed to open outputs store: {e}"));
            }
        }
    }
}

fn cmd_outputs_write(json: bool, task_id: &str, file: &str) {
    if !is_task_id(task_id) {
        if json {
            json_output(json!({"error": format!("Invalid task ID: {task_id}")}));
            std::process::exit(1);
        } else {
            error_exit(&format!(
                "Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M"
            ));
        }
    }

    // Read content from file or stdin.
    let content = if file == "-" {
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            error_exit(&format!("failed to read stdin: {e}"));
        }
        buf
    } else {
        match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => error_exit(&format!("failed to read {file}: {e}")),
        }
    };

    let store = open_store(json);
    match store.write(task_id, &content) {
        Ok(path) => {
            if json {
                json_output(json!({
                    "task_id": task_id,
                    "path": path.to_string_lossy(),
                    "bytes": content.len(),
                }));
            } else {
                println!("Wrote {} ({} bytes)", path.display(), content.len());
            }
        }
        Err(e) => {
            if json {
                json_output(json!({"error": format!("write failed: {e}")}));
                std::process::exit(1);
            } else {
                error_exit(&format!("write failed: {e}"));
            }
        }
    }
}

fn cmd_outputs_list(json: bool, epic_id: &str, limit: Option<usize>) {
    let store = open_store(json);
    match store.list_for_epic(epic_id, limit) {
        Ok(entries) => {
            if json {
                let arr: Vec<_> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "task_id": e.task_id,
                            "path": e.path.to_string_lossy(),
                            "mtime": e.mtime,
                        })
                    })
                    .collect();
                json_output(json!({"entries": arr, "count": entries.len()}));
            } else if entries.is_empty() {
                println!("(no outputs for epic {epic_id})");
            } else {
                for e in &entries {
                    println!("{}\t{}", e.task_id, e.path.display());
                }
            }
        }
        Err(e) => {
            if json {
                json_output(json!({"error": format!("list failed: {e}")}));
                std::process::exit(1);
            } else {
                error_exit(&format!("list failed: {e}"));
            }
        }
    }
}

fn cmd_outputs_show(json: bool, task_id: &str) {
    if !is_task_id(task_id) {
        if json {
            json_output(json!({"error": format!("Invalid task ID: {task_id}")}));
            std::process::exit(1);
        } else {
            error_exit(&format!(
                "Invalid task ID: {task_id}. Expected format: fn-N.M or fn-N-slug.M"
            ));
        }
    }

    let store = open_store(json);
    match store.read(task_id) {
        Ok(content) => {
            if json {
                json_output(json!({
                    "task_id": task_id,
                    "content": content,
                }));
            } else {
                print!("{content}");
            }
        }
        Err(e) => {
            if json {
                json_output(json!({"error": format!("read failed: {e}")}));
                std::process::exit(1);
            } else {
                error_exit(&format!("read failed: {e}"));
            }
        }
    }
}
