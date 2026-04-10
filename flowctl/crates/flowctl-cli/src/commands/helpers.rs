//! Shared helpers used across multiple command modules.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use flowctl_core::graph_store::CodeGraph;
use flowctl_core::ngram_index::NgramIndex;
use flowctl_core::types::FLOW_DIR;

/// Get the .flow/ directory path.
///
/// Resolution order:
/// 1. `FLOW_STATE_DIR` environment variable (explicit override)
/// 2. Walk up the directory tree looking for `.flow/` (like git finds `.git/`)
/// 3. Fallback to `$CWD/.flow/` (for `flowctl init` before `.flow/` exists)
pub fn get_flow_dir() -> PathBuf {
    // 1. Environment variable override (explicit)
    if let Ok(dir) = env::var("FLOW_STATE_DIR") {
        return PathBuf::from(dir);
    }

    // 2. Walk up directory tree looking for .flow (like git finds .git)
    if let Ok(mut current) = env::current_dir() {
        loop {
            let candidate = current.join(FLOW_DIR);
            if candidate.exists() {
                return candidate;
            }
            if !current.pop() {
                break;
            }
        }
    }

    // 3. Fallback to CWD/.flow (for init)
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
                let shared_canonical =
                    std::fs::canonicalize(&shared_dir).unwrap_or_else(|_| shared_dir.clone());
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
            return Err(format!(
                ".flow exists but is not a dir or symlink: {}",
                local_link.display()
            ));
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
    let entries =
        std::fs::read_dir(src).map_err(|e| format!("failed to read {}: {e}", src.display()))?;

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
                        std::fs::copy(entry.path(), &dest).map(|_| ())
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

#[derive(Debug, Default)]
pub struct SearchArtifactBootstrap {
    pub actions: Vec<String>,
    pub warnings: Vec<String>,
}

pub(crate) fn bootstrap_search_artifacts_from_root(
    root: &Path,
    flow_dir: &Path,
) -> SearchArtifactBootstrap {
    let mut result = SearchArtifactBootstrap::default();

    if let Err(e) = std::fs::create_dir_all(flow_dir.join("index")) {
        result
            .warnings
            .push(format!("failed to prepare index directory: {e}"));
        return result;
    }

    let graph_path = flow_dir.join("graph.bin");
    if !graph_path.exists() {
        match CodeGraph::build(root) {
            Ok(graph) => match graph.save(&graph_path) {
                Ok(()) => {
                    let stats = graph.stats();
                    result.actions.push(format!(
                        "built graph.bin ({} symbols, {} files)",
                        stats.symbol_count, stats.file_count
                    ));
                }
                Err(e) => result
                    .warnings
                    .push(format!("failed to save graph.bin: {e}")),
            },
            Err(e) => result
                .warnings
                .push(format!("failed to build graph.bin: {e}")),
        }
    }

    let index_path = flow_dir.join("index").join("ngram.bin");
    if !index_path.exists() {
        match NgramIndex::build(root) {
            Ok(index) => match index.save(&index_path) {
                Ok(()) => {
                    let stats = index.stats();
                    result.actions.push(format!(
                        "built ngram index ({} files, {} trigrams)",
                        stats.file_count, stats.trigram_count
                    ));
                }
                Err(e) => result
                    .warnings
                    .push(format!("failed to save ngram index: {e}")),
            },
            Err(e) => result
                .warnings
                .push(format!("failed to build ngram index: {e}")),
        }
    }

    result
}

/// Apply a `Changes` batch via the service-layer `ChangesApplier`.
///
/// Applies all mutations (JSON store writes + event logging) in order.
/// Returns the number of mutations applied. Calls `error_exit` on failure.
pub fn apply_changes(flow_dir: &Path, changes: &flowctl_core::changes::Changes) -> usize {
    use crate::output::error_exit;
    use flowctl_core::changes::ChangesApplier;

    if changes.is_empty() {
        return 0;
    }

    let actor = resolve_actor();

    let applier = ChangesApplier::new(flow_dir).with_actor(&actor);

    let result = applier
        .apply(changes)
        .unwrap_or_else(|e| error_exit(&format!("Failed to apply changes: {e}")));

    result.applied
}

/// Handle dry-run or real apply of a `Changes` batch.
///
/// When `dry_run` is true, prints the changes as a JSON preview and returns 0
/// without touching storage. Otherwise delegates to `apply_changes`.
pub fn maybe_apply_changes(
    flow_dir: &Path,
    changes: &flowctl_core::changes::Changes,
    dry_run: bool,
) -> usize {
    if dry_run {
        let preview = serde_json::json!({
            "dry_run": true,
            "changes": changes,
        });
        println!(
            "{}",
            serde_json::to_string(&preview)
                .expect("JSON serialization of dry-run preview should not fail")
        );
        return 0;
    }
    apply_changes(flow_dir, changes)
}

/// Resolve current actor: FLOW_ACTOR env > git config user.email > git config user.name > $USER > "unknown"
pub fn resolve_actor() -> String {
    if let Ok(actor) = env::var("FLOW_ACTOR") {
        let trimmed = actor.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }

    if let Ok(output) = Command::new("git").args(["config", "user.email"]).output() {
        if output.status.success() {
            let email = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !email.is_empty() {
                return email;
            }
        }
    }

    if let Ok(output) = Command::new("git").args(["config", "user.name"]).output() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_search_artifacts_fails_open_when_save_path_is_invalid() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("sample.rs"), "fn sample() {}\n").unwrap();

        let blocked = project.path().join("blocked");
        std::fs::write(&blocked, "not a directory").unwrap();
        let invalid_flow_dir = blocked.join(".flow");

        let result = bootstrap_search_artifacts_from_root(project.path(), &invalid_flow_dir);

        assert!(result.actions.is_empty());
        assert!(
            !result.warnings.is_empty(),
            "expected fail-open warnings when artifact save paths are unavailable"
        );
    }
}
