//! Shared helpers used across multiple command modules.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use flowctl_core::types::FLOW_DIR;

/// Get the .flow/ directory path.
///
/// Returns `$CWD/.flow/` which is expected to be a symlink to the shared
/// state dir (`.git/flow-state/flow/`) in git repos. The symlink is created
/// by `flowctl init` and by the worktree kit on worktree creation.
pub fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
}

/// Resolve the shared flow state directory (real path, not symlink).
///
/// In a git repo: `.git/flow-state/flow/` (shared across worktrees).
/// Outside git: `$CWD/.flow/` (regular directory).
pub fn resolve_shared_flow_dir(working_dir: &Path) -> PathBuf {
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

/// Create `.flow/` symlink pointing to the shared state directory.
///
/// In a git repo, creates `.git/flow-state/flow/` (real dir) and
/// `$CWD/.flow/` → `.git/flow-state/flow/` (symlink).
/// Outside git, creates `$CWD/.flow/` as a regular directory.
/// Idempotent: no-op if already correctly linked or is a regular dir.
pub fn ensure_flow_symlink(working_dir: &Path) -> Result<PathBuf, String> {
    let shared_dir = resolve_shared_flow_dir(working_dir);
    let local_link = working_dir.join(FLOW_DIR);

    // If shared == local (non-git fallback), just create the dir
    if shared_dir == local_link {
        std::fs::create_dir_all(&shared_dir)
            .map_err(|e| format!("failed to create {}: {e}", shared_dir.display()))?;
        return Ok(shared_dir);
    }

    // Create the real shared directory
    std::fs::create_dir_all(&shared_dir)
        .map_err(|e| format!("failed to create {}: {e}", shared_dir.display()))?;

    // Handle existing .flow/
    if local_link.exists() || local_link.symlink_metadata().is_ok() {
        if local_link.is_symlink() {
            // Already a symlink — check if it points to the right place
            if let Ok(target) = std::fs::read_link(&local_link) {
                let target_canonical = std::fs::canonicalize(&target)
                    .or_else(|_| std::fs::canonicalize(working_dir.join(&target)))
                    .unwrap_or(target);
                let shared_canonical = std::fs::canonicalize(&shared_dir)
                    .unwrap_or_else(|_| shared_dir.clone());
                if target_canonical == shared_canonical {
                    return Ok(shared_dir); // Already correct
                }
            }
            // Wrong target — remove and re-create
            std::fs::remove_file(&local_link)
                .map_err(|e| format!("failed to remove stale symlink: {e}"))?;
        } else if local_link.is_dir() {
            // Existing real .flow/ dir — migrate contents to shared, then replace with symlink
            migrate_dir_contents(&local_link, &shared_dir)?;
            std::fs::remove_dir_all(&local_link)
                .map_err(|e| format!("failed to remove old .flow/: {e}"))?;
        } else {
            return Err(format!(".flow exists but is not a dir or symlink: {}", local_link.display()));
        }
    }

    // Create symlink
    #[cfg(unix)]
    std::os::unix::fs::symlink(&shared_dir, &local_link)
        .map_err(|e| format!("failed to create symlink: {e}"))?;

    #[cfg(not(unix))]
    {
        // Windows fallback: just use the shared dir directly, no symlink
        std::fs::create_dir_all(&local_link)
            .map_err(|e| format!("failed to create {}: {e}", local_link.display()))?;
    }

    Ok(shared_dir)
}

/// Move contents from src dir to dst dir (non-recursive, files + dirs).
fn migrate_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(src)
        .map_err(|e| format!("failed to read {}: {e}", src.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("read_dir entry: {e}"))?;
        let dest = dst.join(entry.file_name());
        if !dest.exists() {
            std::fs::rename(entry.path(), &dest)
                .or_else(|_| {
                    // rename may fail across filesystems; fall back to copy
                    if entry.path().is_dir() {
                        copy_dir_recursive(&entry.path(), &dest)
                    } else {
                        std::fs::copy(&entry.path(), &dest).map(|_| ())
                    }
                })
                .map_err(|e| format!("migrate {}: {e}", entry.file_name().to_string_lossy()))?;
        }
    }
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
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
