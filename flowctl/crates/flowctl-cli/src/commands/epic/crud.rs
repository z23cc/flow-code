//! Epic CRUD commands: create, plan, review, completion, branch, title, backend, auto_exec.

use std::fs;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::{generate_epic_suffix, parse_id, slugify};
use flowctl_core::types::{
    Epic, EpicStatus, ReviewStatus, Document, FLOW_DIR, SPECS_DIR,
};

use super::helpers::{
    create_epic_spec_body, ensure_flow_exists, ensure_meta_exists, find_max_epic_number,
    load_epic, read_file_or_stdin, save_epic, validate_epic_id,
};
pub fn cmd_create(title: &str, branch: &Option<String>, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    ensure_meta_exists(&flow_dir);

    // DB-based ID allocation
    let max_epic = find_max_epic_number();
    let epic_num = max_epic + 1;
    let slug = slugify(title, 40);
    let suffix = slug.unwrap_or_else(|| generate_epic_suffix(3));
    let epic_id = format!("fn-{epic_num}-{suffix}");

    // Collision check: only check spec file (no more epic MD)
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{epic_id}.md"));
    if spec_path.exists() {
        error_exit(&format!(
            "Refusing to overwrite existing epic {epic_id}. \
             This shouldn't happen - check for orphaned files."
        ));
    }

    let now = Utc::now();
    let branch_name = branch.clone().unwrap_or_else(|| epic_id.clone());

    let epic = Epic {
        schema_version: 1,
        id: epic_id.clone(),
        title: title.to_string(),
        status: EpicStatus::Open,
        branch_name: Some(branch_name),
        plan_review: ReviewStatus::Unknown,
        completion_review: ReviewStatus::Unknown,
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        auto_execute_pending: None,
        auto_execute_set_at: None,
        archived: false,
        file_path: Some(format!("specs/{epic_id}.md")),
        created_at: now,
        updated_at: now,
    };

    // Write to DB (sole source of truth)
    let body = create_epic_spec_body(&epic_id, title);
    let doc = Document {
        frontmatter: epic,
        body: body.clone(),
    };
    save_epic(&doc);

    // Write spec file (body-only Markdown in specs/)
    if let Some(parent) = spec_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&spec_path, &body)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "title": title,
            "spec_path": format!("{FLOW_DIR}/{SPECS_DIR}/{epic_id}.md"),
            "message": format!("Epic {epic_id} created"),
        }));
    } else {
        println!("Epic {epic_id} created: {title}");
    }
}

pub fn cmd_set_plan(id: &str, file: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    // Read content from file or stdin
    let content = read_file_or_stdin(file);

    // Validate: reject duplicate headings
    let heading_re = regex::Regex::new(r"(?m)^(##\s+.+?)\s*$").expect("valid regex");
    let mut seen = std::collections::HashMap::new();
    for cap in heading_re.captures_iter(&content) {
        let h = cap[1].to_string();
        *seen.entry(h).or_insert(0u32) += 1;
    }
    let duplicates: Vec<String> = seen
        .iter()
        .filter(|(_, &count)| count > 1)
        .map(|(h, count)| format!("Duplicate heading: {h} (found {count} times)"))
        .collect();
    if !duplicates.is_empty() {
        error_exit(&format!("Spec validation failed: {}", duplicates.join("; ")));
    }

    // Write spec file (body-only Markdown)
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    fs::write(&spec_path, &content)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write spec: {e}")));

    // Update epic body + timestamp in DB
    doc.body = content;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "spec_path": spec_path.to_string_lossy(),
            "message": format!("Epic {id} spec updated"),
        }));
    } else {
        println!("Epic {id} spec updated");
    }
}

pub fn cmd_set_plan_review_status(id: &str, status: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.plan_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "plan_review_status": status,
            "plan_reviewed_at": Utc::now().to_rfc3339(),
            "message": format!("Epic {id} plan review status set to {status}"),
        }));
    } else {
        println!("Epic {id} plan review status set to {status}");
    }
}

