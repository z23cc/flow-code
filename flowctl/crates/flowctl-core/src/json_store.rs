//! JSON file store for epics and tasks.
//!
//! Provides file-based I/O following the `.flow/` directory layout:
//! - `epics/<id>.json` — epic definitions
//! - `specs/<id>.md` — epic spec markdown
//! - `tasks/<id>.json` — task definitions (no runtime fields)
//! - `tasks/<id>.md` — task spec markdown
//! - `.state/tasks/<id>.state.json` — runtime state (status, assignee, evidence)

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state_machine::Status;
use crate::types::{Epic, Evidence, Task, EPICS_DIR, SPECS_DIR, STATE_DIR, TASKS_DIR};

// ── Error ───────────────────────────────────────────────────────────

/// Errors from JSON store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

// ── Task Runtime State ──────────────────────────────────────────────

/// Runtime state for a task, stored separately from the definition.
/// Lives in `.state/tasks/<id>.state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_rev: Option<String>,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl Default for TaskState {
    fn default() -> Self {
        Self {
            status: Status::Todo,
            assignee: None,
            claimed_at: None,
            completed_at: None,
            evidence: None,
            blocked_reason: None,
            duration_seconds: None,
            baseline_rev: None,
            final_rev: None,
            retry_count: 0,
            updated_at: Utc::now(),
        }
    }
}

// ── Path helpers ────────────────────────────────────────────────────

fn epics_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(EPICS_DIR)
}

fn specs_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(SPECS_DIR)
}

fn tasks_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(TASKS_DIR)
}

fn state_tasks_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join(TASKS_DIR)
}

fn epic_json_path(flow_dir: &Path, epic_id: &str) -> PathBuf {
    epics_dir(flow_dir).join(format!("{epic_id}.json"))
}

fn epic_spec_path(flow_dir: &Path, epic_id: &str) -> PathBuf {
    specs_dir(flow_dir).join(format!("{epic_id}.md"))
}

fn task_json_path(flow_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(flow_dir).join(format!("{task_id}.json"))
}

fn task_spec_path(flow_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(flow_dir).join(format!("{task_id}.md"))
}

fn task_state_path(flow_dir: &Path, task_id: &str) -> PathBuf {
    state_tasks_dir(flow_dir).join(format!("{task_id}.state.json"))
}

/// Ensure a directory exists, creating it if needed.
fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Ensure all `.flow/` subdirectories exist.
pub fn ensure_dirs(flow_dir: &Path) -> Result<()> {
    ensure_dir(&epics_dir(flow_dir))?;
    ensure_dir(&specs_dir(flow_dir))?;
    ensure_dir(&tasks_dir(flow_dir))?;
    ensure_dir(&state_tasks_dir(flow_dir))?;
    Ok(())
}

// ── Epic operations ─────────────────────────────────────────────────

/// Read an epic definition from `epics/<id>.json`.
pub fn epic_read(flow_dir: &Path, epic_id: &str) -> Result<Epic> {
    let path = epic_json_path(flow_dir, epic_id);
    if !path.exists() {
        return Err(StoreError::NotFound(format!("Epic {epic_id}")));
    }
    let content = fs::read_to_string(&path)?;
    let epic: Epic = serde_json::from_str(&content)?;
    Ok(epic)
}

/// Write an epic definition to `epics/<id>.json`.
pub fn epic_write(flow_dir: &Path, epic: &Epic) -> Result<()> {
    ensure_dir(&epics_dir(flow_dir))?;
    let path = epic_json_path(flow_dir, &epic.id);
    let content = serde_json::to_string_pretty(epic)?;
    fs::write(&path, content)?;
    Ok(())
}

/// List all epics by scanning `epics/*.json`.
pub fn epic_list(flow_dir: &Path) -> Result<Vec<Epic>> {
    let dir = epics_dir(flow_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut epics = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(epic) = serde_json::from_str::<Epic>(&content) {
                epics.push(epic);
            }
        }
    }
    epics.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(epics)
}

/// Read epic spec markdown from `specs/<id>.md`.
pub fn epic_spec_read(flow_dir: &Path, epic_id: &str) -> Result<String> {
    let path = epic_spec_path(flow_dir, epic_id);
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(fs::read_to_string(&path)?)
}

/// Write epic spec markdown to `specs/<id>.md`.
pub fn epic_spec_write(flow_dir: &Path, epic_id: &str, content: &str) -> Result<()> {
    ensure_dir(&specs_dir(flow_dir))?;
    let path = epic_spec_path(flow_dir, epic_id);
    fs::write(&path, content)?;
    Ok(())
}

