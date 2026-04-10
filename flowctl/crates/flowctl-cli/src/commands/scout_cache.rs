//! Scout cache commands: `flowctl scout-cache get|set|clear`.
//!
//! File-based scout cache stored in `.state/scout-cache/` directory.

use clap::Subcommand;
use serde_json::json;

use super::helpers::get_flow_dir;
use crate::output::{error_exit, json_output};

#[derive(Subcommand, Debug)]
pub enum ScoutCacheCmd {
    /// Get a cached scout result.
    Get {
        /// Scout type (e.g., repo, capability).
        #[arg(long, alias = "type")]
        scout_type: String,
        /// Git commit hash. Auto-detected from HEAD if omitted.
        #[arg(long)]
        commit: Option<String>,
    },
    /// Set (cache) a scout result.
    Set {
        /// Scout type (e.g., repo, capability).
        #[arg(long, alias = "type")]
        scout_type: String,
        /// Git commit hash. Auto-detected from HEAD if omitted.
        #[arg(long)]
        commit: Option<String>,
        /// Result JSON string, or `@path/to/file.json` to read from disk.
        #[arg(long)]
        result: String,
    },
    /// Clear all cached scout results.
    Clear,
}

/// Auto-detect git HEAD commit hash. Returns "no-git" if not in a git repo.
fn detect_commit(explicit: &Option<String>) -> String {
    if let Some(c) = explicit {
        return c.clone();
    }
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "no-git".to_string())
}

/// Get the cache directory, creating it if needed.
fn cache_dir() -> std::path::PathBuf {
    let dir = get_flow_dir().join(".state").join("scout-cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Sanitize a cache key for use as a filename.
fn key_to_filename(key: &str) -> String {
    key.replace([':', '/', '\\'], "_")
}

pub fn dispatch(cmd: &ScoutCacheCmd, json_mode: bool) {
    match cmd {
        ScoutCacheCmd::Get { scout_type, commit } => {
            let c = detect_commit(commit);
            let key = format!("{scout_type}:{c}");
            let path = cache_dir().join(key_to_filename(&key));

            if path.exists() {
                let result = std::fs::read_to_string(&path).unwrap_or_default();
                if json_mode {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result));
                    json_output(json!({"hit": true, "key": key, "result": parsed}));
                } else {
                    println!("hit: {}", result);
                }
            } else if json_mode {
                json_output(json!({"hit": false, "key": key}));
            } else {
                println!("miss");
            }
        }
        ScoutCacheCmd::Set {
            scout_type,
            commit,
            result,
        } => {
            let c = detect_commit(commit);
            let key = format!("{scout_type}:{c}");

            let result_data = if let Some(path) = result.strip_prefix('@') {
                std::fs::read_to_string(path)
                    .unwrap_or_else(|e| error_exit(&format!("Cannot read result file: {e}")))
            } else {
                result.to_string()
            };

            let path = cache_dir().join(key_to_filename(&key));
            std::fs::write(&path, &result_data)
                .unwrap_or_else(|e| error_exit(&format!("Failed to cache: {e}")));

            if json_mode {
                json_output(json!({"ok": true, "key": key}));
            } else {
                println!("cached: {}", key);
            }
        }
        ScoutCacheCmd::Clear => {
            let dir = cache_dir();
            let mut n = 0u64;
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if std::fs::remove_file(entry.path()).is_ok() {
                        n += 1;
                    }
                }
            }

            if json_mode {
                json_output(json!({"ok": true, "cleared": n}));
            } else {
                println!("cleared {} cached entries", n);
            }
        }
    }
}
