//! Task query and spec update commands: set-spec, set-backend, show-backend.

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::is_task_id;

use super::{
    ensure_flow_exists, load_epic_md, load_task_doc, load_task_md, patch_body_section,
    read_file_or_stdin, try_open_db, write_task_doc,
};

pub(super) fn cmd_task_set_spec(
    json_mode: bool,
    task_id: &str,
    file: Option<&str>,
    description: Option<&str>,
    acceptance: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    if file.is_none() && description.is_none() && acceptance.is_none() {
        error_exit("Requires --file, --description, or --acceptance");
    }

    let mut doc = load_task_doc(&flow_dir, task_id);

    if let Some(f) = file {
        // Full file replacement mode
        let content = read_file_or_stdin(f);
        doc.body = content;
        doc.frontmatter.updated_at = Utc::now();
        write_task_doc(&flow_dir, task_id, &doc);

        if let Some(conn) = try_open_db() {
            let repo = crate::commands::db_shim::TaskRepo::new(&conn);
            let _ = repo.upsert(&doc.frontmatter);
        }

        if json_mode {
            json_output(json!({
                "id": task_id,
                "message": format!("Task {} spec replaced", task_id),
            }));
        } else {
            println!("Task {} spec replaced", task_id);
        }
        return;
    }

    // Section patch mode
    let mut sections_updated = Vec::new();

    if let Some(desc_file) = description {
        let desc_content = read_file_or_stdin(desc_file);
        doc.body = patch_body_section(&doc.body, "## Description", &desc_content);
        sections_updated.push("## Description");
    }

    if let Some(acc_file) = acceptance {
        let acc_content = read_file_or_stdin(acc_file);
        doc.body = patch_body_section(&doc.body, "## Acceptance", &acc_content);
        sections_updated.push("## Acceptance");
    }

    doc.frontmatter.updated_at = Utc::now();
    write_task_doc(&flow_dir, task_id, &doc);

    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        let _ = repo.upsert(&doc.frontmatter);
    }

    if json_mode {
        json_output(json!({
            "id": task_id,
            "sections": sections_updated,
            "message": format!("Task {} updated: {}", task_id, sections_updated.join(", ")),
        }));
    } else {
        println!("Task {} updated: {}", task_id, sections_updated.join(", "));
    }
}

pub(super) fn cmd_task_set_backend(
    json_mode: bool,
    task_id: &str,
    impl_spec: Option<&str>,
    review: Option<&str>,
    sync: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    if impl_spec.is_none() && review.is_none() && sync.is_none() {
        error_exit("At least one of --impl, --review, or --sync must be provided");
    }

    let mut doc = load_task_doc(&flow_dir, task_id);
    let mut updated = Vec::new();

    if let Some(v) = impl_spec {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.r#impl = val;
        updated.push(format!("impl={}", if v.is_empty() { "null" } else { v }));
    }
    if let Some(v) = review {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.review = val;
        updated.push(format!("review={}", if v.is_empty() { "null" } else { v }));
    }
    if let Some(v) = sync {
        let val = if v.is_empty() { None } else { Some(v.to_string()) };
        doc.frontmatter.sync = val;
        updated.push(format!("sync={}", if v.is_empty() { "null" } else { v }));
    }

    doc.frontmatter.updated_at = Utc::now();
    write_task_doc(&flow_dir, task_id, &doc);

    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::TaskRepo::new(&conn);
        let _ = repo.upsert(&doc.frontmatter);
    }

    let msg = format!("Task {} backend specs updated: {}", task_id, updated.join(", "));
    if json_mode {
        json_output(json!({
            "id": task_id,
            "impl": doc.frontmatter.r#impl,
            "review": doc.frontmatter.review,
            "sync": doc.frontmatter.sync,
            "message": msg,
        }));
    } else {
        println!("{}", msg);
    }
}

pub(super) fn cmd_task_show_backend(json_mode: bool, task_id: &str) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let task = load_task_md(&flow_dir, task_id);
    let epic_id_str = &task.epic;
    let epic = load_epic_md(&flow_dir, epic_id_str);

    // Resolve effective specs with source tracking
    let resolve = |task_val: &Option<String>, epic_key: &str| -> (serde_json::Value, serde_json::Value) {
        if let Some(v) = task_val {
            if !v.is_empty() {
                return (json!(v), json!("task"));
            }
        }
        if let Some(ref e) = epic {
            let epic_val = match epic_key {
                "default_impl" => &e.default_impl,
                "default_review" => &e.default_review,
                "default_sync" => &e.default_sync,
                _ => &None,
            };
            if let Some(v) = epic_val {
                if !v.is_empty() {
                    return (json!(v), json!("epic"));
                }
            }
        }
        (json!(null), json!(null))
    };

    let (impl_spec, impl_source) = resolve(&task.r#impl, "default_impl");
    let (review_spec, review_source) = resolve(&task.review, "default_review");
    let (sync_spec, sync_source) = resolve(&task.sync, "default_sync");

    if json_mode {
        json_output(json!({
            "id": task_id,
            "epic": epic_id_str,
            "impl": {"spec": impl_spec, "source": impl_source},
            "review": {"spec": review_spec, "source": review_source},
            "sync": {"spec": sync_spec, "source": sync_source},
        }));
    } else {
        let fmt = |spec: &serde_json::Value, source: &serde_json::Value| -> String {
            if spec.is_null() {
                "null".to_string()
            } else {
                format!("{} ({})", spec.as_str().unwrap_or("null"), source.as_str().unwrap_or(""))
            }
        };
        println!("impl: {}", fmt(&impl_spec, &impl_source));
        println!("review: {}", fmt(&review_spec, &review_source));
        println!("sync: {}", fmt(&sync_spec, &sync_source));
    }
}