pub fn cmd_set_completion_review_status(id: &str, status: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    let review_status = match status {
        "ship" => ReviewStatus::Passed,
        "needs_work" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    };

    doc.frontmatter.completion_review = review_status;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "completion_review_status": status,
            "completion_reviewed_at": Utc::now().to_rfc3339(),
            "message": format!("Epic {id} completion review status set to {status}"),
        }));
    } else {
        println!("Epic {id} completion review status set to {status}");
    }
}

pub fn cmd_set_branch(id: &str, branch: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    doc.frontmatter.branch_name = Some(branch.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "branch_name": branch,
            "message": format!("Epic {id} branch_name set to {branch}"),
        }));
    } else {
        println!("Epic {id} branch_name set to {branch}");
    }
}

pub fn cmd_set_title(id: &str, new_title: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let old_id = id;
    let doc = load_epic(old_id);

    // Extract epic number
    let parsed = parse_id(old_id)
        .unwrap_or_else(|_| error_exit(&format!("Could not parse epic number from {old_id}")));
    let epic_num = parsed.epic;

    // Generate new ID
    let new_slug = slugify(new_title, 40);
    let new_suffix = new_slug.unwrap_or_else(|| generate_epic_suffix(3));
    let new_id = format!("fn-{epic_num}-{new_suffix}");

    let specs_dir = flow_dir.join(SPECS_DIR);

    // Check collision (if ID changed) via JSON
    if new_id != old_id
        && flowctl_core::json_store::epic_read(&flow_dir, &new_id).is_ok() {
            error_exit(&format!(
                "Epic {new_id} already exists. Choose a different title."
            ));
        }

    // Rename spec file (only MD file we keep)
    let mut files_renamed = 0;
    let old_spec = specs_dir.join(format!("{old_id}.md"));
    if old_spec.exists() {
        let new_spec = specs_dir.join(format!("{new_id}.md"));
        if let Err(e) = fs::rename(&old_spec, &new_spec) {
            error_exit(&format!("Failed to rename spec file: {e}"));
        }
        files_renamed += 1;
    }

    // Rename checkpoint file
    let old_checkpoint = flow_dir.join(format!(".checkpoint-{old_id}.json"));
    if old_checkpoint.exists() {
        let _ = fs::rename(
            &old_checkpoint,
            flow_dir.join(format!(".checkpoint-{new_id}.json")),
        );
        files_renamed += 1;
    }

    // Update epic in DB
    let mut new_doc = doc;
    new_doc.frontmatter.id = new_id.clone();
    new_doc.frontmatter.title = new_title.to_string();
    new_doc.frontmatter.file_path = Some(format!("specs/{new_id}.md"));
    new_doc.frontmatter.updated_at = Utc::now();
    save_epic(&new_doc);

    // Update task records via JSON files
    let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, old_id).unwrap_or_default();
    let mut task_renames: Vec<(String, String)> = Vec::new();
    for task in &tasks {
        if let Ok(p) = parse_id(&task.id) {
            if let Some(task_num) = p.task {
                let new_task_id = format!("{new_id}.{task_num}");
                task_renames.push((task.id.clone(), new_task_id));
            }
        }
    }

    let task_id_map: std::collections::HashMap<&str, &str> = task_renames
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    for task in &tasks {
        let new_task_id = task_id_map
            .get(task.id.as_str())
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| task.id.clone());
        let mut updated_task = task.clone();
        updated_task.id = new_task_id.clone();
        updated_task.epic = new_id.clone();
        updated_task.file_path = Some(format!("tasks/{new_task_id}.md"));
        updated_task.depends_on = updated_task
            .depends_on
            .iter()
            .map(|dep| {
                task_id_map
                    .get(dep.as_str())
                    .map(std::string::ToString::to_string)
                    .unwrap_or_else(|| dep.clone())
            })
            .collect();
        updated_task.updated_at = Utc::now();
        // Delete old files, write new ones
        let _ = flowctl_core::json_store::task_delete(&flow_dir, &task.id);
        let _ = flowctl_core::json_store::task_write_definition(&flow_dir, &updated_task);
    }

    // Update depends_on_epics in other epics that reference old_id
    let mut updated_deps_in: Vec<String> = Vec::new();
    if let Ok(all_epics) = flowctl_core::json_store::epic_list(&flow_dir) {
        for other_epic in &all_epics {
            if other_epic.id == new_id || other_epic.id == old_id {
                continue;
            }
            if other_epic.depends_on_epics.contains(&old_id.to_string()) {
                let mut updated_other = other_epic.clone();
                updated_other.depends_on_epics = updated_other
                    .depends_on_epics
                    .iter()
                    .map(|d| {
                        if d == old_id {
                            new_id.clone()
                        } else {
                            d.clone()
                        }
                    })
                    .collect();
                updated_other.updated_at = Utc::now();
                let _ = flowctl_core::json_store::epic_write(&flow_dir, &updated_other);
                updated_deps_in.push(other_epic.id.clone());
            }
        }
    }

    let mut result = json!({
        "old_id": old_id,
        "new_id": new_id,
        "title": new_title,
        "files_renamed": files_renamed,
        "tasks_updated": task_renames.len(),
        "message": format!("Epic renamed: {old_id} -> {new_id}"),
    });
    if !updated_deps_in.is_empty() {
        result["updated_deps_in"] = json!(updated_deps_in);
    }

    if json_mode {
        json_output(result);
    } else {
        println!("Epic renamed: {old_id} -> {new_id}");
        println!("  Title: {new_title}");
        println!("  Files renamed: {files_renamed}");
        println!("  Tasks updated: {}", task_renames.len());
        if !updated_deps_in.is_empty() {
            println!("  Updated deps in: {}", updated_deps_in.join(", "));
        }
    }
}

