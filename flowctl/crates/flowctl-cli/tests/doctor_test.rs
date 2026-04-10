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

fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(path, content).expect("write test file");
}

fn seed_search_fixture(work_dir: &Path) {
    write_file(
        &work_dir.join("src/lib.rs"),
        r#"
        pub fn helper_fn() -> &'static str {
            "AlphaLiteralMarker"
        }
        "#,
    );
    write_file(
        &work_dir.join("src/main.rs"),
        r#"
        fn main() {
            helper_fn();
        }
        "#,
    );
}

#[test]
fn doctor_healthy_state() {
    let dir = tempfile::Builder::new()
        .prefix("doctor_ok_")
        .tempdir()
        .unwrap();
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(exit, 0, "doctor should pass on healthy state: {out}");
    let json = parse_json(&out).expect("doctor should return JSON");
    // Should have a healthy indicator
    assert!(
        json.get("healthy").is_some()
            || json.get("checks").is_some()
            || json.get("status").is_some(),
        "doctor output should contain health info: {json}"
    );
}

#[test]
fn doctor_no_flow_dir() {
    let dir = tempfile::Builder::new()
        .prefix("doctor_nf_")
        .tempdir()
        .unwrap();

    let (_out, exit) = run(dir.path(), &["doctor"]);
    // Without .flow/ dir, doctor should fail or report unhealthy
    assert_ne!(exit, 0, "doctor without .flow/ should fail");
}

#[test]
fn doctor_with_data() {
    let dir = tempfile::Builder::new()
        .prefix("doctor_wd_")
        .tempdir()
        .unwrap();
    run(dir.path(), &["init"]);

    // Create some state
    let (epic_out, _) = run(dir.path(), &["epic", "create", "--title", "Doctor Epic"]);
    let epic_id = parse_json(&epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    run(
        dir.path(),
        &[
            "task",
            "create",
            "--epic",
            &epic_id,
            "--title",
            "Doctor Task",
        ],
    );

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(exit, 0, "doctor should pass with data: {out}");
    assert!(
        parse_json(&out).is_some(),
        "doctor should return valid JSON"
    );
}

#[test]
fn doctor_reports_graph_and_index_health() {
    let dir = tempfile::Builder::new()
        .prefix("doctor_artifacts_")
        .tempdir()
        .unwrap();
    seed_search_fixture(dir.path());
    run(dir.path(), &["init"]);

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(exit, 0, "doctor should succeed: {out}");

    let json = parse_json(&out).expect("doctor should return valid JSON");
    let graph_status = json["search_tools"]["graph"]["status"]
        .as_str()
        .expect("graph status should be present");
    let index_status = json["search_tools"]["ngram_index"]["status"]
        .as_str()
        .expect("index status should be present");

    assert!(matches!(graph_status, "ok" | "stale"));
    assert!(matches!(index_status, "ok" | "stale"));
}

#[test]
fn doctor_reports_missing_graph_and_index_when_removed() {
    let dir = tempfile::Builder::new()
        .prefix("doctor_missing_artifacts_")
        .tempdir()
        .unwrap();
    seed_search_fixture(dir.path());
    run(dir.path(), &["init"]);

    let _ = std::fs::remove_file(dir.path().join(".flow/graph.bin"));
    let _ = std::fs::remove_file(dir.path().join(".flow/index/ngram.bin"));

    let (out, exit) = run(dir.path(), &["doctor"]);
    assert_eq!(
        exit, 0,
        "doctor should warn, not fail, when artifacts are missing: {out}"
    );

    let json = parse_json(&out).expect("doctor should return valid JSON");
    assert_eq!(json["search_tools"]["graph"]["status"], "missing");
    assert_eq!(json["search_tools"]["ngram_index"]["status"], "missing");
}
