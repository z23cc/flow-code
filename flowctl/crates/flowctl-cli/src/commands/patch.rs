//! `flowctl patch-*` — fuzzy diff, patch application, and search-replace commands.
//!
//! Three subcommands:
//! - `patch-apply`: Apply a unified diff to a file (fuzzy context matching).
//! - `patch-diff`: Generate a unified diff between two files.
//! - `patch-replace`: Search-replace with fuzzy fallback (exact → whitespace → context).

use crate::output::{error_exit, json_output};
use clap::Subcommand;
use flowctl_core::patch;
use serde_json::json;
use std::fs;
use std::io::Read;

/// Patch subcommands.
#[derive(Subcommand, Debug)]
pub enum PatchCmd {
    /// Apply a unified diff to a file (fuzzy context matching).
    Apply {
        /// Target file to patch.
        #[arg(long)]
        file: String,
        /// Diff file path (or '-' for stdin).
        #[arg(long)]
        diff: String,
    },
    /// Generate a unified diff between two files.
    Diff {
        /// Original file path.
        #[arg(long)]
        old: String,
        /// Modified file path.
        #[arg(long)]
        new: String,
    },
    /// Search-replace with fuzzy fallback (exact → whitespace-normalized → context).
    Replace {
        /// Target file to modify.
        #[arg(long)]
        file: String,
        /// Text to find (exact or fuzzy).
        #[arg(long)]
        old: String,
        /// Replacement text.
        #[arg(long)]
        new: String,
    },
}

pub fn dispatch(cmd: &PatchCmd, json: bool) {
    match cmd {
        PatchCmd::Apply { file, diff } => cmd_patch_apply(json, file, diff),
        PatchCmd::Diff { old, new } => cmd_patch_diff(json, old, new),
        PatchCmd::Replace { file, old, new } => cmd_patch_replace(json, file, old, new),
    }
}

fn read_diff_input(diff_arg: &str) -> String {
    if diff_arg == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| {
                error_exit(&format!("Failed to read stdin: {e}"));
            });
        buf
    } else {
        fs::read_to_string(diff_arg).unwrap_or_else(|e| {
            error_exit(&format!("Failed to read diff file '{}': {e}", diff_arg));
        })
    }
}

fn cmd_patch_apply(json_flag: bool, file: &str, diff_arg: &str) {
    let content = fs::read_to_string(file).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read '{}': {e}", file));
    });
    let diff_text = read_diff_input(diff_arg);

    match patch::apply_diff(&content, &diff_text) {
        Ok(patched) => {
            fs::write(file, &patched).unwrap_or_else(|e| {
                error_exit(&format!("Failed to write '{}': {e}", file));
            });
            if json_flag {
                json_output(json!({
                    "patched": true,
                    "file": file,
                    "bytes": patched.len(),
                }));
            } else {
                eprintln!("Patched {} ({} bytes)", file, patched.len());
            }
        }
        Err(e) => {
            error_exit(&format!("Patch failed: {e}"));
        }
    }
}

fn cmd_patch_diff(json_flag: bool, old_path: &str, new_path: &str) {
    let old_content = fs::read_to_string(old_path).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read '{}': {e}", old_path));
    });
    let new_content = fs::read_to_string(new_path).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read '{}': {e}", new_path));
    });

    let diff = patch::create_diff(&old_content, &new_content);

    if json_flag {
        json_output(json!({
            "diff": diff,
            "old": old_path,
            "new": new_path,
            "hunks": diff.matches("@@ @@").count(),
        }));
    } else {
        print!("{diff}");
    }
}

fn cmd_patch_replace(json_flag: bool, file: &str, old_text: &str, new_text: &str) {
    let content = fs::read_to_string(file).unwrap_or_else(|e| {
        error_exit(&format!("Failed to read '{}': {e}", file));
    });

    match patch::fuzzy_replace(&content, old_text, new_text) {
        Ok(replaced) => {
            fs::write(file, &replaced).unwrap_or_else(|e| {
                error_exit(&format!("Failed to write '{}': {e}", file));
            });
            if json_flag {
                json_output(json!({
                    "replaced": true,
                    "file": file,
                    "bytes": replaced.len(),
                }));
            } else {
                eprintln!("Replaced in {} ({} bytes)", file, replaced.len());
            }
        }
        Err(e) => {
            error_exit(&format!("Replace failed: {e}"));
        }
    }
}