/// Delete an epic's JSON and spec files.
pub fn epic_delete(flow_dir: &Path, epic_id: &str) -> Result<()> {
    let json_path = epic_json_path(flow_dir, epic_id);
    let spec_path = epic_spec_path(flow_dir, epic_id);
    if json_path.exists() {
        fs::remove_file(&json_path)?;
    }
    if spec_path.exists() {
        fs::remove_file(&spec_path)?;
    }
    Ok(())
}

// ── Task operations ─────────────────────────────────────────────────

/// Read a task by merging definition JSON + runtime state.
pub fn task_read(flow_dir: &Path, task_id: &str) -> Result<Task> {
    let path = task_json_path(flow_dir, task_id);
    if !path.exists() {
        return Err(StoreError::NotFound(format!("Task {task_id}")));
    }
    let content = fs::read_to_string(&path)?;
    let mut task: Task = serde_json::from_str(&content)?;

    // Merge runtime state if it exists
    if let Ok(state) = state_read(flow_dir, task_id) {
        task.status = state.status;
        task.updated_at = state.updated_at;
    }

    Ok(task)
}

/// Write a task definition to `tasks/<id>.json`.
/// Strips runtime fields (status is stored separately in .state/).
pub fn task_write_definition(flow_dir: &Path, task: &Task) -> Result<()> {
    ensure_dir(&tasks_dir(flow_dir))?;
    let path = task_json_path(flow_dir, &task.id);

    // Serialize with status forced to "todo" in definition
    // (actual status lives in .state/)
    let mut task_def = task.clone();
    task_def.status = Status::Todo;
    task_def.updated_at = task.created_at; // definition timestamp is creation time

    let content = serde_json::to_string_pretty(&task_def)?;
    fs::write(&path, content)?;
    Ok(())
}

/// List all tasks for an epic by scanning `tasks/<epic_id>.*.json`.
pub fn task_list_by_epic(flow_dir: &Path, epic_id: &str) -> Result<Vec<Task>> {
    let dir = tasks_dir(flow_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let prefix = format!("{epic_id}.");
    let mut tasks = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with(&prefix) && name.ends_with(".json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(mut task) = serde_json::from_str::<Task>(&content) {
                // Merge runtime state
                if let Ok(state) = state_read(flow_dir, &task.id) {
                    task.status = state.status;
                    task.updated_at = state.updated_at;
                }
                tasks.push(task);
            }
        }
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

/// Read task spec markdown from `tasks/<id>.md`.
pub fn task_spec_read(flow_dir: &Path, task_id: &str) -> Result<String> {
    let path = task_spec_path(flow_dir, task_id);
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(fs::read_to_string(&path)?)
}

/// Write task spec markdown to `tasks/<id>.md`.
pub fn task_spec_write(flow_dir: &Path, task_id: &str, content: &str) -> Result<()> {
    ensure_dir(&tasks_dir(flow_dir))?;
    let path = task_spec_path(flow_dir, task_id);
    fs::write(&path, content)?;
    Ok(())
}

/// Find the highest task number for an epic.
pub fn task_max_num(flow_dir: &Path, epic_id: &str) -> Result<u32> {
    let dir = tasks_dir(flow_dir);
    if !dir.exists() {
        return Ok(0);
    }
    let prefix = format!("{epic_id}.");
    let mut max = 0u32;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().unwrap_or("");
        if name.starts_with(&prefix) && name.ends_with(".json") {
            // Extract number from "fn-N-slug.M.json"
            let without_ext = name.trim_end_matches(".json");
            if let Some(num_str) = without_ext.rsplit('.').next() {
                if let Ok(num) = num_str.parse::<u32>() {
                    max = max.max(num);
                }
            }
        }
    }
    Ok(max)
}

/// Delete a task's JSON, spec, and state files.
pub fn task_delete(flow_dir: &Path, task_id: &str) -> Result<()> {
    let json_path = task_json_path(flow_dir, task_id);
    let spec_path = task_spec_path(flow_dir, task_id);
    let state_path = task_state_path(flow_dir, task_id);
    for p in &[json_path, spec_path, state_path] {
        if p.exists() {
            fs::remove_file(p)?;
        }
    }
    Ok(())
}

// ── Runtime state operations ────────────────────────────────────────

/// Read task runtime state from `.state/tasks/<id>.state.json`.
pub fn state_read(flow_dir: &Path, task_id: &str) -> Result<TaskState> {
    let path = task_state_path(flow_dir, task_id);
    if !path.exists() {
        return Err(StoreError::NotFound(format!("State for {task_id}")));
    }
    let content = fs::read_to_string(&path)?;
    let state: TaskState = serde_json::from_str(&content)?;
    Ok(state)
}

