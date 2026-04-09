//! `flowctl edit` — Smart code edit: exact match first, fuzzy fallback.
//!
//! Tries exact string replacement first; if the old text is not found verbatim,
//! falls back to the fuzzy replace engine from `flowctl_core::patch`.

use serde_json::json;

use crate::output::{error_exit, json_output};

/// Run smart code edit on a single file.
pub fn cmd_edit(json: bool, file: &str, old: &str, new: &str) {
    let content = std::fs::read_to_string(file).unwrap_or_else(|e| {
        error_exit(&format!("Cannot read '{}': {e}", file));
    });

    // Strategy 1: Exact match (first occurrence only).
    if content.contains(old) {
        let result = content.replacen(old, new, 1);
        let bytes = result.len();
        std::fs::write(file, &result).unwrap_or_else(|e| {
            error_exit(&format!("Cannot write '{}': {e}", file));
        });
        if json {
            json_output(json!({
                "file": file,
                "method": "exact",
                "bytes_written": bytes,
            }));
        } else {
            eprintln!("Edited {} (exact, {} bytes)", file, bytes);
        }
        return;
    }

    // Strategy 2: Fuzzy patch fallback.
    match flowctl_core::patch::fuzzy_replace(&content, old, new) {
        Ok(result) => {
            let bytes = result.len();
            std::fs::write(file, &result).unwrap_or_else(|e| {
                error_exit(&format!("Cannot write '{}': {e}", file));
            });
            if json {
                json_output(json!({
                    "file": file,
                    "method": "fuzzy",
                    "bytes_written": bytes,
                }));
            } else {
                eprintln!("Edited {} (fuzzy, {} bytes)", file, bytes);
            }
        }
        Err(e) => {
            error_exit(&format!(
                "Could not find text to replace in '{}' (exact and fuzzy both failed): {e}",
                file
            ));
        }
    }
}
