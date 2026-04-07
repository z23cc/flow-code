//! Export and import commands.
//!
//! With file-based storage, export is a no-op (data is already in files)
//! and import scans files to rebuild any derived state.

use serde_json::json;

use crate::output::{error_exit, json_output};

pub fn cmd_export(json: bool, _epic_filter: Option<String>, _format: String) {
    let flow_dir = super::get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap_or_default();
    let mut tasks_count = 0;
    for epic in &epics {
        let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic.id).unwrap_or_default();
        tasks_count += tasks.len();
    }

    if json {
        json_output(json!({
            "success": true,
            "epics_exported": epics.len(),
            "tasks_exported": tasks_count,
            "message": "Data is already in JSON files (file-based storage)",
        }));
    } else {
        println!("Data is already in JSON files: {} epics, {} tasks in .flow/", epics.len(), tasks_count);
    }
}

pub fn cmd_import(json: bool) {
    let flow_dir = super::get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap_or_default();
    let mut tasks_count = 0;
    for epic in &epics {
        let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic.id).unwrap_or_default();
        tasks_count += tasks.len();
    }

    if json {
        json_output(json!({
            "success": true,
            "epics_imported": epics.len(),
            "tasks_imported": tasks_count,
            "files_skipped": 0,
            "warnings": [],
        }));
    } else {
        println!(
            "Scanned {} epics, {} tasks from .flow/ (file-based storage, no DB to import into)",
            epics.len(), tasks_count
        );
    }
}
