//! Ralph run control commands: pause, resume, stop, status.
//!
//! Ralph runs live in `scripts/ralph/runs/<run-id>/`. Control is via
//! sentinel files (PAUSE, STOP) and progress is tracked in progress.txt.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Subcommand;
use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output};

#[derive(Subcommand, Debug)]
pub enum RalphCmd {
    /// Pause a Ralph run.
    Pause {
        /// Run ID (auto-detect if single).
        #[arg(long)]
        run: Option<String>,
    },
    /// Resume a paused Ralph run.
    Resume {
        /// Run ID (auto-detect if single).
        #[arg(long)]
        run: Option<String>,
    },
    /// Request a Ralph run to stop.
    Stop {
        /// Run ID (auto-detect if single).
        #[arg(long)]
        run: Option<String>,
    },
    /// Show Ralph run status.
    Status {
        /// Run ID (auto-detect if single).
        #[arg(long)]
        run: Option<String>,
    },
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Get repo root via `git rev-parse --show-toplevel`, falling back to cwd.
fn get_repo_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string())
        }
        _ => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Scan `scripts/ralph/runs/*/progress.txt` for active runs.
/// A run is active if progress.txt exists and does NOT contain both
/// `completion_reason=` and `promise=COMPLETE`.
fn find_active_runs() -> Vec<(String, PathBuf)> {
    let runs_dir = get_repo_root().join("scripts").join("ralph").join("runs");
    let mut active = Vec::new();

    let entries = match fs::read_dir(&runs_dir) {
        Ok(e) => e,
        Err(_) => return active,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let progress = path.join("progress.txt");
        if !progress.exists() {
            continue;
        }
        let content = fs::read_to_string(&progress).unwrap_or_default();
        if content.contains("completion_reason=") && content.contains("promise=COMPLETE") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        active.push((name, path));
    }

    active
}

/// Find a single active run. Auto-detect if run_id is None.
fn find_active_run(run_id: Option<&str>) -> (String, PathBuf) {
    let runs = find_active_runs();

    if let Some(rid) = run_id {
        for (name, path) in &runs {
            if name == rid {
                return (name.clone(), path.clone());
            }
        }
        error_exit(&format!("Run {rid} not found or not active"));
    }

    match runs.len() {
        0 => error_exit("No active runs"),
        1 => runs.into_iter().next().unwrap(),
        _ => {
            let ids: Vec<_> = runs.iter().map(|(n, _)| n.as_str()).collect();
            error_exit(&format!("Multiple active runs, specify --run: {}", ids.join(", ")));
        }
    }
}

/// Parse progress.txt for iteration, epic, and task info.
fn parse_progress(run_dir: &Path) -> (Option<i64>, Option<String>, Option<String>) {
    let progress = run_dir.join("progress.txt");
    let content = match fs::read_to_string(&progress) {
        Ok(c) => c,
        Err(_) => return (None, None, None),
    };

    let iter_re = Regex::new(r"(?i)iteration[:\s]+(\d+)").unwrap();
    let epic_re = Regex::new(r"(?i)epic[:\s]+(fn-[\w-]+)").unwrap();
    let task_re = Regex::new(r"(?i)task[:\s]+(fn-[\w.-]+\.\d+)").unwrap();

    let iteration = iter_re.captures(&content).and_then(|c| c[1].parse().ok());
    let epic = epic_re.captures(&content).map(|c| c[1].to_string());
    let task = task_re.captures(&content).map(|c| c[1].to_string());

    (iteration, epic, task)
}

// ── Dispatch ────────────────────────────────────────────────────────

pub fn dispatch(cmd: &RalphCmd, json: bool) {
    match cmd {
        RalphCmd::Pause { run } => cmd_pause(json, run.as_deref()),
        RalphCmd::Resume { run } => cmd_resume(json, run.as_deref()),
        RalphCmd::Stop { run } => cmd_stop(json, run.as_deref()),
        RalphCmd::Status { run } => cmd_status(json, run.as_deref()),
    }
}

// ── Commands ────────────────────────────────────────────────────────

fn cmd_pause(json_mode: bool, run_id: Option<&str>) {
    let (name, run_dir) = find_active_run(run_id);
    let pause_file = run_dir.join("PAUSE");
    let _ = fs::write(&pause_file, "");

    if json_mode {
        json_output(json!({"run": name, "action": "paused"}));
    } else {
        println!("Paused {name}");
    }
}

fn cmd_resume(json_mode: bool, run_id: Option<&str>) {
    let (name, run_dir) = find_active_run(run_id);
    let pause_file = run_dir.join("PAUSE");
    let _ = fs::remove_file(&pause_file);

    if json_mode {
        json_output(json!({"run": name, "action": "resumed"}));
    } else {
        println!("Resumed {name}");
    }
}

fn cmd_stop(json_mode: bool, run_id: Option<&str>) {
    let (name, run_dir) = find_active_run(run_id);
    let stop_file = run_dir.join("STOP");
    let _ = fs::write(&stop_file, "");

    if json_mode {
        json_output(json!({"run": name, "action": "stop_requested"}));
    } else {
        println!("Stop requested for {name}");
    }
}

fn cmd_status(json_mode: bool, run_id: Option<&str>) {
    let (name, run_dir) = find_active_run(run_id);
    let paused = run_dir.join("PAUSE").exists();
    let stopped = run_dir.join("STOP").exists();
    let (iteration, current_epic, current_task) = parse_progress(&run_dir);

    if json_mode {
        json_output(json!({
            "run": name,
            "iteration": iteration,
            "current_epic": current_epic,
            "current_task": current_task,
            "paused": paused,
            "stopped": stopped,
        }));
    } else {
        let mut state = Vec::new();
        if paused { state.push("PAUSED"); }
        if stopped { state.push("STOPPED"); }
        let state_str = if state.is_empty() {
            " [running]".to_string()
        } else {
            format!(" [{}]", state.join(", "))
        };

        let task_info = if let Some(ref t) = current_task {
            format!(", working on {t}")
        } else if let Some(ref e) = current_epic {
            format!(", epic {e}")
        } else {
            String::new()
        };

        let iter_info = if let Some(i) = iteration {
            format!("iteration {i}")
        } else {
            "starting".to_string()
        };

        println!("{name} ({iter_info}{task_info}){state_str}");
    }
}
