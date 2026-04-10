//! Task creation command.

use std::path::Path;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::changes::{Changes, Mutation};
use flowctl_core::id::{epic_id_from_task, is_epic_id, is_task_id};
use flowctl_core::json_store::TaskState;
use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, FLOW_DIR, Task};

use super::{
    create_task_spec, ensure_flow_exists, parse_domain, read_file_or_stdin, scan_max_task_id,
};

/// Pure compute: build a `Changes` for task creation.
/// Returns (task_id, dep_list, changes).
#[allow(clippy::too_many_arguments)]
fn compute_task_create(
    flow_dir: &Path,
    epic_id: &str,
    title: &str,
    deps: Option<&str>,
    acceptance_file: Option<&str>,
    priority: Option<i32>,
    domain: Option<&str>,
    files: Option<&str>,
) -> (String, Vec<String>, Changes) {
    if !is_epic_id(epic_id) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic_id
        ));
    }

    // Verify epic exists
    if flowctl_core::json_store::epic_read(flow_dir, epic_id).is_err() {
        error_exit(&format!("Epic {} not found", epic_id));
    }

    // Scan-based ID allocation
    let task_num = scan_max_task_id(flow_dir, epic_id) + 1;
    let task_id = format!("{}.{}", epic_id, task_num);

    // Check no collision
    if flowctl_core::json_store::task_read(flow_dir, &task_id).is_ok() {
        error_exit(&format!(
            "Refusing to overwrite existing task {}. Check for orphaned entries.",
            task_id
        ));
    }

    // Parse dependencies and auto-expand short IDs
    let dep_list: Vec<String> = match deps {
        Some(d) if !d.is_empty() => d
            .split(',')
            .map(|s| {
                let trimmed = s.trim().to_string();
                if trimmed.is_empty() {
                    return trimmed;
                }
                // Auto-expand short IDs (e.g., fn-42.1 → fn-42-full-slug.1)
                flowctl_core::id::expand_dep_id(&trimmed, &epic_id)
            })
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    // Validate deps
    for dep in &dep_list {
        if !is_task_id(dep) {
            error_exit(&format!(
                "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M\nHint: for this epic, use {}.N",
                dep, epic_id
            ));
        }
        if let Ok(dep_epic) = epic_id_from_task(dep) {
            if dep_epic != epic_id {
                error_exit(&format!(
                    "Dependency {} is not in epic {}.\nHint: use the full task ID format: {}.N",
                    dep, epic_id, epic_id
                ));
            }
        }
    }

    // Read acceptance from file if provided
    let acceptance = acceptance_file.map(read_file_or_stdin);

    // Parse files
    let file_list: Vec<String> = match files {
        Some(f) if !f.is_empty() => f
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    let domain_enum = domain.map(parse_domain).unwrap_or(Domain::General);
    let now = Utc::now();

    let task = Task {
        schema_version: 1,
        id: task_id.clone(),
        epic: epic_id.to_string(),
        title: title.to_string(),
        status: Status::Todo,
        priority: priority.map(|p| p as u32),
        domain: domain_enum,
        depends_on: dep_list.clone(),
        files: file_list,
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("{}/tasks/{}.md", FLOW_DIR, task_id)),
        created_at: now,
        updated_at: now,
    };

    let body = create_task_spec(&task_id, title, acceptance.as_deref());

    let changes = Changes::new()
        .with(Mutation::CreateTask { task })
        .with(Mutation::SetTaskSpec {
            task_id: task_id.clone(),
            content: body,
        })
        .with(Mutation::SetTaskState {
            task_id: task_id.clone(),
            state: TaskState::default(),
        });

    (task_id, dep_list, changes)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cmd_task_create(
    json_mode: bool,
    epic_id: &str,
    title: &str,
    deps: Option<&str>,
    acceptance_file: Option<&str>,
    priority: Option<i32>,
    domain: Option<&str>,
    files: Option<&str>,
    dry_run: bool,
) {
    let flow_dir = ensure_flow_exists();

    let (task_id, dep_list, changes) = compute_task_create(
        &flow_dir,
        epic_id,
        title,
        deps,
        acceptance_file,
        priority,
        domain,
        files,
    );

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    let spec_path_str = format!("{}/tasks/{}.md", FLOW_DIR, task_id);
    if json_mode {
        json_output(json!({
            "id": task_id,
            "epic": epic_id,
            "title": title,
            "depends_on": dep_list,
            "spec_path": spec_path_str,
            "message": format!("Task {} created", task_id),
        }));
    } else {
        println!("Task {} created: {}", task_id, title);
    }
}
