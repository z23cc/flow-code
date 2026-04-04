//! Integration tests for Rust flowctl JSON output.
//!
//! These tests set up isolated .flow/ directories, run the Rust binary,
//! and verify JSON output structure, field values, and exit codes.
//!
//! Originally these were parity tests comparing against a Python implementation.
//! Since the Python version was removed (577e9c7), they now validate Rust
//! behavior independently.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the Rust flowctl binary (cargo-built).
fn rust_flowctl() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_flowctl"));
    assert!(path.exists(), "flowctl binary not found at {path:?}");
    path
}

/// Run Rust flowctl: `flowctl --json <cmd...>`
fn run(work_dir: &Path, args: &[&str]) -> (String, i32) {
    let bin = rust_flowctl();
    let mut cmd_args: Vec<&str> = vec!["--json"];
    cmd_args.extend_from_slice(args);

    let output = Command::new(&bin)
        .args(&cmd_args)
        .current_dir(work_dir)
        .output()
        .expect("Failed to run flowctl");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

    (combined, output.status.code().unwrap_or(-1))
}

/// Parse JSON output, returning None if unparseable.
fn parse_json(output: &str) -> Option<Value> {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                return Some(v);
            }
        }
    }
    serde_json::from_str(output.trim()).ok()
}

/// Assert output parses as JSON and contains expected keys.
fn assert_has_keys(output: &str, keys: &[&str], label: &str) {
    let json = parse_json(output)
        .unwrap_or_else(|| panic!("{label}: output is not valid JSON:\n{output}"));
    for key in keys {
        assert!(
            json.get(*key).is_some(),
            "{label}: missing expected key '{key}' in {json}"
        );
    }
}

/// Assert a JSON field equals a specific value.
fn assert_field(output: &str, field: &str, expected: &Value, label: &str) {
    let json = parse_json(output).expect(&format!("{label}: not valid JSON"));
    assert_eq!(
        json.get(field),
        Some(expected),
        "{label}: .{field} != {expected}"
    );
}

/// Create a temp directory for testing.
fn temp_dir(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("Failed to create temp dir")
}

// ═══════════════════════════════════════════════════════════════════════
// Core workflow tests (formerly parity tests)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn init() {
    let dir = temp_dir("rs_init_");
    let (out, exit) = run(dir.path(), &["init"]);

    assert_eq!(exit, 0, "init should exit 0");
    assert_field(&out, "success", &Value::Bool(true), "init");
    assert_has_keys(&out, &["success"], "init");
}

#[test]
fn init_idempotent() {
    let dir = temp_dir("rs_reinit_");
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["init"]);
    assert_eq!(exit, 0);
    assert_field(&out, "success", &Value::Bool(true), "reinit");
}

#[test]
fn status_empty() {
    let dir = temp_dir("rs_status_");
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["status"]);
    assert_eq!(exit, 0);
    assert_has_keys(&out, &["tasks"], "status");

    let json = parse_json(&out).unwrap();
    assert_eq!(json["tasks"]["todo"], 0, "todo count should be 0");
}

#[test]
fn epics_empty() {
    let dir = temp_dir("rs_epics_");
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["epics"]);
    assert_eq!(exit, 0);
    assert_has_keys(&out, &["count"], "epics");

    let json = parse_json(&out).unwrap();
    assert_eq!(json["count"], 0, "epic count should be 0");
}

#[test]
fn epic_create() {
    let dir = temp_dir("rs_epicc_");
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["epic", "create", "--title", "Test Epic"]);
    assert_eq!(exit, 0);
    assert_field(&out, "success", &Value::Bool(true), "epic create");
    assert_has_keys(&out, &["success", "id", "title"], "epic create");

    let json = parse_json(&out).unwrap();
    assert_eq!(json["title"], "Test Epic");
}

