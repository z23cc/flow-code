//! Scout cache commands: `flowctl scout-cache get|set|clear`.

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use super::db_shim;

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
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "no-git".to_string())
}

pub fn dispatch(cmd: &ScoutCacheCmd, json_mode: bool) {
    match cmd {
        ScoutCacheCmd::Get { scout_type, commit } => {
            let c = detect_commit(commit);
            let key = format!("{scout_type}:{c}");

            // Get DB connection outside async (require_db uses its own runtime).
            let conn = match db_shim::require_db() {
                Ok(c) => c,
                Err(_) => {
                    if json_mode {
                        json_output(json!({"hit": false, "key": key}));
                    } else {
                        println!("miss (db unavailable)");
                    }
                    return;
                }
            };

            let repo = flowctl_db::ScoutCacheRepo::new(conn.inner_conn());
            db_shim::block_on_pub(async {
                match repo.get(&key).await {
                    Ok(Some(result)) => {
                        if json_mode {
                            let parsed: serde_json::Value =
                                serde_json::from_str(&result)
                                    .unwrap_or(serde_json::Value::String(result));
                            json_output(json!({"hit": true, "key": key, "result": parsed}));
                        } else {
                            println!("hit: {}", result);
                        }
                    }
                    Ok(None) => {
                        if json_mode {
                            json_output(json!({"hit": false, "key": key}));
                        } else {
                            println!("miss");
                        }
                    }
                    Err(_) => {
                        if json_mode {
                            json_output(json!({"hit": false, "key": key}));
                        } else {
                            println!("miss (db error)");
                        }
                    }
                }
            });
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

            let conn = db_shim::require_db()
                .unwrap_or_else(|e| error_exit(&format!("DB unavailable: {e}")));

            let repo = flowctl_db::ScoutCacheRepo::new(conn.inner_conn());
            db_shim::block_on_pub(async {
                repo.set(&key, &c, scout_type, &result_data)
                    .await
                    .unwrap_or_else(|e| error_exit(&format!("Failed to cache: {e}")));
            });

            if json_mode {
                json_output(json!({"ok": true, "key": key}));
            } else {
                println!("cached: {}", key);
            }
        }
        ScoutCacheCmd::Clear => {
            let conn = db_shim::require_db()
                .unwrap_or_else(|e| error_exit(&format!("DB unavailable: {e}")));

            let repo = flowctl_db::ScoutCacheRepo::new(conn.inner_conn());
            let n = db_shim::block_on_pub(async {
                repo.clear()
                    .await
                    .unwrap_or_else(|e| error_exit(&format!("Failed to clear: {e}")))
            });

            if json_mode {
                json_output(json!({"ok": true, "cleared": n}));
            } else {
                println!("cleared {} cached entries", n);
            }
        }
    }
}
