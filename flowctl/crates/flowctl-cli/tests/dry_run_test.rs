//! Integration test: --dry-run flag produces JSON preview without side effects.

use std::process::Command;

fn flowctl_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_flowctl"))
}

/// Set up a temp .flow/ directory with an epic, return (temp_dir, epic_id).
fn setup_flow_dir() -> (tempfile::TempDir, String) {
    let tmp = tempfile::TempDir::new().unwrap();

    // Init .flow/
    let out = flowctl_bin()
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .expect("flowctl init");
    assert!(
        out.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Create an epic
    let out = flowctl_bin()
        .args(["epic", "create", "--title", "Dry Run Test"])
        .current_dir(tmp.path())
        .output()
        .expect("epic create");
    assert!(
        out.status.success(),
        "epic create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Parse epic ID from stdout (format: "Epic fn-N-dry-run-test created: ...")
    let stdout = String::from_utf8_lossy(&out.stdout);
    let epic_id = stdout
        .split_whitespace()
        .nth(1)
        .expect("parse epic id from output")
        .to_string();

    (tmp, epic_id)
}

#[test]
fn dry_run_task_create_produces_json_preview() {
    let (tmp, epic_id) = setup_flow_dir();

    let out = flowctl_bin()
        .args([
            "--dry-run",
            "task",
            "create",
            "--epic",
            &epic_id,
            "--title",
            "Should Not Persist",
        ])
        .current_dir(tmp.path())
        .output()
        .expect("dry-run task create");
    assert!(
        out.status.success(),
        "dry-run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");

    // Verify dry_run envelope
    assert_eq!(parsed["dry_run"], serde_json::json!(true));
    assert!(parsed["changes"]["mutations"].is_array());
    assert!(
        !parsed["changes"]["mutations"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Verify no task was actually created (listing tasks should return empty)
    let out = flowctl_bin()
        .args(["--json", "tasks", "--epic", &epic_id])
        .current_dir(tmp.path())
        .output()
        .expect("tasks list");
    let tasks_stdout = String::from_utf8_lossy(&out.stdout);
    let tasks: serde_json::Value =
        serde_json::from_str(&tasks_stdout).unwrap_or(serde_json::json!({}));
    // tasks should be empty or have zero items
    if let Some(arr) = tasks["tasks"].as_array() {
        assert!(
            arr.is_empty(),
            "dry-run should not have created any tasks, found: {arr:?}"
        );
    }
}

#[test]
fn dry_run_epic_create_produces_no_side_effects() {
    let tmp = tempfile::TempDir::new().unwrap();

    // Init .flow/
    let out = flowctl_bin()
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .expect("flowctl init");
    assert!(out.status.success());

    // Count existing epics
    let out = flowctl_bin()
        .args(["--json", "epics"])
        .current_dir(tmp.path())
        .output()
        .expect("epics list before");
    let before_stdout = String::from_utf8_lossy(&out.stdout);
    let before: serde_json::Value = serde_json::from_str(&before_stdout).unwrap_or_default();
    let before_count = before["epics"].as_array().map_or(0, Vec::len);

    // Dry-run create
    let out = flowctl_bin()
        .args(["--dry-run", "epic", "create", "--title", "Ghost Epic"])
        .current_dir(tmp.path())
        .output()
        .expect("dry-run epic create");
    assert!(out.status.success());

    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON preview");
    assert_eq!(parsed["dry_run"], serde_json::json!(true));

    // Count epics after — should be unchanged
    let out = flowctl_bin()
        .args(["--json", "epics"])
        .current_dir(tmp.path())
        .output()
        .expect("epics list after");
    let after_stdout = String::from_utf8_lossy(&out.stdout);
    let after: serde_json::Value = serde_json::from_str(&after_stdout).unwrap_or_default();
    let after_count = after["epics"].as_array().map_or(0, Vec::len);

    assert_eq!(
        before_count, after_count,
        "dry-run should not create any epic"
    );
}
