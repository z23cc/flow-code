//! Epic history commands: replay, diff.

use serde_json::json;

use crate::output::{error_exit, json_output};

use super::helpers::{ensure_flow_exists, load_epic_branch, validate_epic_id};
use super::super::helpers::get_flow_dir;

pub fn cmd_replay(json_mode: bool, epic_id: &str, dry_run: bool, force: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    let flow_dir = get_flow_dir();

    // Load tasks from JSON
    let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, epic_id).unwrap_or_default();
    if tasks.is_empty() {
        error_exit(&format!("No tasks found for epic {}", epic_id));
    }

    // Check for in_progress tasks unless force
    if !force {
        let in_progress: Vec<&str> = tasks
            .iter()
            .filter(|t| t.status == flowctl_core::state_machine::Status::InProgress)
            .map(|t| t.id.as_str())
            .collect();
        if !in_progress.is_empty() {
            error_exit(&format!(
                "Tasks in progress: {}. Use --force to override.",
                in_progress.join(", ")
            ));
        }
    }

    // Count what would be reset
    let to_reset: Vec<&flowctl_core::types::Task> = tasks
        .iter()
        .filter(|t| t.status != flowctl_core::state_machine::Status::Todo)
        .collect();

    if dry_run {
        if json_mode {
            let ids: Vec<&str> = to_reset.iter().map(|t| t.id.as_str()).collect();
            json_output(json!({
                "dry_run": true,
                "epic": epic_id,
                "would_reset": ids,
                "count": ids.len(),
            }));
        } else {
            println!("Dry run — would reset {} task(s) to todo:", to_reset.len());
            for t in &to_reset {
                println!("  {} ({}) -> todo", t.id, t.status);
            }
        }
        return;
    }

    // Reset all tasks to todo via JSON state
    let mut reset_count = 0;
    for task in &to_reset {
        let blank = flowctl_core::json_store::TaskState::default();
        if let Err(e) = flowctl_core::json_store::state_write(&flow_dir, &task.id, &blank) {
            eprintln!("Warning: failed to reset {} state: {}", task.id, e);
        }
        reset_count += 1;
    }

    if json_mode {
        let ids: Vec<&str> = to_reset.iter().map(|t| t.id.as_str()).collect();
        json_output(json!({
            "epic": epic_id,
            "reset": ids,
            "count": reset_count,
            "message": format!("Run /flow-code:work {} to re-execute", epic_id),
        }));
    } else {
        println!("Reset {} task(s) to todo for epic {}", reset_count, epic_id);
        println!();
        println!("To re-execute, run:  /flow-code:work {}", epic_id);
    }
}

pub fn cmd_diff(json_mode: bool, epic_id: &str) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    // Load epic to get branch name from DB
    let branch = load_epic_branch(epic_id);

    let branch = match branch {
        Some(b) => b,
        None => error_exit(&format!(
            "No branch found for epic {}. Set with: flowctl epic set-branch {} --branch <name>",
            epic_id, epic_id
        )),
    };

    // Find merge base with main
    let merge_base = std::process::Command::new("git")
        .args(["merge-base", "main", &branch])
        .output();

    let base_ref = match merge_base {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Fallback: try to use the branch directly
            eprintln!("Warning: could not find merge-base with main, showing full branch history");
            String::new()
        }
    };

    // Git log
    let range_spec = format!("{}..{}", base_ref, branch);
    let log_output = if base_ref.is_empty() {
        std::process::Command::new("git")
            .args(["log", "--oneline", "-20", &branch])
            .output()
    } else {
        std::process::Command::new("git")
            .args(["log", "--oneline", &range_spec])
            .output()
    };

    let log_text = match log_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    // Git diff --stat
    let diff_output = if base_ref.is_empty() {
        std::process::Command::new("git")
            .args(["diff", "--stat", &branch])
            .output()
    } else {
        std::process::Command::new("git")
            .args(["diff", "--stat", &range_spec])
            .output()
    };

    let diff_text = match diff_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "branch": branch,
            "base_ref": if base_ref.is_empty() { None } else { Some(&base_ref) },
            "log": log_text,
            "diff_stat": diff_text,
        }));
    } else {
        println!("Epic: {}  Branch: {}", epic_id, branch);
        if !base_ref.is_empty() {
            println!("Base: {}", &base_ref[..base_ref.len().min(12)]);
        }
        println!();

        if !log_text.is_empty() {
            println!("Commits:");
            for line in log_text.lines() {
                println!("  {}", line);
            }
            println!();
        } else {
            println!("No commits found.");
            println!();
        }

        if !diff_text.is_empty() {
            println!("Diff summary:");
            for line in diff_text.lines() {
                println!("  {}", line);
            }
        } else {
            println!("No diff.");
        }
    }
}
