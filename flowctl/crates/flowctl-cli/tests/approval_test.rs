//! Integration tests for the approval request/resolve/list workflow.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn flowctl_bin() -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_flowctl"));
    assert!(path.exists(), "flowctl binary not found at {path:?}");
    path
}

fn run(work_dir: &Path, args: &[&str]) -> (String, i32) {
    let mut cmd_args: Vec<&str> = vec!["--json"];
    cmd_args.extend_from_slice(args);

    let output = Command::new(flowctl_bin())
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

/// Set up .flow with an epic and a task, return (tmp_dir, epic_id, task_id).
fn setup(prefix: &str) -> (tempfile::TempDir, String, String) {
    let dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    run(dir.path(), &["init"]);

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Approval Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (task_out, _) = run(
        dir.path(),
        &[
            "task",
            "create",
            "--epic",
            &epic_id,
            "--title",
            "Approval Task",
        ],
    );
    let task_id = parse_json(&task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    (dir, epic_id, task_id)
}

#[test]
fn approval_create_and_list() {
    let (dir, _epic_id, task_id) = setup("approval_cl_");

    // Create an approval request
    let (out, exit) = run(
        dir.path(),
        &[
            "approval",
            "create",
            "--task",
            &task_id,
            "--kind",
            "generic",
            "--payload",
            r#"{"message":"need review"}"#,
        ],
    );
    assert_eq!(exit, 0, "approval create failed: {out}");
    let json = parse_json(&out).expect("approval create should return JSON");
    assert!(json.get("id").is_some(), "approval should have an id");

    // List approvals — should have at least one
    let (list_out, list_exit) = run(dir.path(), &["approval", "list"]);
    assert_eq!(list_exit, 0, "approval list failed: {list_out}");
    let list_json = parse_json(&list_out).expect("approval list should return JSON");
    // The list wraps results in "data"
    let approvals = list_json["data"]
        .as_array()
        .or_else(|| list_json.as_array())
        .expect("should have a data array");
    assert!(!approvals.is_empty(), "should have at least one approval");
}

#[test]
fn approval_approve_resolves() {
    let (dir, _epic_id, task_id) = setup("approval_ap_");

    // Create
    let (out, _) = run(
        dir.path(),
        &[
            "approval",
            "create",
            "--task",
            &task_id,
            "--kind",
            "file_access",
            "--payload",
            r#"{"files":["src/main.rs"]}"#,
        ],
    );
    let approval_id = parse_json(&out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Approve it
    let (approve_out, approve_exit) = run(dir.path(), &["approval", "approve", &approval_id]);
    assert_eq!(approve_exit, 0, "approval approve failed: {approve_out}");
}

#[test]
fn approval_reject_resolves() {
    let (dir, _epic_id, task_id) = setup("approval_rj_");

    // Create
    let (out, _) = run(
        dir.path(),
        &[
            "approval",
            "create",
            "--task",
            &task_id,
            "--kind",
            "generic",
            "--payload",
            r#"{"question":"should we proceed?"}"#,
        ],
    );
    let approval_id = parse_json(&out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Reject it
    let (reject_out, reject_exit) = run(
        dir.path(),
        &["approval", "reject", &approval_id, "--reason", "not needed"],
    );
    assert_eq!(reject_exit, 0, "approval reject failed: {reject_out}");
}
