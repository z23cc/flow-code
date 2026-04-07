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
#[allow(dead_code)]
fn assert_field(output: &str, field: &str, expected: &Value, label: &str) {
    let msg = format!("{label}: not valid JSON");
    let json = parse_json(output).expect(&msg);
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
    // In compact mode (non-TTY), "success" is stripped; just verify valid JSON
    assert!(parse_json(&out).is_some(), "init: output should be valid JSON");
}

#[test]
fn init_idempotent() {
    let dir = temp_dir("rs_reinit_");
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["init"]);
    assert_eq!(exit, 0);
    assert!(parse_json(&out).is_some(), "reinit: output should be valid JSON");
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
    assert_has_keys(&out, &["id", "title"], "epic create");

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
    assert_has_keys(&out, &["id"], "task create");
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
    assert!(parse_json(&out).is_some(), "start: output should be valid JSON");
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
    assert!(parse_json(&out).is_some(), "done: output should be valid JSON");
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
// Verify that the service layer (used by MCP) produces the
// same DB state as the CLI path.
// ═══════════════════════════════════════════════════════════════════════

/// Set up a .flow dir + DB + epic + task via CLI, return (dir, task_id).
#[allow(dead_code)]
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

/// Read task status from the JSON store.
#[allow(dead_code)]
fn json_task_status(work_dir: &Path, task_id: &str) -> String {
    let flow_dir = work_dir.join(".flow");
    let task = flowctl_core::json_store::task_read(&flow_dir, task_id).expect("read task");
    task.status.to_string()
}

#[test]
fn parity_service_round_trip() {
    // Smoke test: create an epic+task via the CLI, then read it back via
    // json_store. Verifies CLI writes JSON files correctly.
    let (dir, task_id) = setup_task("parity-rt");
    let status = json_task_status(dir.path(), &task_id);
    assert_eq!(status, "todo", "newly created task should be todo");
}