pub fn cmd_set_backend(
    id: &str,
    impl_spec: &Option<String>,
    review: &Option<String>,
    sync: &Option<String>,
    json_mode: bool,
) {
    ensure_flow_exists();
    validate_epic_id(id);

    if impl_spec.is_none() && review.is_none() && sync.is_none() {
        error_exit("At least one of --impl, --review, or --sync must be provided");
    }

    let mut doc = load_epic(id);

    let mut updated: Vec<String> = Vec::new();

    if let Some(val) = impl_spec {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_impl = v;
        updated.push(format!(
            "default_impl={}",
            impl_spec.as_deref().unwrap_or("null")
        ));
    }
    if let Some(val) = review {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_review = v;
        updated.push(format!(
            "default_review={}",
            review.as_deref().unwrap_or("null")
        ));
    }
    if let Some(val) = sync {
        let v = if val.is_empty() { None } else { Some(val.clone()) };
        doc.frontmatter.default_sync = v;
        updated.push(format!(
            "default_sync={}",
            sync.as_deref().unwrap_or("null")
        ));
    }

    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "default_impl": doc.frontmatter.default_impl,
            "default_review": doc.frontmatter.default_review,
            "default_sync": doc.frontmatter.default_sync,
            "message": format!("Epic {id} backend specs updated: {}", updated.join(", ")),
        }));
    } else {
        println!(
            "Epic {id} backend specs updated: {}",
            updated.join(", ")
        );
    }
}

pub fn cmd_set_auto_execute(id: &str, pending: bool, done: bool, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    if !pending && !done {
        error_exit("Either --pending or --done must be specified");
    }

    let mut doc = load_epic(id);

    let action;
    if pending {
        doc.frontmatter.auto_execute_pending = Some(true);
        doc.frontmatter.auto_execute_set_at = Some(Utc::now().to_rfc3339());
        action = "pending";
    } else {
        doc.frontmatter.auto_execute_pending = Some(false);
        action = "done";
    }

    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "auto_execute_pending": doc.frontmatter.auto_execute_pending,
            "auto_execute_set_at": doc.frontmatter.auto_execute_set_at,
            "message": format!("Epic {id} auto_execute set to {action}"),
        }));
    } else {
        println!("Epic {id} auto_execute set to {action}");
    }
}
