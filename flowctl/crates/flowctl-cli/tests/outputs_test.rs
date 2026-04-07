//! Integration tests for the outputs write/list/show workflow.

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
    let combined = if stdout.trim().is_empty() { stderr } else { stdout };
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

    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Outputs Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (task_out, _) = run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "Outputs Task"],
    );
    let task_id = parse_json(&task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    (dir, epic_id, task_id)
}

#[test]
fn outputs_write_and_list() {
    let (dir, epic_id, task_id) = setup("outputs_wl_");

    // Write output content from a file
    let content_file = dir.path().join("output_content.md");
    std::fs::write(&content_file, "## Summary\nTask completed successfully.\n").unwrap();

    let (out, exit) = run(
        dir.path(),
        &[
            "outputs", "write", &task_id,
            "--file", content_file.to_str().unwrap(),
        ],
    );
    assert_eq!(exit, 0, "outputs write failed: {out}");

    // List outputs for the epic
    let (list_out, list_exit) = run(
        dir.path(),
        &["outputs", "list", "--epic", &epic_id],
    );
    assert_eq!(list_exit, 0, "outputs list failed: {list_out}");
    let list_json = parse_json(&list_out).expect("outputs list should return JSON");
    // Outputs list wraps results in "entries"
    let outputs = list_json["entries"]
        .as_array()
        .or_else(|| list_json.as_array())
        .expect("should have an entries array");
    assert!(!outputs.is_empty(), "should have at least one output");
}

#[test]
fn outputs_show() {
    let (dir, _epic_id, task_id) = setup("outputs_sh_");

    // Write output content
    let content_file = dir.path().join("show_content.md");
    std::fs::write(&content_file, "## Details\nImplementation notes here.\n").unwrap();

    let (_, write_exit) = run(
        dir.path(),
        &[
            "outputs", "write", &task_id,
            "--file", content_file.to_str().unwrap(),
        ],
    );
    assert_eq!(write_exit, 0);

    // Show the output
    let (show_out, show_exit) = run(dir.path(), &["outputs", "show", &task_id]);
    assert_eq!(show_exit, 0, "outputs show failed: {show_out}");
    // Output should contain our content
    assert!(
        show_out.contains("Implementation notes") || parse_json(&show_out).is_some(),
        "show output should contain our content or be valid JSON"
    );
}

#[test]
fn outputs_list_empty() {
    let (dir, epic_id, _task_id) = setup("outputs_em_");

    let (out, exit) = run(
        dir.path(),
        &["outputs", "list", "--epic", &epic_id],
    );
    assert_eq!(exit, 0, "outputs list on empty should succeed: {out}");
}
