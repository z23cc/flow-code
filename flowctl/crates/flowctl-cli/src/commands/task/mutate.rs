//! Task mutation commands: reset, skip, split, set-deps.

use std::path::Path;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::changes::{Changes, Mutation};
use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::json_store::TaskState;
use flowctl_core::state_machine::Status;
use flowctl_core::types::{FLOW_DIR, Task};

use super::{
    clear_evidence_in_body, create_task_spec, ensure_flow_exists, find_dependents, load_epic_md,
    load_task_doc, scan_max_task_id,
};

/// Pure compute: build Changes for task reset (+ optional cascade).
/// Returns (list of reset task IDs, changes). Returns None if already todo.
fn compute_task_reset(
    flow_dir: &Path,
    task_id: &str,
    cascade: bool,
) -> Option<(Vec<String>, Changes)> {
    let mut doc = load_task_doc(flow_dir, task_id);
    let current_status = doc.frontmatter.status;

    // Check if epic is closed
    if let Ok(eid) = epic_id_from_task(task_id) {
        if let Some(epic) = load_epic_md(flow_dir, &eid) {
            if epic.status == flowctl_core::types::EpicStatus::Done {
                error_exit(&format!("Cannot reset task in closed epic {}", eid));
            }
        }
    }

    if current_status == Status::InProgress {
        error_exit(&format!(
            "Cannot reset in_progress task {}. Complete or block it first.",
            task_id
        ));
    }

    if current_status == Status::Todo {
        return None; // already todo
    }

    // Build changes for the primary task
    doc.frontmatter.status = Status::Todo;
    doc.frontmatter.updated_at = Utc::now();
    let cleared_body = clear_evidence_in_body(&doc.body);

    let mut changes = Changes::new()
        .with(Mutation::UpdateTask {
            task: doc.frontmatter.clone(),
        })
        .with(Mutation::SetTaskSpec {
            task_id: task_id.to_string(),
            content: cleared_body,
        })
        .with(Mutation::SetTaskState {
            task_id: task_id.to_string(),
            state: TaskState::default(),
        });

    let mut reset_ids = vec![task_id.to_string()];

    // Handle cascade
    if cascade {
        let dependents = find_dependents(flow_dir, task_id);
        for dep_id in &dependents {
            let mut dep_doc = load_task_doc(flow_dir, dep_id);
            let dep_status = dep_doc.frontmatter.status;
            if dep_status == Status::InProgress || dep_status == Status::Todo {
                continue;
            }

            dep_doc.frontmatter.status = Status::Todo;
            dep_doc.frontmatter.updated_at = Utc::now();
            let dep_cleared_body = clear_evidence_in_body(&dep_doc.body);

            changes.push(Mutation::UpdateTask {
                task: dep_doc.frontmatter,
            });
            changes.push(Mutation::SetTaskSpec {
                task_id: dep_id.clone(),
                content: dep_cleared_body,
            });
            changes.push(Mutation::SetTaskState {
                task_id: dep_id.clone(),
                state: TaskState::default(),
            });
            reset_ids.push(dep_id.clone());
        }
    }

    Some((reset_ids, changes))
}

pub(super) fn cmd_task_reset(json_mode: bool, task_id: &str, cascade: bool, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    match compute_task_reset(&flow_dir, task_id, cascade) {
        None => {
            // Already todo
            if json_mode {
                json_output(json!({
                    "reset": [],
                    "message": format!("{} already todo", task_id),
                }));
            } else {
                println!("{} already todo", task_id);
            }
        }
        Some((reset_ids, changes)) => {
            crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
            if dry_run {
                return;
            }

            if json_mode {
                json_output(json!({
                    "reset": reset_ids,
                }));
            } else {
                println!("Reset: {}", reset_ids.join(", "));
            }
        }
    }
}

/// Pure compute: build Changes for task skip.
fn compute_task_skip(flow_dir: &Path, task_id: &str) -> Changes {
    let mut doc = load_task_doc(flow_dir, task_id);

    if doc.frontmatter.status == Status::Done {
        error_exit(&format!("Cannot skip already-done task {}", task_id));
    }

    doc.frontmatter.status = Status::Skipped;
    doc.frontmatter.updated_at = Utc::now();

    Changes::new().with(Mutation::UpdateTask {
        task: doc.frontmatter,
    })
}

pub(super) fn cmd_task_skip(json_mode: bool, task_id: &str, reason: Option<&str>, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }

    let changes = compute_task_skip(&flow_dir, task_id);

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    let reason_str = reason.unwrap_or("");
    if json_mode {
        json_output(json!({
            "id": task_id,
            "status": "skipped",
            "reason": reason_str,
            "message": format!("Task {} skipped", task_id),
        }));
    } else {
        let suffix = if !reason_str.is_empty() {
            format!(": {}", reason_str)
        } else {
            String::new()
        };
        println!("Task {} skipped{}", task_id, suffix);
    }
}