#[test]
fn show_epic() {
    let dir = temp_dir("rs_show_");
    run(dir.path(), &["init"]);

    let (create_out, _) = run(dir.path(), &["epic", "create", "--title", "Show Me"]);
    let id = parse_json(&create_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (out, exit) = run(dir.path(), &["show", &id]);
    assert_eq!(exit, 0);

    let json = parse_json(&out).unwrap();
    assert_eq!(json["title"], "Show Me", "show should return the epic title");
}

#[test]
fn task_create() {
    let dir = temp_dir("rs_taskc_");
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (out, exit) = run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "Task Alpha"],
    );
    assert_eq!(exit, 0);
    assert_field(&out, "success", &Value::Bool(true), "task create");
    assert_has_keys(&out, &["success", "id"], "task create");
}

#[test]
fn tasks_list() {
    let dir = temp_dir("rs_tasks_");
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "T1"],
    );

    let (out, exit) = run(dir.path(), &["tasks", "--epic", &epic_id]);
    assert_eq!(exit, 0);

    let json = parse_json(&out).unwrap();
    assert_has_keys(&out, &["tasks"], "tasks list");
    // Should have at least 1 task
    let tasks = json["tasks"].as_array().expect("tasks should be an array");
    assert_eq!(tasks.len(), 1, "should have 1 task");
}

#[test]
fn start_task() {
    let dir = temp_dir("rs_start_");
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "E"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (task_out, _) = run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "T"],
    );
    let task_id = parse_json(&task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (out, exit) = run(dir.path(), &["start", &task_id]);
    assert_eq!(exit, 0);
    assert_field(&out, "success", &Value::Bool(true), "start");
}

#[test]
fn done_task() {
    let dir = temp_dir("rs_done_");
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "E"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (task_out, _) = run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "T"],
    );
    let task_id = parse_json(&task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    run(dir.path(), &["start", &task_id]);

    let (out, exit) = run(
        dir.path(),
        &["done", &task_id, "--summary", "Completed", "--force"],
    );
    assert_eq!(exit, 0);
    assert_field(&out, "success", &Value::Bool(true), "done");
}

// ═══════════════════════════════════════════════════════════════════════
// Edge Cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn edge_status_no_flow_dir() {
    let dir = temp_dir("rs_noflow_");

    let (out, exit) = run(dir.path(), &["status"]);

    // Should indicate no .flow/ or return an error
    if exit == 0 {
        let json = parse_json(&out);
        if let Some(j) = json {
            // If JSON returned, flow_exists should be false
            if let Some(fe) = j.get("flow_exists") {
                assert_eq!(*fe, Value::Bool(false));
            }
        }
    }
    // Non-zero exit is also acceptable (no .flow dir)
}

#[test]
fn edge_show_invalid_id() {
    let dir = temp_dir("rs_badiid_");
    run(dir.path(), &["init"]);

    let (_out, exit) = run(dir.path(), &["show", "nonexistent-999"]);
    assert_ne!(exit, 0, "show invalid ID should fail");
}

#[test]
fn edge_start_invalid_id() {
    let dir = temp_dir("rs_badst_");
    run(dir.path(), &["init"]);

    let (_out, exit) = run(dir.path(), &["start", "bogus-task"]);
    assert_ne!(exit, 0, "start invalid ID should fail");
}

#[test]
fn edge_epic_create_no_title() {
    let dir = temp_dir("rs_notitle_");
    run(dir.path(), &["init"]);

    let (_out, exit) = run(dir.path(), &["epic", "create"]);
    assert_ne!(exit, 0, "epic create without title should fail");
}

#[test]
fn edge_task_create_no_epic() {
    let dir = temp_dir("rs_noepic_");
    run(dir.path(), &["init"]);

    let (_out, exit) = run(dir.path(), &["task", "create", "--title", "Orphan"]);
    assert_ne!(exit, 0, "task create without epic should fail");
}

#[test]
fn edge_done_no_task_id() {
    let dir = temp_dir("rs_nodone_");
    run(dir.path(), &["init"]);

    let (_out, exit) = run(dir.path(), &["done"]);
    assert_ne!(exit, 0, "done without task ID should fail");
}

