//! Shared helpers used across multiple command modules.

use std::env;
use std::path::PathBuf;
use std::process::Command;

use flowctl_core::types::FLOW_DIR;

/// Get the .flow/ directory path (current directory + .flow/).
pub fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
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
