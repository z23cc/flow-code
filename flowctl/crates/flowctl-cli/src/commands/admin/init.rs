//! Init and detect commands.

use std::fs;

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::{
    CONFIG_FILE, EPICS_DIR, MEMORY_DIR, META_FILE, REVIEWS_DIR, SCHEMA_VERSION,
    SPECS_DIR, SUPPORTED_SCHEMA_VERSIONS, TASKS_DIR,
};

use super::{deep_merge, get_default_config, get_flow_dir, write_json_file};

// ── Init command ────────────────────────────────────────────────────

pub fn cmd_init(json: bool) {
    let flow_dir = get_flow_dir();
    let mut actions: Vec<String> = Vec::new();

    // Create directories if missing (idempotent, never destroys existing)
    for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
        let dir_path = flow_dir.join(subdir);
        if !dir_path.exists() {
            if let Err(e) = fs::create_dir_all(&dir_path) {
                error_exit(&format!("Failed to create {}: {}", dir_path.display(), e));
            }
            actions.push(format!("created {}/", subdir));
        }
    }

    // Create meta.json if missing (never overwrite existing)
    let meta_path = flow_dir.join(META_FILE);
    if !meta_path.exists() {
        let meta = json!({
            "schema_version": SCHEMA_VERSION,
            "next_epic": 1
        });
        write_json_file(&meta_path, &meta);
        actions.push("created meta.json".to_string());
    }

    // Config: create or upgrade (merge missing defaults)
    let config_path = flow_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        write_json_file(&config_path, &get_default_config());
        actions.push("created config.json".to_string());
    } else {
        // Load raw config, compare with merged (which includes new defaults)
        let raw = match fs::read_to_string(&config_path) {
            Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
                .unwrap_or(json!({})),
            Err(_) => json!({}),
        };
        let merged = deep_merge(&get_default_config(), &raw);
        if merged != raw {
            write_json_file(&config_path, &merged);
            actions.push("upgraded config.json (added missing keys)".to_string());
        }
    }

    // Build output
    let message = if actions.is_empty() {
        ".flow/ already up to date".to_string()
    } else {
        format!(".flow/ updated: {}", actions.join(", "))
    };

    if json {
        json_output(json!({
            "message": message,
            "path": flow_dir.to_string_lossy(),
            "actions": actions,
        }));
    } else {
        println!("{}", message);
    }
}

// ── Detect command ──────────────────────────────────────────────────

pub fn cmd_detect(json: bool) {
    let flow_dir = get_flow_dir();
    let exists = flow_dir.exists();
    let mut issues: Vec<String> = Vec::new();

    if exists {
        let meta_path = flow_dir.join(META_FILE);
        if !meta_path.exists() {
            issues.push("meta.json missing".to_string());
        } else {
            match fs::read_to_string(&meta_path) {
                Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(meta) => {
                        let version = meta.get("schema_version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if !SUPPORTED_SCHEMA_VERSIONS.contains(&version) {
                            issues.push(format!(
                                "schema_version unsupported (supported {:?}, got {})",
                                SUPPORTED_SCHEMA_VERSIONS, version
                            ));
                        }
                    }
                    Err(e) => issues.push(format!("meta.json parse error: {}", e)),
                },
                Err(e) => issues.push(format!("meta.json unreadable: {}", e)),
            }
        }

        for subdir in &[EPICS_DIR, SPECS_DIR, TASKS_DIR, MEMORY_DIR, REVIEWS_DIR] {
            if !flow_dir.join(subdir).exists() {
                issues.push(format!("{}/ missing", subdir));
            }
        }
    }

    let valid = exists && issues.is_empty();

    if json {
        json_output(json!({
            "exists": exists,
            "valid": valid,
            "issues": issues,
            "path": flow_dir.to_string_lossy(),
        }));
    } else if exists {
        if valid {
            println!(".flow/ exists and is valid");
        } else {
            println!(".flow/ exists but has issues:");
            for issue in &issues {
                println!("  - {}", issue);
            }
        }
    } else {
        println!(".flow/ not found");
    }
}