// ═══════════════════════════════════════════════════════════════════════
// Service layer parity tests
//
// Verify that the service layer (used by MCP + daemon) produces the
// same DB state as the CLI path.
// ═══════════════════════════════════════════════════════════════════════

/// Set up a .flow dir + DB + epic + task via CLI, return (dir, task_id).
fn setup_task(prefix: &str) -> (tempfile::TempDir, String) {
    let dir = temp_dir(prefix);
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Parity Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (task_out, _) = run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "Parity Task"],
    );
    let task_id = parse_json(&task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    (dir, task_id)
}

/// Read task status from the DB directly.
fn db_task_status(work_dir: &Path, task_id: &str) -> String {
    let conn = flowctl_db::open(work_dir).expect("open db");
    let repo = flowctl_db::TaskRepo::new(&conn);
    let task = repo.get(task_id).expect("get task");
    task.status.to_string()
}

#[test]
fn parity_start_cli_vs_service() {
    // CLI path
    let (cli_dir, cli_task) = setup_task("par_start_cli_");
    run(cli_dir.path(), &["start", &cli_task]);
    let cli_status = db_task_status(cli_dir.path(), &cli_task);

    // Service path (same setup, then call service directly)
    let (svc_dir, svc_task) = setup_task("par_start_svc_");
    let flow_dir = svc_dir.path().join(".flow");
    let conn = flowctl_db::open(svc_dir.path()).expect("open db");
    let req = flowctl_service::lifecycle::StartTaskRequest {
        task_id: svc_task.clone(),
        force: false,
        actor: "test".to_string(),
    };
    let resp = flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, req);
    assert!(resp.is_ok(), "service start_task should succeed: {:?}", resp.err());
    let svc_status = db_task_status(svc_dir.path(), &svc_task);

    assert_eq!(cli_status, svc_status, "CLI and service should produce same status after start");
    assert_eq!(cli_status, "in_progress", "status should be in_progress");
}

#[test]
fn parity_done_cli_vs_service() {
    // CLI path
    let (cli_dir, cli_task) = setup_task("par_done_cli_");
    run(cli_dir.path(), &["start", &cli_task]);
    run(
        cli_dir.path(),
        &["done", &cli_task, "--summary", "Done via CLI", "--force"],
    );
    let cli_status = db_task_status(cli_dir.path(), &cli_task);

    // Service path
    let (svc_dir, svc_task) = setup_task("par_done_svc_");
    let flow_dir = svc_dir.path().join(".flow");
    let conn = flowctl_db::open(svc_dir.path()).expect("open db");

    // Start first
    let start_req = flowctl_service::lifecycle::StartTaskRequest {
        task_id: svc_task.clone(),
        force: false,
        actor: "test".to_string(),
    };
    flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, start_req).unwrap();

    // Done
    let done_req = flowctl_service::lifecycle::DoneTaskRequest {
        task_id: svc_task.clone(),
        summary: Some("Done via service".to_string()),
        summary_file: None,
        evidence_json: None,
        evidence_inline: None,
        force: true,
        actor: "test".to_string(),
    };
    let resp = flowctl_service::lifecycle::done_task(Some(&conn), &flow_dir, done_req);
    assert!(resp.is_ok(), "service done_task should succeed: {:?}", resp.err());
    let svc_status = db_task_status(svc_dir.path(), &svc_task);

    assert_eq!(cli_status, svc_status, "CLI and service should produce same status after done");
    assert_eq!(cli_status, "done", "status should be done");
}

#[test]
fn parity_start_invalid_task_service() {
    let dir = temp_dir("par_bad_start_");
    run(dir.path(), &["init"]);

    let flow_dir = dir.path().join(".flow");
    let conn = flowctl_db::open(dir.path()).expect("open db");

    let req = flowctl_service::lifecycle::StartTaskRequest {
        task_id: "nonexistent-1.1".to_string(),
        force: false,
        actor: "test".to_string(),
    };
    let result = flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, req);
    assert!(result.is_err(), "service should reject nonexistent task");
}
