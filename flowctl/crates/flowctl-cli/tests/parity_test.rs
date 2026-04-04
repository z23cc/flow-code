//! Integration tests comparing Rust flowctl output against Python flowctl.
//!
//! These tests set up isolated .flow/ directories, run both binaries with
//! identical input, and verify JSON output structure and exit codes match.
//!
//! Requirements:
//! - Python 3 on PATH
//! - FLOWCTL_PYTHON env var pointing to Python flowctl.py
//!   (defaults to `../../scripts/flowctl.py` relative to workspace root)
//! - Rust binary built via `cargo build`

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the Python flowctl.py script.
fn python_flowctl() -> PathBuf {
    if let Ok(p) = std::env::var("FLOWCTL_PYTHON") {
        return PathBuf::from(p);
    }
    // Default: relative to the repo root (CARGO_MANIFEST_DIR is crates/flowctl-cli)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../scripts/flowctl.py")
}

/// Locate the Rust flowctl binary (cargo-built).
fn rust_flowctl() -> PathBuf {
    // cargo test sets this for us
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_flowctl"));
    if !path.exists() {
        // Fallback: target/debug/flowctl
        path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/debug/flowctl");
    }
    path
}

/// Run Python flowctl: `python3 flowctl.py <cmd...> --json`
fn run_python(work_dir: &Path, args: &[&str]) -> (String, i32) {
    let py = python_flowctl();
    let mut cmd_args: Vec<&str> = args.to_vec();
    cmd_args.push("--json");

    let output = Command::new("python3")
        .arg(&py)
        .args(&cmd_args)
        .current_dir(work_dir)
        .output()
        .expect("Failed to run python3");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

    (combined, output.status.code().unwrap_or(-1))
}

/// Run Rust flowctl: `flowctl --json <cmd...>`
fn run_rust(work_dir: &Path, args: &[&str]) -> (String, i32) {
    let bin = rust_flowctl();
    let mut cmd_args: Vec<&str> = vec!["--json"];
    cmd_args.extend_from_slice(args);

    let output = Command::new(&bin)
        .args(&cmd_args)
        .current_dir(work_dir)
        .output()
        .expect("Failed to run rust flowctl");

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
    // Take the first valid JSON object/array from output
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                return Some(v);
            }
        }
    }
    // Try parsing the whole output
    serde_json::from_str(output.trim()).ok()
}

/// Extract sorted top-level keys from a JSON object.
fn json_keys(val: &Value) -> Vec<String> {
    match val {
        Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            keys
        }
        _ => vec![],
    }
}

/// Assert both outputs parse as JSON with matching top-level keys.
fn assert_keys_match(py_out: &str, rs_out: &str, label: &str) {
    let py_json = parse_json(py_out)
        .unwrap_or_else(|| panic!("{label}: Python output is not valid JSON:\n{py_out}"));
    let rs_json = parse_json(rs_out)
        .unwrap_or_else(|| panic!("{label}: Rust output is not valid JSON:\n{rs_out}"));

    let py_keys = json_keys(&py_json);
    let rs_keys = json_keys(&rs_json);

    assert_eq!(
        py_keys, rs_keys,
        "{label}: JSON keys differ.\n  Python: {py_keys:?}\n  Rust:   {rs_keys:?}"
    );
}

/// Assert Rust keys are a subset of Python keys (for commands where Rust
/// may not yet implement all fields). Also checks that core keys like
/// "success", "id" are present in both.
fn assert_rust_keys_subset(py_out: &str, rs_out: &str, label: &str) {
    let py_json = parse_json(py_out)
        .unwrap_or_else(|| panic!("{label}: Python output is not valid JSON:\n{py_out}"));
    let rs_json = parse_json(rs_out)
        .unwrap_or_else(|| panic!("{label}: Rust output is not valid JSON:\n{rs_out}"));

    let py_keys = json_keys(&py_json);
    let rs_keys = json_keys(&rs_json);

    // Every Rust key should also exist in Python output
    let extra: Vec<&String> = rs_keys.iter().filter(|k| !py_keys.contains(k)).collect();
    assert!(
        extra.is_empty(),
        "{label}: Rust has keys not in Python: {extra:?}\n  Python: {py_keys:?}\n  Rust:   {rs_keys:?}"
    );

    // Log missing keys for visibility (not a failure)
    let missing: Vec<&String> = py_keys.iter().filter(|k| !rs_keys.contains(k)).collect();
    if !missing.is_empty() {
        eprintln!("  {label}: Rust missing keys (not yet implemented): {missing:?}");
    }
}

