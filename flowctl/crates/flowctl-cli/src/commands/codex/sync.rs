//! Codex sync command — generates Codex artifacts from agent `.md` files.

use std::path::Path;

use serde_json::json;

use flowctl_core::codex_sync::sync_all;

use crate::output::{error_exit, json_output};

pub fn cmd_sync(
    json_mode: bool,
    agents_dir: &str,
    output_dir: &str,
    hooks: &str,
    dry_run: bool,
    verbose: bool,
) {
    let agents_path = Path::new(agents_dir);
    let output_path = Path::new(output_dir);
    let hooks_path = Path::new(hooks);

    let hooks_arg = if hooks_path.exists() {
        Some(hooks_path)
    } else {
        None
    };

    let summary = match sync_all(agents_path, hooks_arg, output_path, dry_run) {
        Ok(s) => s,
        Err(e) => {
            error_exit(&format!("codex sync failed: {e}"));
        }
    };

    if json_mode {
        json_output(json!({
            "agents_generated": summary.agents_generated,
            "agents_skipped": summary.agents_skipped,
            "hooks_generated": summary.hooks_generated,
            "errors": summary.errors,
            "dry_run": dry_run,
        }));
    } else {
        if dry_run {
            println!("[dry-run] Would generate:");
        }
        println!(
            "Agents: {} generated, {} skipped",
            summary.agents_generated, summary.agents_skipped
        );
        if summary.hooks_generated {
            println!("Hooks: patched");
        }
        if !summary.errors.is_empty() {
            eprintln!("Errors:");
            for e in &summary.errors {
                eprintln!("  - {e}");
            }
        }
        if verbose {
            println!("Output directory: {}", output_path.display());
        }
    }
}
