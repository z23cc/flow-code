//! Epic lifecycle commands: close, reopen, archive, clean.

use std::fs;
use std::path::Path;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::{EpicStatus, ReviewStatus, ARCHIVE_DIR, REVIEWS_DIR, SPECS_DIR};

use super::helpers::{
    ensure_flow_exists, load_epic, review_belongs_to_epic, save_epic, validate_epic_id,
    GAP_BLOCKING_PRIORITIES,
};
use super::super::helpers::get_flow_dir;

pub fn cmd_close(id: &str, skip_gap_check: bool, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    // Check all tasks are done/skipped via JSON files
    let flow_dir = get_flow_dir();
    let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, id).unwrap_or_default();

    let incomplete: Vec<String> = tasks
        .iter()
        .filter(|t| {
            let s = t.status.to_string();
            s != "done" && s != "skipped"
        })
        .map(|t| format!("{} ({})", t.id, t.status))
        .collect();

    if !incomplete.is_empty() {
        error_exit(&format!(
            "Cannot close epic: incomplete tasks - {}",
            incomplete.join(", ")
        ));
    }

    // Gap registry gate — check JSON gaps
    let mut open_blocking_count = 0;
    let mut gap_list_parts: Vec<String> = Vec::new();

    if let Ok(gaps) = flowctl_core::json_store::gaps_read(&flow_dir, id) {
        for gap in &gaps {
            if !gap.resolved && GAP_BLOCKING_PRIORITIES.contains(&gap.priority.as_str()) {
                open_blocking_count += 1;
                gap_list_parts.push(format!("[{}] {}", gap.priority, gap.capability));
            }
        }
    }

    if open_blocking_count > 0 && !skip_gap_check {
        error_exit(&format!(
            "Cannot close epic: {open_blocking_count} unresolved blocking gap(s): {}. \
             Use --skip-gap-check to bypass.",
            gap_list_parts.join(", ")
        ));
    }
    if open_blocking_count > 0 && skip_gap_check && !json_mode {
        eprintln!(
            "WARNING: Bypassing {open_blocking_count} unresolved blocking gap(s)"
        );
    }

    doc.frontmatter.status = EpicStatus::Done;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "done",
            "message": format!("Epic {id} closed"),
            "gaps_skipped": if skip_gap_check { open_blocking_count } else { 0 },
            "retro_suggested": true,
        }));
    } else {
        println!("Epic {id} closed");
        println!(
            "\n  Tip: Run /flow-code:retro to capture lessons learned before archiving."
        );
    }
}

pub fn cmd_reopen(id: &str, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    // Check if archived
    let archive_path = flow_dir.join(ARCHIVE_DIR).join(id);
    if archive_path.exists() {
        if let Ok(epic) = flowctl_core::json_store::epic_read(&flow_dir, id) {
            if epic.archived {
                error_exit(&format!(
                    "Epic {id} is archived. Unarchive it first before reopening."
                ));
            }
        }
    }

    let mut doc = load_epic(id);
    let previous_status = doc.frontmatter.status.to_string();

    if doc.frontmatter.status == EpicStatus::Open {
        error_exit(&format!(
            "Epic {id} is already open (no-op protection)"
        ));
    }

    doc.frontmatter.status = EpicStatus::Open;
    doc.frontmatter.completion_review = ReviewStatus::Unknown;
    doc.frontmatter.plan_review = ReviewStatus::Unknown;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": id,
            "previous_status": previous_status,
            "new_status": "open",
            "message": format!("Epic {id} reopened"),
        }));
    } else {
        println!("Epic {id} reopened (was: {previous_status})");
    }
}

pub fn cmd_archive(id: &str, force: bool, json_mode: bool) {
    let flow_dir = ensure_flow_exists();
    validate_epic_id(id);

    let mut doc = load_epic(id);

    if doc.frontmatter.status != EpicStatus::Done && !force {
        error_exit(&format!(
            "Cannot archive epic {id}: status is '{}', not 'done'. \
             Close it first or use --force.",
            doc.frontmatter.status
        ));
    }

    // Set archived=true in DB
    doc.frontmatter.archived = true;
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    // Build archive directory for specs and reviews
    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    fs::create_dir_all(&archive_dir)
        .unwrap_or_else(|e| error_exit(&format!("Failed to create archive dir: {e}")));

    let mut moved: Vec<String> = Vec::new();

    // Move spec file
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let dest = archive_dir.join(spec_path.file_name().unwrap());
        let _ = fs::rename(&spec_path, &dest);
        moved.push(format!("specs/{}", spec_path.file_name().unwrap().to_string_lossy()));
    }

    // Move review receipts
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if reviews_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&reviews_dir) {
            let mut review_entries: Vec<_> = entries.flatten().collect();
            review_entries.sort_by_key(std::fs::DirEntry::file_name);
            for entry in review_entries {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if review_belongs_to_epic(&name_str, id) {
                    let dest = archive_dir.join(&*name);
                    let _ = fs::rename(entry.path(), &dest);
                    moved.push(format!("reviews/{name_str}"));
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "epic": id,
            "archive_dir": archive_dir.to_string_lossy(),
            "moved": moved,
            "count": moved.len(),
        }));
    } else {
        println!(
            "Archived epic {id} ({} files) \u{2192} .flow/.archive/{id}/",
            moved.len()
        );
        for f in &moved {
            println!("  {f}");
        }
    }
}

pub fn cmd_clean(json_mode: bool) {
    let flow_dir = ensure_flow_exists();

    let mut archived: Vec<String> = Vec::new();

    if let Ok(epics) = flowctl_core::json_store::epic_list(&flow_dir) {
        for epic in &epics {
            if epic.status == EpicStatus::Done && !epic.archived {
                cmd_archive_silent(&epic.id, &flow_dir);
                archived.push(epic.id.clone());
            }
        }
    }

    if json_mode {
        json_output(json!({
            "archived": archived,
            "count": archived.len(),
        }));
    } else if archived.is_empty() {
        println!("No closed epics to archive.");
    } else {
        println!(
            "Archived {} closed epic(s): {}",
            archived.len(),
            archived.join(", ")
        );
    }
}

/// Silent archive helper for clean command (no output).
/// Sets archived=true in DB, moves only specs and reviews to .archive/.
fn cmd_archive_silent(id: &str, flow_dir: &Path) {
    // Set archived=true in JSON
    if let Ok(mut epic) = flowctl_core::json_store::epic_read(flow_dir, id) {
        epic.archived = true;
        epic.updated_at = Utc::now();
        let _ = flowctl_core::json_store::epic_write(flow_dir, &epic);
    }

    let archive_dir = flow_dir.join(ARCHIVE_DIR).join(id);
    let _ = fs::create_dir_all(&archive_dir);

    // Move spec
    let spec_path = flow_dir.join(SPECS_DIR).join(format!("{id}.md"));
    if spec_path.exists() {
        let _ = fs::rename(&spec_path, archive_dir.join(format!("{id}.md")));
    }

    // Move reviews
    let reviews_dir = flow_dir.join(REVIEWS_DIR);
    if reviews_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&reviews_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if review_belongs_to_epic(&name_str, id) {
                    let _ = fs::rename(entry.path(), archive_dir.join(&name));
                }
            }
        }
    }
}
