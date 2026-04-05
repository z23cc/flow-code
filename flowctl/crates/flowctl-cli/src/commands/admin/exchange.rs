//! Export and import commands.

use std::env;
use std::fs;

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::{EPICS_DIR, FLOW_DIR, TASKS_DIR};

pub fn cmd_export(json: bool, epic_filter: Option<String>, _format: String) {
    let flow_dir = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join(FLOW_DIR);
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let conn = crate::commands::db_shim::open(&cwd)
        .unwrap_or_else(|e| error_exit(&format!("Failed to open DB: {e}")));

    let epic_repo = crate::commands::db_shim::EpicRepo::new(&conn);
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);

    let epics_dir = flow_dir.join(EPICS_DIR);
    let _ = fs::create_dir_all(&epics_dir);
    let epics = match &epic_filter {
        Some(id) => match epic_repo.get(id) {
            Ok(e) => vec![e],
            Err(_) => { error_exit(&format!("Epic {} not found", id)); }
        },
        None => epic_repo.list(None).unwrap_or_default(),
    };

    let mut epics_exported = 0;
    for epic in &epics {
        let (_, body) = epic_repo.get_with_body(&epic.id).unwrap_or((epic.clone(), String::new()));
        let doc = flowctl_core::frontmatter::Document { frontmatter: epic.clone(), body };
        if let Ok(content) = flowctl_core::frontmatter::write(&doc) {
            let path = epics_dir.join(format!("{}.md", epic.id));
            let _ = fs::write(&path, content);
            epics_exported += 1;
        }
    }

    let tasks_dir = flow_dir.join(TASKS_DIR);
    let _ = fs::create_dir_all(&tasks_dir);
    let mut tasks_exported = 0;
    for epic in &epics {
        let tasks = task_repo.list_by_epic(&epic.id).unwrap_or_default();
        for task in &tasks {
            let (_, body) = task_repo.get_with_body(&task.id).unwrap_or((task.clone(), String::new()));
            let doc = flowctl_core::frontmatter::Document { frontmatter: task.clone(), body };
            if let Ok(content) = flowctl_core::frontmatter::write(&doc) {
                let path = tasks_dir.join(format!("{}.md", task.id));
                let _ = fs::write(&path, content);
                tasks_exported += 1;
            }
        }
    }

    if json {
        json_output(json!({
            "success": true,
            "epics_exported": epics_exported,
            "tasks_exported": tasks_exported,
        }));
    } else {
        println!("Exported {} epics, {} tasks to .flow/", epics_exported, tasks_exported);
    }
}

pub fn cmd_import(json: bool) {
    let flow_dir = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join(FLOW_DIR);
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let conn = crate::commands::db_shim::open(&cwd)
        .unwrap_or_else(|e| error_exit(&format!("Failed to open DB: {e}")));

    let state_dir = crate::commands::db_shim::resolve_state_dir(&cwd).ok();
    let result = crate::commands::db_shim::reindex(&conn, &flow_dir, state_dir.as_deref())
        .unwrap_or_else(|e| error_exit(&format!("Import failed: {e}")));

    if json {
        json_output(json!({
            "success": true,
            "epics_imported": result.epics_indexed,
            "tasks_imported": result.tasks_indexed,
            "files_skipped": result.files_skipped,
            "warnings": result.warnings,
        }));
    } else {
        println!(
            "Imported {} epics, {} tasks ({} skipped)",
            result.epics_indexed, result.tasks_indexed, result.files_skipped
        );
        for w in &result.warnings {
            eprintln!("  warning: {w}");
        }
    }
}
