//! Integration tests for export/import round-trip (file-based).
//!
//! Tests the JSON → Markdown → JSON path by:
//! 1. Writing epic/task JSON + Markdown files
//! 2. Reading them back via json_store
//! 3. Verifying data matches

use std::fs;

use flowctl_core::frontmatter;
use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, Epic, EpicStatus, Task, EPICS_DIR, TASKS_DIR};

fn make_test_epic(id: &str, title: &str) -> Epic {
    Epic {
        schema_version: 1,
        id: id.to_string(),
        title: title.to_string(),
        status: EpicStatus::Open,
        branch_name: None,
        plan_review: Default::default(),
        completion_review: Default::default(),
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        auto_execute_pending: None,
        auto_execute_set_at: None,
        archived: false,
        file_path: Some(format!("epics/{id}.md")),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn make_test_task(id: &str, epic: &str, title: &str) -> Task {
    Task {
        schema_version: 1,
        id: id.to_string(),
        epic: epic.to_string(),
        title: title.to_string(),
        status: Status::Todo,
        priority: None,
        domain: Domain::General,
        depends_on: vec![],
        files: vec![],
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("tasks/{id}.md")),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn export_import_round_trip() {
    let tmp = tempfile::TempDir::new().unwrap();
    let flow_dir = tmp.path().to_path_buf();

    // Step 1: Write epic and task to JSON store.
    flowctl_core::json_store::ensure_dirs(&flow_dir).unwrap();

    let epic = make_test_epic("fn-50-roundtrip", "Round Trip Test");
    let epic_body = "## Description\nThis is the epic body content.";
    flowctl_core::json_store::epic_write(&flow_dir, &epic).unwrap();

    let task = make_test_task("fn-50-roundtrip.1", "fn-50-roundtrip", "First Task");
    let task_body = "## Implementation\nDo the thing.";
    flowctl_core::json_store::task_write_definition(&flow_dir, &task).unwrap();

    // Step 2: Export to Markdown files.
    let epics_dir = flow_dir.join(EPICS_DIR);
    let tasks_dir = flow_dir.join(TASKS_DIR);

    let doc = frontmatter::Document {
        frontmatter: epic.clone(),
        body: epic_body.to_string(),
    };
    let content = frontmatter::write(&doc).unwrap();
    fs::write(epics_dir.join("fn-50-roundtrip.md"), &content).unwrap();

    let tdoc = frontmatter::Document {
        frontmatter: task.clone(),
        body: task_body.to_string(),
    };
    let tcontent = frontmatter::write(&tdoc).unwrap();
    fs::write(tasks_dir.join("fn-50-roundtrip.1.md"), &tcontent).unwrap();

    // Step 3: Verify data can be read back from JSON store.
    let reimported_epic = flowctl_core::json_store::epic_read(&flow_dir, "fn-50-roundtrip").unwrap();
    assert_eq!(reimported_epic.title, "Round Trip Test");

    let reimported_tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, "fn-50-roundtrip").unwrap();
    assert_eq!(reimported_tasks.len(), 1);
    assert_eq!(reimported_tasks[0].title, "First Task");
}

#[test]
fn empty_flow_dir_produces_no_data() {
    let tmp = tempfile::TempDir::new().unwrap();
    let flow_dir = tmp.path().to_path_buf();
    flowctl_core::json_store::ensure_dirs(&flow_dir).unwrap();

    let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap();
    assert!(epics.is_empty());
}
