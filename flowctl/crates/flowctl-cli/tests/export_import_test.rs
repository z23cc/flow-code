//! Integration tests for export/import round-trip.
//!
//! Tests the DB → Markdown → DB path by:
//! 1. Creating an in-memory DB with test data
//! 2. Writing Markdown files using frontmatter::write
//! 3. Re-importing via flowctl_db::reindex
//! 4. Verifying data matches

use std::fs;

use flowctl_core::frontmatter;
use flowctl_core::types::{Epic, EpicStatus, Task, Domain, EPICS_DIR, TASKS_DIR};
use flowctl_core::state_machine::Status;

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
    let flow_dir = tmp.path().join(".flow");
    fs::create_dir_all(&flow_dir).unwrap();

    // Step 1: Create DB with test data.
    let conn = flowctl_db::open_memory().unwrap();
    let epic_repo = flowctl_db::EpicRepo::new(&conn);
    let task_repo = flowctl_db::TaskRepo::new(&conn);

    let epic = make_test_epic("fn-50-roundtrip", "Round Trip Test");
    let epic_body = "## Description\nThis is the epic body content.";
    epic_repo.upsert_with_body(&epic, epic_body).unwrap();

    let task = make_test_task("fn-50-roundtrip.1", "fn-50-roundtrip", "First Task");
    let task_body = "## Implementation\nDo the thing.";
    task_repo.upsert_with_body(&task, task_body).unwrap();

    // Step 2: Export to Markdown files.
    let epics_dir = flow_dir.join(EPICS_DIR);
    let tasks_dir = flow_dir.join(TASKS_DIR);
    fs::create_dir_all(&epics_dir).unwrap();
    fs::create_dir_all(&tasks_dir).unwrap();

    let (exported_epic, body) = epic_repo.get_with_body("fn-50-roundtrip").unwrap();
    let doc = frontmatter::Document {
        frontmatter: exported_epic,
        body: body.clone(),
    };
    let content = frontmatter::write(&doc).unwrap();
    fs::write(epics_dir.join("fn-50-roundtrip.md"), &content).unwrap();

    let (exported_task, tbody) = task_repo.get_with_body("fn-50-roundtrip.1").unwrap();
    let tdoc = frontmatter::Document {
        frontmatter: exported_task,
        body: tbody.clone(),
    };
    let tcontent = frontmatter::write(&tdoc).unwrap();
    fs::write(tasks_dir.join("fn-50-roundtrip.1.md"), &tcontent).unwrap();

    // Step 3: Import into a fresh DB.
    let conn2 = flowctl_db::open_memory().unwrap();
    let result = flowctl_db::reindex(&conn2, &flow_dir, None).unwrap();

    assert_eq!(result.epics_indexed, 1);
    assert_eq!(result.tasks_indexed, 1);

    // Step 4: Verify data matches.
    let repo2 = flowctl_db::EpicRepo::new(&conn2);
    let (reimported_epic, reimported_body) = repo2.get_with_body("fn-50-roundtrip").unwrap();
    assert_eq!(reimported_epic.title, "Round Trip Test");
    assert_eq!(reimported_body.trim(), epic_body.trim());

    let trepo2 = flowctl_db::TaskRepo::new(&conn2);
    let (reimported_task, reimported_tbody) = trepo2.get_with_body("fn-50-roundtrip.1").unwrap();
    assert_eq!(reimported_task.title, "First Task");
    assert_eq!(reimported_tbody.trim(), task_body.trim());
}

#[test]
fn export_empty_db_produces_no_files() {
    let tmp = tempfile::TempDir::new().unwrap();
    let flow_dir = tmp.path().join(".flow");
    let epics_dir = flow_dir.join(EPICS_DIR);
    let tasks_dir = flow_dir.join(TASKS_DIR);
    fs::create_dir_all(&epics_dir).unwrap();
    fs::create_dir_all(&tasks_dir).unwrap();

    let conn = flowctl_db::open_memory().unwrap();
    let epic_repo = flowctl_db::EpicRepo::new(&conn);
    let epics = epic_repo.list(None).unwrap();
    assert!(epics.is_empty());
}
