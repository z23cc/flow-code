//! Epic audit command.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::REVIEWS_DIR;

use super::helpers::{ensure_flow_exists, load_epic, validate_epic_id};

/// Find the most recent `epic-audit-<id>-*.json` receipt in `.flow/reviews/`.
/// Returns `(path, age_hours)` or `None` if none exists.
fn find_recent_audit(flow_dir: &Path, id: &str) -> Option<(PathBuf, f64)> {
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if !reviews_dir.is_dir() {
        return None;
    }
    let prefix = format!("epic-audit-{id}-");
    let entries = fs::read_dir(&reviews_dir).ok()?;
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with(&prefix) || !name_str.ends_with(".json") {
            continue;
        }
        let path = entry.path();
        let modified = entry.metadata().and_then(|m| m.modified()).ok();
        if let Some(mtime) = modified {
            match &best {
                None => best = Some((path, mtime)),
                Some((_, cur)) if mtime > *cur => best = Some((path, mtime)),
                _ => {}
            }
        }
    }
    let (path, mtime) = best?;
    let age = std::time::SystemTime::now()
        .duration_since(mtime)
        .ok()?
        .as_secs_f64()
        / 3600.0;
    Some((path, age))
}

pub fn cmd_audit(id: &str, force: bool, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    // Re-use recent audit unless --force.
    if !force {
        if let Some((existing, age_hours)) = find_recent_audit(&flow_dir, id) {
            if age_hours < 24.0 {
                if json_mode {
                    json_output(json!({
                        "id": id,
                        "reused": true,
                        "receipt_path": existing.to_string_lossy(),
                        "age_hours": age_hours,
                        "message": format!(
                            "Reusing audit receipt from {:.1}h ago. Pass --force to regenerate.",
                            age_hours
                        ),
                    }));
                } else {
                    println!(
                        "Reusing audit receipt ({:.1}h old): {}",
                        age_hours,
                        existing.display()
                    );
                    println!("Pass --force to regenerate.");
                }
                return;
            }
        }
    }

    // Load epic spec (frontmatter + body) from DB.
    let epic_doc = load_epic(id);
    let epic_body = epic_doc.body.clone();

    // Load tasks from JSON.
    let tasks: Vec<flowctl_core::types::Task> = flowctl_core::json_store::task_list_by_epic(&flow_dir, id).unwrap_or_default();

    // Shape task summaries for the payload.
    let task_entries: Vec<serde_json::Value> = tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "title": t.title,
                "status": format!("{:?}", t.status).to_lowercase(),
                "domain": format!("{:?}", t.domain).to_lowercase(),
                "depends_on": t.depends_on,
                "files": t.files,
            })
        })
        .collect();

    // Assemble payload receipt.
    let timestamp = Utc::now();
    let receipt = json!({
        "schema_version": 1,
        "kind": "epic-audit-payload",
        "epic_id": id,
        "generated_at": timestamp.to_rfc3339(),
        "epic": {
            "id": epic_doc.frontmatter.id,
            "title": epic_doc.frontmatter.title,
            "status": format!("{:?}", epic_doc.frontmatter.status).to_lowercase(),
            "spec_body": epic_body,
        },
        "tasks": task_entries,
        "task_count": tasks.len(),
        // Audit findings placeholder — populated by agents/epic-auditor.md.
        "audit": {
            "coverage_score": null,
            "gaps": [],
            "redundancies": [],
            "recommendations": [],
            "notes": "Pending auditor agent — run agents/epic-auditor.md against this payload."
        }
    });

    // Write receipt.
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if let Err(e) = fs::create_dir_all(&reviews_dir) {
        error_exit(&format!("Failed to create reviews dir: {e}"));
    }
    let ts_slug = timestamp.format("%Y%m%dT%H%M%SZ").to_string();
    let receipt_path = reviews_dir.join(format!("epic-audit-{id}-{ts_slug}.json"));
    let serialized = serde_json::to_string_pretty(&receipt)
        .unwrap_or_else(|e| error_exit(&format!("Failed to serialize audit: {e}")));
    fs::write(&receipt_path, &serialized)
        .unwrap_or_else(|e| error_exit(&format!("Failed to write {}: {e}", receipt_path.display())));

    if json_mode {
        json_output(json!({
            "id": id,
            "reused": false,
            "receipt_path": receipt_path.to_string_lossy(),
            "task_count": tasks.len(),
            "message": format!(
                "Wrote audit payload to {}. Run agents/epic-auditor.md to populate findings.",
                receipt_path.display()
            ),
        }));
    } else {
        println!("Wrote audit payload: {}", receipt_path.display());
        println!("  Epic: {id} ({} tasks)", tasks.len());
        println!("  Next: run agents/epic-auditor.md with receipt path as input");
    }
}
