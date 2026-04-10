//! Recover task completion status from git history.
//!
//! When `.flow/` state is lost or corrupted but code is already written and
//! committed, this command scans git log for commits mentioning task IDs and
//! marks matched tasks as done.

use std::process::Command;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::json_store::TaskState;
use flowctl_core::state_machine::Status;
use flowctl_core::types::Evidence;

use super::workflow::ensure_flow_exists;

/// Check whether any git commit message mentions the given task ID.
fn git_log_mentions(task_id: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["log", "--oneline", "--all", &format!("--grep={task_id}")])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<String> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    if lines.is_empty() { None } else { Some(lines) }
}

/// Extract short commit hashes from `git log --oneline` output lines.
fn extract_commit_hashes(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect()
}

pub fn cmd_recover(json_mode: bool, epic_id: &str, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    // 1. Load all tasks for this epic.
    let tasks_map =
        flowctl_core::json_store::task_list_by_epic(&flow_dir, epic_id).unwrap_or_default();

    if tasks_map.is_empty() {
        if json_mode {
            json_output(json!({
                "epic": epic_id,
                "error": "No tasks found for epic",
            }));
        } else {
            error_exit(&format!("No tasks found for epic {epic_id}"));
        }
        return;
    }

    // 2. Verify git is available.
    let git_check = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output();
    if git_check.is_err() || !git_check.unwrap().status.success() {
        error_exit("Not in a git repository or git is not available");
    }

    // 3. For each task, check git log for mentions.
    let mut recovered: Vec<serde_json::Value> = Vec::new();
    let mut already_done: Vec<String> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();

    for task in &tasks_map {
        if task.status == Status::Done {
            already_done.push(task.id.clone());
            continue;
        }

        if let Some(matching_lines) = git_log_mentions(&task.id) {
            let commits = extract_commit_hashes(&matching_lines);

            if !dry_run {
                // Mark task as done with recovery evidence.
                let now = Utc::now();
                let evidence = Evidence {
                    commits: commits.clone(),
                    ..Evidence::default()
                };
                let task_state = TaskState {
                    status: Status::Done,
                    assignee: None,
                    claimed_at: None,
                    completed_at: Some(now),
                    evidence: Some(evidence),
                    blocked_reason: None,
                    duration_seconds: None,
                    baseline_rev: None,
                    final_rev: commits.last().cloned(),
                    retry_count: 0,
                    updated_at: now,
                };
                if let Err(e) =
                    flowctl_core::json_store::state_write(&flow_dir, &task.id, &task_state)
                {
                    if json_mode {
                        json_output(json!({
                            "error": format!("Failed to write state for {}: {e}", task.id),
                        }));
                    } else {
                        eprintln!("  warning: failed to write state for {}: {e}", task.id);
                    }
                    continue;
                }
            }

            recovered.push(json!({
                "id": task.id,
                "title": task.title,
                "commits": matching_lines,
            }));
        } else {
            not_found.push(task.id.clone());
        }
    }

    // 4. Output report.
    if json_mode {
        json_output(json!({
            "epic": epic_id,
            "dry_run": dry_run,
            "recovered": recovered,
            "already_done": already_done,
            "not_found": not_found,
            "summary": {
                "recovered_count": recovered.len(),
                "already_done_count": already_done.len(),
                "not_found_count": not_found.len(),
            },
        }));
    } else {
        let action = if dry_run {
            "Would recover"
        } else {
            "Recovered"
        };
        println!(
            "{action} {}/{} tasks for epic {epic_id}\n",
            recovered.len(),
            tasks_map.len()
        );

        if !recovered.is_empty() {
            println!(
                "  {} task(s) {}:",
                recovered.len(),
                if dry_run {
                    "matched in git"
                } else {
                    "marked done"
                }
            );
            for entry in &recovered {
                let id = entry["id"].as_str().unwrap_or("?");
                let title = entry["title"].as_str().unwrap_or("?");
                let commit_count = entry["commits"].as_array().map(|a| a.len()).unwrap_or(0);
                println!("    {id} \u{2014} {title} ({commit_count} commit(s))");
            }
        }

        if !already_done.is_empty() {
            println!("\n  {} task(s) already done:", already_done.len());
            for id in &already_done {
                println!("    {id}");
            }
        }

        if !not_found.is_empty() {
            println!("\n  {} task(s) with no matching commits:", not_found.len());
            for id in &not_found {
                println!("    {id}");
            }
        }
    }
}
