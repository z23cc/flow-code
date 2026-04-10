//! Shared helpers used across multiple command modules.

use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use flowctl_core::graph_store::CodeGraph;
use flowctl_core::ngram_index::NgramIndex;
use flowctl_core::types::FLOW_DIR;
use serde_json::Value;

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

// ── JSON payload input infrastructure (Agent-Primary input model) ────

/// Parse a JSON payload from --input-json value.
///
/// Accepts:
/// - Inline JSON string: `'{"title": "..."}'`
/// - File reference: `@path/to/file.json`
/// - Stdin: `-`
///
/// Returns the parsed `serde_json::Value` (must be an object).
/// Calls `error_exit()` with agent-friendly messages on failure.
pub fn parse_input_json(raw: &str) -> Value {
    use crate::output::error_exit;

    let content = if raw == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| error_exit(&format!("Failed to read stdin: {e}")));
        if buf.trim().is_empty() {
            error_exit("--input-json stdin (-) was empty. Provide a JSON object.");
        }
        buf
    } else if let Some(path) = raw.strip_prefix('@') {
        std::fs::read_to_string(path)
            .unwrap_or_else(|e| error_exit(&format!("Cannot read file '{path}': {e}")))
    } else {
        raw.to_string()
    };

    // Detect double-encoded JSON (agent-specific failure mode)
    let trimmed = content.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(inner) = serde_json::from_str::<String>(trimmed) {
            if inner.starts_with('{') {
                error_exit(
                    "Double-encoded JSON detected: the input is a JSON string containing \
                     JSON. Pass the inner object directly, not wrapped in quotes.",
                );
            }
        }
    }

    let val: Value = serde_json::from_str(trimmed).unwrap_or_else(|e| {
        error_exit(&format!("Invalid JSON in --input-json: {e}"));
    });

    if !val.is_object() {
        error_exit("--input-json must be a JSON object ({{...}}), not an array or scalar.");
    }

    val
}

/// Validate that a JSON object only contains expected fields.
/// Rejects unknown fields with fuzzy suggestions.
///
/// `known_fields`: list of valid field names for this command.
/// `required_fields`: subset of known_fields that must be present.
pub fn validate_json_fields(
    val: &Value,
    known_fields: &[&str],
    required_fields: &[&str],
) {
    use crate::output::error_exit;

    let obj = val.as_object().unwrap_or_else(|| {
        error_exit("--input-json must be a JSON object");
    });

    // Check for unknown fields
    for key in obj.keys() {
        if !known_fields.contains(&key.as_str()) {
            let suggestion = find_closest_match(key, known_fields);
            let hint = match suggestion {
                Some(s) => format!(", did you mean '{s}'?"),
                None => String::new(),
            };
            error_exit(&format!(
                "Unknown field '{key}' in --input-json{hint}. Valid fields: {}",
                known_fields.join(", ")
            ));
        }
    }

    // Check for required fields
    for &field in required_fields {
        if !obj.contains_key(field) {
            error_exit(&format!("Missing required field '{field}' in --input-json"));
        }
    }
}

/// Find the closest match for a string in a list (simple edit-distance).
fn find_closest_match<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase();
    candidates
        .iter()
        .filter_map(|&candidate| {
            let dist = edit_distance(&input_lower, &candidate.to_lowercase());
            if dist <= 2 {
                Some((candidate, dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

/// Simple Levenshtein edit distance for fuzzy matching.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Extract a string field from a JSON object, or return None.
pub fn json_str(val: &Value, key: &str) -> Option<String> {
    val.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

/// Extract a string array field from a JSON object (supports both ["a","b"] and "a,b" formats).
pub fn json_str_vec(val: &Value, key: &str) -> Option<Vec<String>> {
    match val.get(key) {
        Some(Value::Array(arr)) => {
            Some(arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        }
        Some(Value::String(s)) => Some(s.split(',').map(|s| s.trim().to_string()).collect()),
        _ => None,
    }
}

/// Extract an integer field from a JSON object.
pub fn json_i64(val: &Value, key: &str) -> Option<i64> {
    val.get(key).and_then(|v| v.as_i64())
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
    use serde_json::json;

    #[test]
    fn edit_distance_exact_match() {
        assert_eq!(edit_distance("title", "title"), 0);
    }

    #[test]
    fn edit_distance_one_char_typo() {
        assert_eq!(edit_distance("titl", "title"), 1);
    }

    #[test]
    fn edit_distance_distant_strings() {
        assert!(edit_distance("abcdef", "xyz") > 2);
    }

    #[test]
    fn find_closest_match_finds_typo() {
        let candidates = &["title", "branch", "domain"];
        assert_eq!(find_closest_match("titl", candidates), Some("title"));
        assert_eq!(find_closest_match("brach", candidates), Some("branch"));
    }

    #[test]
    fn find_closest_match_no_match() {
        let candidates = &["title", "branch"];
        assert_eq!(find_closest_match("zzzzz", candidates), None);
    }

    #[test]
    fn json_str_extracts_string() {
        let val = json!({"name": "test"});
        assert_eq!(json_str(&val, "name"), Some("test".to_string()));
        assert_eq!(json_str(&val, "missing"), None);
    }

    #[test]
    fn json_str_vec_from_array() {
        let val = json!({"deps": ["a", "b", "c"]});
        assert_eq!(
            json_str_vec(&val, "deps"),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn json_str_vec_from_csv_string() {
        let val = json!({"deps": "a,b,c"});
        assert_eq!(
            json_str_vec(&val, "deps"),
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn json_i64_extracts_number() {
        let val = json!({"priority": 5});
        assert_eq!(json_i64(&val, "priority"), Some(5));
        assert_eq!(json_i64(&val, "missing"), None);
    }

    #[test]
    fn parse_input_json_inline() {
        let val = parse_input_json(r#"{"title": "test"}"#);
        assert_eq!(val.get("title").unwrap().as_str().unwrap(), "test");
    }

    #[test]
    fn parse_input_json_from_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), r#"{"key": "val"}"#).unwrap();
        let path = format!("@{}", tmp.path().display());
        let val = parse_input_json(&path);
        assert_eq!(val.get("key").unwrap().as_str().unwrap(), "val");
    }

    #[test]
    fn validate_json_fields_accepts_valid() {
        let val = json!({"title": "x", "branch": "y"});
        // Should not panic
        validate_json_fields(&val, &["title", "branch"], &["title"]);
    }

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
