//! Admin commands: init, detect, status, doctor, validate, state-path,
//! review-backend, parse-findings, guard, worker-prompt, config.

use std::fs;
use std::path::Path;

use serde_json::json;

use crate::commands::helpers::get_flow_dir;
use crate::output::error_exit;

// ── Private helpers (admin-only) ───────────────────────────────────

/// Default config structure matching Python's get_default_config().
fn get_default_config() -> serde_json::Value {
    json!({
        "memory": {"enabled": true},
        "outputs": {"enabled": true},
        "planSync": {"enabled": true, "crossEpic": true},
        "review": {"backend": "rp"},
        "scouts": {"backend": "auto", "github": true},
        "stack": {},
    })
}

/// Deep merge: override values win for conflicts.
fn deep_merge(base: &serde_json::Value, over: &serde_json::Value) -> serde_json::Value {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            let mut result = b.clone();
            for (key, value) in o {
                if let Some(base_val) = result.get(key) {
                    result.insert(key.clone(), deep_merge(base_val, value));
                } else {
                    result.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        (_, over_val) => over_val.clone(),
    }
}

/// Write JSON to a file with pretty formatting.
fn write_json_file(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = serde_json::to_string_pretty(value).unwrap();
    if let Err(e) = fs::write(path, &content) {
        error_exit(&format!("Failed to write {}: {}", path.display(), e));
    }
}

// ── Submodules ─────────────────────────────────────────────────────

mod config;
mod exchange;
mod guard;
mod init;
mod review;
mod status;

// ── Re-exports (preserves public API) ──────────────────────────────

pub use config::{ConfigCmd, cmd_config, cmd_state_path};
pub use exchange::{cmd_export, cmd_import};
pub use guard::{cmd_guard, cmd_worker_prompt};
pub use init::{cmd_detect, cmd_init, cmd_startup};
pub use review::{ReviewCmd, cmd_parse_findings, cmd_review_backend, dispatch_review};
pub use status::{cmd_doctor, cmd_progress, cmd_status, cmd_validate};