/// Pure compute: build Changes for task split.
/// Returns (list of created sub-task IDs, changes).
fn compute_task_split(
    flow_dir: &Path,
    task_id: &str,
    titles: &str,
    chain: bool,
) -> (Vec<String>, Changes) {
    let doc = load_task_doc(flow_dir, task_id);
    let status = doc.frontmatter.status;

    if status == Status::Done || status == Status::Skipped {
        error_exit(&format!(
            "Cannot split task {} with status '{}'",
            task_id, status
        ));
    }

    let epic_id = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Cannot extract epic from {}", task_id)));

    let title_list: Vec<String> = titles
        .split('|')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();

    if title_list.len() < 2 {
        error_exit("Need at least 2 sub-task titles separated by '|'");
    }

    let max_task = scan_max_task_id(flow_dir, &epic_id);
    let original_deps = doc.frontmatter.depends_on.clone();
    let mut created: Vec<String> = Vec::new();
    let now = Utc::now();
    let mut changes = Changes::new();

    for (i, sub_title) in title_list.iter().enumerate() {
        let sub_num = max_task + 1 + i as u32;
        let sub_id = format!("{}.{}", epic_id, sub_num);

        let sub_deps = if i == 0 {
            original_deps.clone()
        } else if chain {
            let prev_id = format!("{}.{}", epic_id, max_task + i as u32);
            vec![prev_id]
        } else {
            vec![]
        };

        let sub_task = Task {
            schema_version: 1,
            id: sub_id.clone(),
            epic: epic_id.clone(),
            title: sub_title.clone(),
            status: Status::Todo,
            priority: doc.frontmatter.priority,
            domain: doc.frontmatter.domain,
            depends_on: sub_deps,
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some(format!("{}/tasks/{}.md", FLOW_DIR, sub_id)),
            created_at: now,
            updated_at: now,
        };

        let body = create_task_spec(&sub_id, sub_title, None);

        changes.push(Mutation::CreateTask { task: sub_task });
        changes.push(Mutation::SetTaskSpec {
            task_id: sub_id.clone(),
            content: body,
        });

        created.push(sub_id);
    }

    // Mark original task as skipped
    let mut orig_task = doc.frontmatter;
    orig_task.status = Status::Skipped;
    orig_task.updated_at = now;
    changes.push(Mutation::UpdateTask { task: orig_task });

    // Update tasks that depended on original to depend on last sub-task
    let last_sub = created.last().unwrap().clone();
    if let Ok(all_tasks) = flowctl_core::json_store::task_list_by_epic(flow_dir, &epic_id) {
        for other_task in all_tasks {
            let other_id = &other_task.id;
            if other_id == task_id || created.contains(other_id) {
                continue;
            }
            if other_task.depends_on.contains(&task_id.to_string()) {
                let mut updated_task = other_task.clone();
                updated_task.depends_on = updated_task
                    .depends_on
                    .iter()
                    .map(|d| {
                        if d == task_id {
                            last_sub.clone()
                        } else {
                            d.clone()
                        }
                    })
                    .collect();
                updated_task.updated_at = now;
                changes.push(Mutation::UpdateTask { task: updated_task });
            }
        }
    }

    (created, changes)
}

pub(super) fn cmd_task_split(
    json_mode: bool,
    task_id: &str,
    titles: &str,
    chain: bool,
    dry_run: bool,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!("Invalid task ID: {}", task_id));
    }

    let (created, changes) = compute_task_split(&flow_dir, task_id, titles, chain);

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    let last_sub = created.last().unwrap().clone();
    if json_mode {
        json_output(json!({
            "original": task_id,
            "split_into": created,
            "chain": chain,
            "message": format!("Task {} split into {} sub-tasks", task_id, created.len()),
        }));
    } else {
        println!("Task {} split into:", task_id);
        for sub_id in &created {
            println!("  {}", sub_id);
        }
        println!(
            "Original task marked as skipped. Downstream deps updated to {}.",
            last_sub
        );
    }
}

/// Pure compute: build Changes for setting task dependencies.
fn compute_task_set_deps(
    flow_dir: &Path,
    task_id: &str,
    deps: &str,
) -> (Vec<String>, Vec<String>, Changes) {
    let dep_ids: Vec<String> = deps
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if dep_ids.is_empty() {
        error_exit("--deps cannot be empty");
    }

    let task_epic = epic_id_from_task(task_id)
        .unwrap_or_else(|_| error_exit(&format!("Invalid task ID: {}", task_id)));

    for dep_id in &dep_ids {
        if !is_task_id(dep_id) {
            error_exit(&format!(
                "Invalid dependency ID: {}. Expected format: fn-N.M or fn-N-slug.M",
                dep_id
            ));
        }
        if let Ok(dep_epic) = epic_id_from_task(dep_id) {
            if dep_epic != task_epic {
                error_exit(&format!(
                    "Dependencies must be within same epic. Task {} is in {}, dependency {} is in {}",
                    task_id, task_epic, dep_id, dep_epic
                ));
            }
        }
    }

    let mut doc = load_task_doc(flow_dir, task_id);

    let mut added = Vec::new();
    for dep_id in &dep_ids {
        if !doc.frontmatter.depends_on.contains(dep_id) {
            doc.frontmatter.depends_on.push(dep_id.clone());
            added.push(dep_id.clone());
        }
    }

    let changes = if !added.is_empty() {
        doc.frontmatter.updated_at = Utc::now();
        Changes::new().with(Mutation::UpdateTask {
            task: doc.frontmatter.clone(),
        })
    } else {
        Changes::new()
    };

    (doc.frontmatter.depends_on.clone(), added, changes)
}

pub(super) fn cmd_task_set_deps(json_mode: bool, task_id: &str, deps: &str, dry_run: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let (all_deps, added, changes) = compute_task_set_deps(&flow_dir, task_id, deps);

    crate::commands::helpers::maybe_apply_changes(&flow_dir, &changes, dry_run);
    if dry_run {
        return;
    }

    if json_mode {
        json_output(json!({
            "task": task_id,
            "depends_on": all_deps,
            "added": added,
            "message": format!("Dependencies set for {}", task_id),
        }));
    } else if !added.is_empty() {
        println!("Added dependencies to {}: {}", task_id, added.join(", "));
    } else {
        println!("No new dependencies added (already set)");
    }
}