/// Write task runtime state to `.state/tasks/<id>.state.json`.
///
/// Uses write-to-temp + `fs::rename()` for atomic writes on POSIX,
/// preventing partial/corrupt state files on crash or power loss.
pub fn state_write(flow_dir: &Path, task_id: &str, state: &TaskState) -> Result<()> {
    ensure_dir(&state_tasks_dir(flow_dir))?;
    let path = task_state_path(flow_dir, task_id);
    let content = serde_json::to_string_pretty(state)?;

    // Write to a temporary file in the same directory, then atomic rename
    let tmp_path = path.with_extension("state.json.tmp");
    fs::write(&tmp_path, &content)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Find the highest epic number by scanning `epics/fn-N-*.json`.
pub fn epic_max_num(flow_dir: &Path) -> Result<u32> {
    let dir = epics_dir(flow_dir);
    if !dir.exists() {
        return Ok(0);
    }
    let mut max = 0u32;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().unwrap_or("");
        if name.starts_with("fn-") && name.ends_with(".json") {
            // Extract N from "fn-N-slug.json"
            let without_prefix = name.trim_start_matches("fn-");
            if let Some(num_str) = without_prefix.split('-').next() {
                if let Ok(num) = num_str.parse::<u32>() {
                    max = max.max(num);
                }
            }
        }
    }
    Ok(max)
}

/// List all tasks across all epics.
pub fn task_list_all(flow_dir: &Path) -> Result<Vec<Task>> {
    let dir = tasks_dir(flow_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut tasks = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".json") {
            let content = fs::read_to_string(&path)?;
            if let Ok(mut task) = serde_json::from_str::<Task>(&content) {
                if let Ok(state) = state_read(flow_dir, &task.id) {
                    task.status = state.status;
                    task.updated_at = state.updated_at;
                }
                tasks.push(task);
            }
        }
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

// ── Gap operations ──────────────────────────────────────────────────

/// Gap entry stored in `gaps/<epic-id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GapEntry {
    pub id: u32,
    pub capability: String,
    pub priority: String,
    pub source: String,
    #[serde(default)]
    pub resolved: bool,
}

/// Read gaps for an epic from `gaps/<epic-id>.json`.
pub fn gaps_read(flow_dir: &Path, epic_id: &str) -> Result<Vec<GapEntry>> {
    let path = flow_dir.join("gaps").join(format!("{epic_id}.json"));
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    let gaps: Vec<GapEntry> = serde_json::from_str(&content)?;
    Ok(gaps)
}

