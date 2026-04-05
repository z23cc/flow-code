//! Config, state-path, and migrate-state commands.

use std::env;
use std::fs;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output, stub};

use flowctl_core::types::CONFIG_FILE;

use super::{deep_merge, get_default_config, get_flow_dir, write_json_file};

// ── State-path command ─────────────────────────────────────────────

pub fn cmd_state_path(json_mode: bool, task: Option<String>) {
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let state_dir = match crate::commands::db_shim::resolve_state_dir(&cwd) {
        Ok(d) => d,
        Err(e) => {
            error_exit(&format!("Could not resolve state dir: {}", e));
        }
    };

    if let Some(task_id) = task {
        if !flowctl_core::id::is_task_id(&task_id) {
            error_exit(&format!(
                "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
                task_id
            ));
        }
        let state_path = state_dir.join("tasks").join(format!("{}.state.json", task_id));
        if json_mode {
            json_output(json!({
                "state_dir": state_dir.to_string_lossy(),
                "task_state_path": state_path.to_string_lossy(),
            }));
        } else {
            println!("{}", state_path.display());
        }
    } else if json_mode {
        json_output(json!({"state_dir": state_dir.to_string_lossy()}));
    } else {
        println!("{}", state_dir.display());
    }
}

// ── Migrate-state command (stub - complex migration logic) ─────────

pub fn cmd_migrate_state(json: bool, clean: bool) {
    let _ = clean;
    stub("migrate-state", json);
}

// ── Config commands ────────────────────────────────────────────────

/// Config subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Get a config value.
    Get {
        /// Config key (e.g., memory.enabled).
        key: String,
    },
    /// Set a config value.
    Set {
        /// Config key.
        key: String,
        /// Config value.
        value: String,
    },
}

pub fn cmd_config(cmd: &ConfigCmd, json: bool) {
    match cmd {
        ConfigCmd::Get { key } => cmd_config_get(json, key),
        ConfigCmd::Set { key, value } => cmd_config_set(json, key, value),
    }
}

fn cmd_config_get(json_mode: bool, key: &str) {
    let flow_dir = get_flow_dir();
    let config_path = flow_dir.join(CONFIG_FILE);

    // Load config with defaults
    let config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let raw = serde_json::from_str::<serde_json::Value>(&content)
                    .unwrap_or(json!({}));
                deep_merge(&get_default_config(), &raw)
            }
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    // Navigate nested key path
    let mut current = &config;
    for part in key.split('.') {
        match current.get(part) {
            Some(val) => current = val,
            None => {
                if json_mode {
                    json_output(json!({
                        "key": key,
                        "value": null,
                    }));
                } else {
                    println!("{}: (not set)", key);
                }
                return;
            }
        }
    }

    if json_mode {
        json_output(json!({
            "key": key,
            "value": current,
        }));
    } else {
        println!("{}: {}", key, current);
    }
}

fn cmd_config_set(json_mode: bool, key: &str, value: &str) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let config_path = flow_dir.join(CONFIG_FILE);

    // Load existing config
    let mut config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(json!({})),
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    // Parse value (handle type conversion)
    let parsed_value: serde_json::Value = match value.to_lowercase().as_str() {
        "true" => json!(true),
        "false" => json!(false),
        _ if value.parse::<i64>().is_ok() => json!(value.parse::<i64>().unwrap()),
        _ => json!(value),
    };

    // Navigate/create nested path
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &mut config;
    for part in &parts[..parts.len() - 1] {
        if !current.is_object() || !current.as_object().unwrap().contains_key(*part) {
            current[*part] = json!({});
        }
        current = &mut current[*part];
    }
    if let Some(last) = parts.last() {
        current[*last] = parsed_value.clone();
    }

    write_json_file(&config_path, &config);

    if json_mode {
        json_output(json!({
            "key": key,
            "value": parsed_value,
            "message": format!("Set {} = {}", key, parsed_value),
        }));
    } else {
        println!("Set {} = {}", key, parsed_value);
    }
}
