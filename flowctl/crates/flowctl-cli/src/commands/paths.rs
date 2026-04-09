//! `flowctl paths` — show all resolved state directory paths.

use flowctl_core::FlowPaths;
use serde_json::json;

use crate::output::{error_exit, json_output};

/// Show all resolved state directory paths.
pub fn cmd_paths(json: bool) {
    match FlowPaths::resolve() {
        Some(paths) => {
            if json {
                json_output(json!({
                    "project_root": paths.project_root,
                    "slug": paths.slug,
                    "runtime_dir": paths.runtime_dir,
                    "config_dir": paths.config_dir,
                    "global_project_dir": paths.global_project_dir,
                    "config_json": paths.config_json(),
                    "project_context": paths.project_context(),
                    "invariants": paths.invariants(),
                    "frecency": paths.frecency(),
                    "memory_dir": paths.memory_dir(),
                }));
            } else {
                println!(
                    "Project: {} ({})",
                    paths.slug,
                    paths.project_root.display()
                );
                println!("Runtime:  {}", paths.runtime_dir.display());
                println!("Config:   {}", paths.config_dir.display());
                println!("Global:   {}", paths.global_project_dir.display());
            }
        }
        None => error_exit("Cannot resolve project paths. Run flowctl init first."),
    }
}