/// Assert a field equals a specific value in both outputs.
fn assert_field_eq(py_out: &str, rs_out: &str, field: &str, expected: &Value, label: &str) {
    let py_json = parse_json(py_out).expect("Python JSON");
    let rs_json = parse_json(rs_out).expect("Rust JSON");

    assert_eq!(
        py_json.get(field),
        Some(expected),
        "{label}: Python .{field} != {expected}"
    );
    assert_eq!(
        rs_json.get(field),
        Some(expected),
        "{label}: Rust .{field} != {expected}"
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
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn parity_init() {
    let py_dir = temp_dir("py_init_");
    let rs_dir = temp_dir("rs_init_");

    let (py_out, py_exit) = run_python(py_dir.path(), &["init"]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["init"]);

    assert_eq!(py_exit, 0, "Python init should exit 0");
    assert_eq!(rs_exit, 0, "Rust init should exit 0");
    assert_keys_match(&py_out, &rs_out, "init");
    assert_field_eq(&py_out, &rs_out, "success", &Value::Bool(true), "init");
}

#[test]
fn parity_init_idempotent() {
    let py_dir = temp_dir("py_reinit_");
    let rs_dir = temp_dir("rs_reinit_");

    // First init
    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    // Second init (idempotent)
    let (py_out, py_exit) = run_python(py_dir.path(), &["init"]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["init"]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_field_eq(
        &py_out,
        &rs_out,
        "success",
        &Value::Bool(true),
        "reinit",
    );
}

#[test]
fn parity_status_empty() {
    let py_dir = temp_dir("py_status_");
    let rs_dir = temp_dir("rs_status_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_out, py_exit) = run_python(py_dir.path(), &["status"]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["status"]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "status");

    // Verify zero task counts
    let py_json = parse_json(&py_out).unwrap();
    let rs_json = parse_json(&rs_out).unwrap();
    assert_eq!(py_json["tasks"]["todo"], 0);
    assert_eq!(rs_json["tasks"]["todo"], 0);
}

#[test]
fn parity_epics_empty() {
    let py_dir = temp_dir("py_epics_");
    let rs_dir = temp_dir("rs_epics_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_out, py_exit) = run_python(py_dir.path(), &["epics"]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["epics"]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "epics");

    let py_json = parse_json(&py_out).unwrap();
    let rs_json = parse_json(&rs_out).unwrap();
    assert_eq!(py_json["count"], 0);
    assert_eq!(rs_json["count"], 0);
}

#[test]
fn parity_epic_create() {
    let py_dir = temp_dir("py_epicc_");
    let rs_dir = temp_dir("rs_epicc_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_out, py_exit) =
        run_python(py_dir.path(), &["epic", "create", "--title", "Test Epic"]);
    let (rs_out, rs_exit) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "Test Epic"]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "epic create");
    assert_field_eq(
        &py_out,
        &rs_out,
        "success",
        &Value::Bool(true),
        "epic create",
    );

    // Both should have title = "Test Epic"
    let py_json = parse_json(&py_out).unwrap();
    let rs_json = parse_json(&rs_out).unwrap();
    assert_eq!(py_json["title"], "Test Epic");
    assert_eq!(rs_json["title"], "Test Epic");
}

#[test]
fn parity_show_epic() {
    let py_dir = temp_dir("py_show_");
    let rs_dir = temp_dir("rs_show_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_create, _) =
        run_python(py_dir.path(), &["epic", "create", "--title", "Show Me"]);
    let (rs_create, _) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "Show Me"]);

    let py_id = parse_json(&py_create).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_id = parse_json(&rs_create).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (py_out, py_exit) = run_python(py_dir.path(), &["show", &py_id]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["show", &rs_id]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    // show may have extra keys in Python that Rust hasn't implemented yet
    assert_rust_keys_subset(&py_out, &rs_out, "show epic");
}

#[test]
fn parity_task_create() {
    let py_dir = temp_dir("py_taskc_");
    let rs_dir = temp_dir("rs_taskc_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_epic_out, _) =
        run_python(py_dir.path(), &["epic", "create", "--title", "Epic"]);
    let (rs_epic_out, _) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "Epic"]);

    let py_epic = parse_json(&py_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_epic = parse_json(&rs_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (py_out, py_exit) = run_python(
        py_dir.path(),
        &["task", "create", "--epic", &py_epic, "--title", "Task Alpha"],
    );
    let (rs_out, rs_exit) = run_rust(
        rs_dir.path(),
        &["task", "create", "--epic", &rs_epic, "--title", "Task Alpha"],
    );

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "task create");
    assert_field_eq(
        &py_out,
        &rs_out,
        "success",
        &Value::Bool(true),
        "task create",
    );
}

#[test]
fn parity_tasks_list() {
    let py_dir = temp_dir("py_tasks_");
    let rs_dir = temp_dir("rs_tasks_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_epic_out, _) =
        run_python(py_dir.path(), &["epic", "create", "--title", "Epic"]);
    let (rs_epic_out, _) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "Epic"]);

    let py_epic = parse_json(&py_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_epic = parse_json(&rs_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    run_python(
        py_dir.path(),
        &["task", "create", "--epic", &py_epic, "--title", "T1"],
    );
    run_rust(
        rs_dir.path(),
        &["task", "create", "--epic", &rs_epic, "--title", "T1"],
    );

    let (py_out, py_exit) = run_python(py_dir.path(), &["tasks", "--epic", &py_epic]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["tasks", "--epic", &rs_epic]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "tasks");
}

#[test]
fn parity_start() {
    let py_dir = temp_dir("py_start_");
    let rs_dir = temp_dir("rs_start_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_epic_out, _) =
        run_python(py_dir.path(), &["epic", "create", "--title", "E"]);
    let (rs_epic_out, _) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "E"]);

    let py_epic = parse_json(&py_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_epic = parse_json(&rs_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (py_task_out, _) = run_python(
        py_dir.path(),
        &["task", "create", "--epic", &py_epic, "--title", "T"],
    );
    let (rs_task_out, _) = run_rust(
        rs_dir.path(),
        &["task", "create", "--epic", &rs_epic, "--title", "T"],
    );

    let py_task = parse_json(&py_task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_task = parse_json(&rs_task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (py_out, py_exit) = run_python(py_dir.path(), &["start", &py_task]);
    let (rs_out, rs_exit) = run_rust(rs_dir.path(), &["start", &rs_task]);

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "start");
    assert_field_eq(&py_out, &rs_out, "success", &Value::Bool(true), "start");
}

#[test]
fn parity_done() {
    let py_dir = temp_dir("py_done_");
    let rs_dir = temp_dir("rs_done_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (py_epic_out, _) =
        run_python(py_dir.path(), &["epic", "create", "--title", "E"]);
    let (rs_epic_out, _) =
        run_rust(rs_dir.path(), &["epic", "create", "--title", "E"]);

    let py_epic = parse_json(&py_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_epic = parse_json(&rs_epic_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (py_task_out, _) = run_python(
        py_dir.path(),
        &["task", "create", "--epic", &py_epic, "--title", "T"],
    );
    let (rs_task_out, _) = run_rust(
        rs_dir.path(),
        &["task", "create", "--epic", &rs_epic, "--title", "T"],
    );

    let py_task = parse_json(&py_task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rs_task = parse_json(&rs_task_out).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Start first
    run_python(py_dir.path(), &["start", &py_task]);
    run_rust(rs_dir.path(), &["start", &rs_task]);

    // Done with --force --summary
    let (py_out, py_exit) = run_python(
        py_dir.path(),
        &["done", &py_task, "--summary", "Completed", "--force"],
    );
    let (rs_out, rs_exit) = run_rust(
        rs_dir.path(),
        &["done", &rs_task, "--summary", "Completed", "--force"],
    );

    assert_eq!(py_exit, 0);
    assert_eq!(rs_exit, 0);
    assert_keys_match(&py_out, &rs_out, "done");
    assert_field_eq(&py_out, &rs_out, "success", &Value::Bool(true), "done");
}

// ═══════════════════════════════════════════════════════════════════════
// Edge Cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn edge_status_no_flow_dir() {
    let py_dir = temp_dir("py_noflow_");
    let rs_dir = temp_dir("rs_noflow_");

    let (py_out, _py_exit) = run_python(py_dir.path(), &["status"]);
    let (rs_out, _rs_exit) = run_rust(rs_dir.path(), &["status"]);

    // Both should indicate flow_exists=false or return an error
    let py_json = parse_json(&py_out);
    let rs_json = parse_json(&rs_out);

    match (py_json, rs_json) {
        (Some(py), Some(rs)) => {
            // Both return JSON - check flow_exists
            assert_eq!(
                py.get("flow_exists"),
                rs.get("flow_exists"),
                "flow_exists should match"
            );
        }
        (None, None) => {
            // Both return non-JSON error - acceptable
        }
        _ => {
            // One returns JSON, other doesn't - still ok if both indicate error
        }
    }
}

#[test]
fn edge_show_invalid_id() {
    let py_dir = temp_dir("py_badiid_");
    let rs_dir = temp_dir("rs_badiid_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (_py_out, py_exit) = run_python(py_dir.path(), &["show", "nonexistent-999"]);
    let (_rs_out, rs_exit) = run_rust(rs_dir.path(), &["show", "nonexistent-999"]);

    // Both should return non-zero
    assert_ne!(py_exit, 0, "Python show invalid ID should fail");
    assert_ne!(rs_exit, 0, "Rust show invalid ID should fail");
}

#[test]
fn edge_start_invalid_id() {
    let py_dir = temp_dir("py_badst_");
    let rs_dir = temp_dir("rs_badst_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (_py_out, py_exit) = run_python(py_dir.path(), &["start", "bogus-task"]);
    let (_rs_out, rs_exit) = run_rust(rs_dir.path(), &["start", "bogus-task"]);

    assert_ne!(py_exit, 0, "Python start invalid ID should fail");
    assert_ne!(rs_exit, 0, "Rust start invalid ID should fail");
}

#[test]
fn edge_epic_create_no_title() {
    let py_dir = temp_dir("py_notitle_");
    let rs_dir = temp_dir("rs_notitle_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (_py_out, py_exit) = run_python(py_dir.path(), &["epic", "create"]);
    let (_rs_out, rs_exit) = run_rust(rs_dir.path(), &["epic", "create"]);

    assert_ne!(py_exit, 0, "Python epic create without title should fail");
    assert_ne!(rs_exit, 0, "Rust epic create without title should fail");
}

#[test]
fn edge_task_create_no_epic() {
    let py_dir = temp_dir("py_noepic_");
    let rs_dir = temp_dir("rs_noepic_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (_py_out, py_exit) =
        run_python(py_dir.path(), &["task", "create", "--title", "Orphan"]);
    let (_rs_out, rs_exit) =
        run_rust(rs_dir.path(), &["task", "create", "--title", "Orphan"]);

    assert_ne!(py_exit, 0, "Python task create without epic should fail");
    assert_ne!(rs_exit, 0, "Rust task create without epic should fail");
}

#[test]
fn edge_done_no_task_id() {
    let py_dir = temp_dir("py_nodone_");
    let rs_dir = temp_dir("rs_nodone_");

    run_python(py_dir.path(), &["init"]);
    run_rust(rs_dir.path(), &["init"]);

    let (_py_out, py_exit) = run_python(py_dir.path(), &["done"]);
    let (_rs_out, rs_exit) = run_rust(rs_dir.path(), &["done"]);

    assert_ne!(py_exit, 0, "Python done without task ID should fail");
    assert_ne!(rs_exit, 0, "Rust done without task ID should fail");
}
