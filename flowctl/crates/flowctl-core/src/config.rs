//! Configuration helpers for reading `.flow/config.json` values.

use crate::types::{CONFIG_FILE, FLOW_DIR};

/// Read a boolean value from `.flow/config.json` using a dotted key path
/// (e.g. `"memory.enabled"`). Returns `default` if the file is missing,
/// unreadable, or the key is absent.
pub fn read_config_bool(key: &str, default: bool) -> bool {
    let cfg_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(FLOW_DIR)
        .join(CONFIG_FILE);
    if !cfg_path.exists() {
        return default;
    }
    let content = match std::fs::read_to_string(&cfg_path) {
        Ok(c) => c,
        Err(_) => return default,
    };
    let cfg: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    let parts: Vec<&str> = key.split('.').collect();
    let mut current = &cfg;
    for part in &parts {
        match current.get(part) {
            Some(v) => current = v,
            None => return default,
        }
    }
    current.as_bool().unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_config_bool_missing_file() {
        // No .flow/config.json in test env — should return default
        assert!(read_config_bool("outputs.enabled", true));
        assert!(!read_config_bool("memory.enabled", false));
    }
}
