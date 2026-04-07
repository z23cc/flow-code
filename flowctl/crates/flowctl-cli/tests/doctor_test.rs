//! Integration tests for the doctor command (state health diagnostics).

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

#[test]
fn doctor_healthy_state() {
    let dir = tempfile::Builder::new().prefix("doctor_ok_").tempdir().unwrap();
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(exit, 0, "doctor should pass on healthy state: {out}");
    let json = parse_json(&out).expect("doctor should return JSON");
    // Should have a healthy indicator
    assert!(
        json.get("healthy").is_some() || json.get("checks").is_some() || json.get("status").is_some(),
        "doctor output should contain health info: {json}"
    );
}

#[test]
fn doctor_no_flow_dir() {
    let dir = tempfile::Builder::new().prefix("doctor_nf_").tempdir().unwrap();

    let (_out, exit) = run(dir.path(), &["doctor"]);
    // Without .flow/ dir, doctor should fail or report unhealthy
    assert_ne!(exit, 0, "doctor without .flow/ should fail");
}

#[test]
fn doctor_with_data() {
    let dir = tempfile::Builder::new().prefix("doctor_wd_").tempdir().unwrap();
    run(dir.path(), &["init"]);

    // Create some state
    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Doctor Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    run(
        dir.path(),
        &["task", "create", "--epic", &epic_id, "--title", "Doctor Task"],
    );

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(exit, 0, "doctor should pass with data: {out}");
    assert!(parse_json(&out).is_some(), "doctor should return valid JSON");
}
