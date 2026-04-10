//! Epic dependency commands: add-dep, rm-dep.

use chrono::Utc;
use serde_json::json;

use crate::output::json_output;

use super::super::helpers::get_flow_dir;
use super::helpers::{ensure_flow_exists, load_epic, save_epic, validate_epic_id};
use crate::output::error_exit;

pub fn cmd_add_dep(epic_id: &str, dep_id: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);
    validate_epic_id(dep_id);

    if epic_id == dep_id {
        error_exit("Epic cannot depend on itself");
    }

    // Verify dep epic exists
    let flow_dir = get_flow_dir();
    if flowctl_core::json_store::epic_read(&flow_dir, dep_id).is_err() {
        error_exit(&format!("Epic {dep_id} not found"));
    }

    let mut doc = load_epic(epic_id);

    if doc
        .frontmatter
        .depends_on_epics
        .contains(&dep_id.to_string())
    {
        if json_mode {
            json_output(json!({
                "id": epic_id,
                "depends_on_epics": doc.frontmatter.depends_on_epics,
                "message": format!("{dep_id} already in dependencies"),
            }));
        } else {
            println!("{dep_id} already in {epic_id} dependencies");
        }
        return;
    }

    doc.frontmatter.depends_on_epics.push(dep_id.to_string());
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "depends_on_epics": doc.frontmatter.depends_on_epics,
            "message": format!("Added {dep_id} to {epic_id} dependencies"),
        }));
    } else {
        println!("Added {dep_id} to {epic_id} dependencies");
    }
}

pub fn cmd_rm_dep(epic_id: &str, dep_id: &str, json_mode: bool) {
    ensure_flow_exists();
    validate_epic_id(epic_id);

    let mut doc = load_epic(epic_id);

    if !doc
        .frontmatter
        .depends_on_epics
        .contains(&dep_id.to_string())
    {
        if json_mode {
            json_output(json!({
                "id": epic_id,
                "depends_on_epics": doc.frontmatter.depends_on_epics,
                "message": format!("{dep_id} not in dependencies"),
            }));
        } else {
            println!("{dep_id} not in {epic_id} dependencies");
        }
        return;
    }

    doc.frontmatter.depends_on_epics.retain(|d| d != dep_id);
    doc.frontmatter.updated_at = Utc::now();
    save_epic(&doc);

    if json_mode {
        json_output(json!({
            "id": epic_id,
            "depends_on_epics": doc.frontmatter.depends_on_epics,
            "message": format!("Removed {dep_id} from {epic_id} dependencies"),
        }));
    } else {
        println!("Removed {dep_id} from {epic_id} dependencies");
    }
}
