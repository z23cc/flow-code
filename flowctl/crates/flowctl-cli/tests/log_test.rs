//! Integration tests for the decision log (log decision / log decisions).

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

fn init_flow(prefix: &str) -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    run(dir.path(), &["init"]);
    dir
}

#[test]
fn log_decision_and_query() {
    let dir = init_flow("log_dq_");

    // Record a decision
    let (out, exit) = run(
        dir.path(),
        &[
            "log",
            "decision",
            "--key",
            "review_backend",
            "--value",
            "rp-mcp",
            "--reason",
            "RP available and faster",
        ],
    );
    assert_eq!(exit, 0, "log decision failed: {out}");
    let json = parse_json(&out).expect("log decision should return JSON");
    assert!(
        json.get("id").is_some() || json.get("key").is_some(),
        "should have id or key"
    );

    // Query decisions
    let (list_out, list_exit) = run(dir.path(), &["log", "decisions"]);
    assert_eq!(list_exit, 0, "log decisions failed: {list_out}");
    let list_json = parse_json(&list_out).expect("log decisions should return JSON");
    let decisions = list_json["decisions"]
        .as_array()
        .or_else(|| list_json.as_array())
        .expect("should have a decisions array");
    assert!(!decisions.is_empty(), "should have at least one decision");
}

#[test]
fn log_decision_with_epic_scope() {
    let dir = init_flow("log_ep_");

    // Create an epic to scope to
    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Log Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Record a scoped decision
    let (out, exit) = run(
        dir.path(),
        &[
            "log",
            "decision",
            "--key",
            "branch_strategy",
            "--value",
            "worktree",
            "--reason",
            "parallel work needed",
            "--epic",
            &epic_id,
        ],
    );
    assert_eq!(exit, 0, "scoped log decision failed: {out}");

    // Query scoped
    let (list_out, list_exit) = run(dir.path(), &["log", "decisions", "--epic", &epic_id]);
    assert_eq!(list_exit, 0, "scoped log decisions failed: {list_out}");
}

#[test]
fn log_decisions_empty() {
    let dir = init_flow("log_empty_");

    let (out, exit) = run(dir.path(), &["log", "decisions"]);
    assert_eq!(exit, 0, "log decisions on empty should succeed: {out}");
}
