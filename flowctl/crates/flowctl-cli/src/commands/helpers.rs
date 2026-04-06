//! Shared helpers used across multiple command modules.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use flowctl_core::types::FLOW_DIR;

/// Resolve the shared flow directory.
///
/// In a git repo, uses `git rev-parse --git-common-dir` so all worktrees
/// share one `.flow/` state (at `.git/flow-state/flow/`).
/// Falls back to `$CWD/.flow/` outside a git repo.
pub fn get_flow_dir() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_flow_dir(&cwd)
}

/// Inner resolver, testable with explicit working dir.
pub fn resolve_flow_dir(working_dir: &Path) -> PathBuf {
    let git_result = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(working_dir)
        .output();

    match git_result {
        Ok(output) if output.status.success() => {
            let git_common = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let git_common_path = if Path::new(&git_common).is_absolute() {
                PathBuf::from(git_common)
            } else {
                working_dir.join(git_common)
            };
            git_common_path.join("flow-state").join("flow")
        }
        _ => working_dir.join(FLOW_DIR),
    }
}

/// Resolve current actor: FLOW_ACTOR env > git config user.email > git config user.name > $USER > "unknown"
pub fn resolve_actor() -> String {
    if let Ok(actor) = env::var("FLOW_ACTOR") {
        let trimmed = actor.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    if let Ok(output) = Command::new("git")
        .args(["config", "user.email"])
        .output()
    {
        if output.status.success() {
            let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !email.is_empty() {
                return email;
            }
        }
    }

    if let Ok(output) = Command::new("git")
        .args(["config", "user.name"])
        .output()
    {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    if let Ok(user) = env::var("USER") {
        if !user.is_empty() {
            return user;
        }
    }

    "unknown".to_string()
}
