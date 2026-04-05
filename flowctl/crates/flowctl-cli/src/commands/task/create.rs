//! Task creation command.

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::{epic_id_from_task, is_epic_id, is_task_id};
use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, Task, EPICS_DIR, FLOW_DIR, TASKS_DIR};

use super::{
    create_task_spec, ensure_flow_exists, parse_domain, read_file_or_stdin, scan_max_task_id,
    try_open_db, write_task_doc,
};

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
) {
    let flow_dir = ensure_flow_exists();

    if !is_epic_id(epic_id) {
        error_exit(&format!(
            "Invalid epic ID: {}. Expected format: fn-N or fn-N-slug (e.g., fn-1, fn-1-add-auth)",
            epic_id
        ));
    }

    // Verify epic exists
    let epic_spec = flow_dir.join(EPICS_DIR).join(format!("{}.md", epic_id));
    if !epic_spec.exists() {
        error_exit(&format!("Epic {} not found", epic_id));
    }

    // Scan-based ID allocation
    let task_num = scan_max_task_id(&flow_dir, epic_id) + 1;
    let task_id = format!("{}.{}", epic_id, task_num);

    // Check no collision
    let spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", task_id));
    if spec_path.exists() {
        error_exit(&format!(
            "Refusing to overwrite existing task {}. Check for orphaned files.",
            task_id
        ));
    }

    // Parse dependencies
    let dep_list: Vec<String> = match deps {
        Some(d) if !d.is_empty() => d
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec![],
    };

    // Validate deps
    for dep in &dep_list {
        if !is_task_id(dep) {
            error_exit(&format!(
                "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
                dep
            ));
        }
        if let Ok(dep_epic) = epic_id_from_task(dep) {
            if dep_epic != epic_id {
                error_exit(&format!(
                    "Dependency {} must be within the same epic ({})",
                    dep, epic_id
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

    // Create Task struct
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
        file_path: Some(format!("{}/{}/{}.md", FLOW_DIR, TASKS_DIR, task_id)),
        created_at: now,
        updated_at: now,
    };

    // Create spec markdown body
    let body = create_task_spec(&task_id, title, acceptance.as_deref());

    // Write Markdown file with frontmatter
    let doc = flowctl_core::frontmatter::Document {
        frontmatter: task.clone(),
        body,
    };
    write_task_doc(&flow_dir, &task_id, &doc);

    // Upsert into SQLite if DB available
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        let _ = repo.upsert(&task);
    }

    let spec_path_str = format!("{}/{}/{}.md", FLOW_DIR, TASKS_DIR, task_id);
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