/// Write gaps for an epic to `gaps/<epic-id>.json`.
pub fn gaps_write(flow_dir: &Path, epic_id: &str, gaps: &[GapEntry]) -> Result<()> {
    let dir = flow_dir.join("gaps");
    ensure_dir(&dir)?;
    let path = dir.join(format!("{epic_id}.json"));
    let content = serde_json::to_string_pretty(gaps)?;
    fs::write(&path, content)?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EpicStatus, ReviewStatus};
    use tempfile::TempDir;

    fn make_epic(id: &str) -> Epic {
        Epic {
            schema_version: 1,
            id: id.to_string(),
            title: "Test Epic".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            auto_execute_pending: None,
            auto_execute_set_at: None,
            archived: false,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_task(id: &str, epic: &str) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: epic.to_string(),
            title: "Test Task".to_string(),
            status: Status::Todo,
            priority: None,
            domain: crate::types::Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_epic_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();
        let epic = make_epic("fn-1-test");

        epic_write(flow_dir, &epic).unwrap();
        let read_back = epic_read(flow_dir, "fn-1-test").unwrap();
        assert_eq!(read_back.id, "fn-1-test");
        assert_eq!(read_back.title, "Test Epic");
    }

    #[test]
    fn test_epic_list() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        epic_write(flow_dir, &make_epic("fn-1-a")).unwrap();
        epic_write(flow_dir, &make_epic("fn-2-b")).unwrap();

        let epics = epic_list(flow_dir).unwrap();
        assert_eq!(epics.len(), 2);
        assert_eq!(epics[0].id, "fn-1-a");
        assert_eq!(epics[1].id, "fn-2-b");
    }

    #[test]
    fn test_epic_spec_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        epic_spec_write(flow_dir, "fn-1-test", "# My Epic\n\nSome content").unwrap();
        let spec = epic_spec_read(flow_dir, "fn-1-test").unwrap();
        assert_eq!(spec, "# My Epic\n\nSome content");
    }

    #[test]
    fn test_task_definition_strips_runtime() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        let mut task = make_task("fn-1-test.1", "fn-1-test");
        task.status = Status::InProgress; // this should NOT persist in definition

        task_write_definition(flow_dir, &task).unwrap();

        // Read raw JSON — status should be "todo" in definition file
        let raw = fs::read_to_string(task_json_path(flow_dir, "fn-1-test.1")).unwrap();
        let raw_json: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(raw_json["status"], "todo");
    }

    #[test]
    fn test_task_read_merges_state() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        let task = make_task("fn-1-test.1", "fn-1-test");
        task_write_definition(flow_dir, &task).unwrap();

        // Write runtime state
        let state = TaskState {
            status: Status::InProgress,
            assignee: Some("worker-1".to_string()),
            ..Default::default()
        };
        state_write(flow_dir, "fn-1-test.1", &state).unwrap();

        // Read should merge
        let read_back = task_read(flow_dir, "fn-1-test.1").unwrap();
        assert_eq!(read_back.status, Status::InProgress);
    }

    #[test]
    fn test_task_list_by_epic() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        task_write_definition(flow_dir, &make_task("fn-1-test.1", "fn-1-test")).unwrap();
        task_write_definition(flow_dir, &make_task("fn-1-test.2", "fn-1-test")).unwrap();
        task_write_definition(flow_dir, &make_task("fn-2-other.1", "fn-2-other")).unwrap();

        let tasks = task_list_by_epic(flow_dir, "fn-1-test").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "fn-1-test.1");
        assert_eq!(tasks[1].id, "fn-1-test.2");
    }

    #[test]
    fn test_task_max_num() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        task_write_definition(flow_dir, &make_task("fn-1-test.1", "fn-1-test")).unwrap();
        task_write_definition(flow_dir, &make_task("fn-1-test.5", "fn-1-test")).unwrap();
        task_write_definition(flow_dir, &make_task("fn-1-test.3", "fn-1-test")).unwrap();

        let max = task_max_num(flow_dir, "fn-1-test").unwrap();
        assert_eq!(max, 5);
    }

    #[test]
    fn test_task_spec_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        task_spec_write(flow_dir, "fn-1-test.1", "# Task\n\n## Description\nTBD").unwrap();
        let spec = task_spec_read(flow_dir, "fn-1-test.1").unwrap();
        assert_eq!(spec, "# Task\n\n## Description\nTBD");
    }

    #[test]
    fn test_state_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        let state = TaskState {
            status: Status::Done,
            assignee: Some("worker-1".to_string()),
            evidence: Some(Evidence::default()),
            ..Default::default()
        };
        state_write(flow_dir, "fn-1-test.1", &state).unwrap();

        let read_back = state_read(flow_dir, "fn-1-test.1").unwrap();
        assert_eq!(read_back.status, Status::Done);
        assert_eq!(read_back.assignee.as_deref(), Some("worker-1"));
    }

    #[test]
    fn test_gaps_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        let gaps = vec![
            GapEntry { id: 1, capability: "auth".into(), priority: "required".into(), source: "test".into(), resolved: false },
            GapEntry { id: 2, capability: "logging".into(), priority: "nice-to-have".into(), source: "test".into(), resolved: true },
        ];
        gaps_write(flow_dir, "fn-1-test", &gaps).unwrap();

        let read_back = gaps_read(flow_dir, "fn-1-test").unwrap();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].capability, "auth");
        assert!(read_back[1].resolved);
    }

    #[test]
    fn test_state_write_atomic_no_corrupt() {
        // Verify that state_write uses atomic rename: if the original file
        // exists, a second write should fully replace it (no partial content).
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        // Write initial state
        let state1 = TaskState {
            status: Status::InProgress,
            assignee: Some("worker-1".to_string()),
            ..Default::default()
        };
        state_write(flow_dir, "fn-1-test.1", &state1).unwrap();

        // Write a second state that overwrites the first
        let state2 = TaskState {
            status: Status::Done,
            assignee: Some("worker-2".to_string()),
            ..Default::default()
        };
        state_write(flow_dir, "fn-1-test.1", &state2).unwrap();

        // Read back and verify the file contains only the second write
        let read_back = state_read(flow_dir, "fn-1-test.1").unwrap();
        assert_eq!(read_back.status, Status::Done);
        assert_eq!(read_back.assignee.as_deref(), Some("worker-2"));

        // Verify no leftover .tmp file exists
        let tmp_path = task_state_path(flow_dir, "fn-1-test.1").with_extension("state.json.tmp");
        assert!(!tmp_path.exists(), "temporary file should be cleaned up by rename");
    }

    #[test]
    fn test_not_found_errors() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert!(epic_read(flow_dir, "nonexistent").is_err());
        assert!(task_read(flow_dir, "nonexistent").is_err());
        assert!(state_read(flow_dir, "nonexistent").is_err());
    }

    #[test]
    fn test_ensure_dirs() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        ensure_dirs(flow_dir).unwrap();
        assert!(epics_dir(flow_dir).exists());
        assert!(specs_dir(flow_dir).exists());
        assert!(tasks_dir(flow_dir).exists());
        assert!(state_tasks_dir(flow_dir).exists());
    }
}
