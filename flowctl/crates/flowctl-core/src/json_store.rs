//! JSON file store for epics, tasks, and runtime state.
//!
//! Provides file-based I/O following the `.flow/` directory layout:
//! - `epics/<id>.json` — epic definitions
//! - `specs/<id>.md` — epic spec markdown
//! - `tasks/<id>.json` — task definitions (no runtime fields)
//! - `tasks/<id>.md` — task spec markdown
//! - `.state/tasks/<id>.state.json` — runtime state (status, assignee, evidence)
//! - `.state/events.jsonl` — append-only event log
//! - `.state/pipeline.json` — epic pipeline progress
//! - `.state/phases.json` — task phase progress
//! - `.state/locks.json` — file locks
//! - `.state/approvals.json` — approval records
//! - `memory/entries.jsonl` — append-only memory entries

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state_machine::Status;
use crate::types::{EPICS_DIR, Epic, Evidence, MEMORY_DIR, SPECS_DIR, STATE_DIR, TASKS_DIR, Task};

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
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    ensure_dir(&flow_dir.join(MEMORY_DIR))?;
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

// ── Atomic write helper ────────────────────────────────────────────

/// Write content atomically: write to `.tmp`, then rename over target.
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Acquire an exclusive advisory file lock for read-modify-write operations.
/// Returns the lock file handle (lock is released when handle is dropped).
fn acquire_lock(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&lock_path)?;
    lock_file.lock_exclusive().map_err(|e| {
        StoreError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to acquire lock on {}: {}", lock_path.display(), e),
        ))
    })?;
    Ok(lock_file)
}

// ── Events (.flow/.state/events.jsonl) ─────────────────────────────

fn events_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("events.jsonl")
}

/// Append a JSON event line to the events log.
pub fn events_append(flow_dir: &Path, event_json: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = events_path(flow_dir);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", event_json.trim_end()).map_err(StoreError::Io)?;
    Ok(())
}

/// Read all event lines from the events log.
pub fn events_read_all(flow_dir: &Path) -> Result<Vec<String>> {
    let path = events_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Read events filtered by stream_id (substring match on each line).
pub fn events_read_by_stream(flow_dir: &Path, stream_id: &str) -> Result<Vec<String>> {
    let needle = format!("\"stream_id\":\"{}\"", stream_id);
    let all = events_read_all(flow_dir)?;
    Ok(all
        .into_iter()
        .filter(|line| line.contains(&needle))
        .collect())
}

// ── Pipeline progress (.flow/.state/pipeline.json) ─────────────────

fn pipeline_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("pipeline.json")
}

/// Read the current pipeline phase for an epic.
pub fn pipeline_read(flow_dir: &Path, epic_id: &str) -> Result<Option<String>> {
    let path = pipeline_path(flow_dir);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)?;
    Ok(map.get(epic_id).and_then(|v| v.as_str()).map(String::from))
}

/// Set the pipeline phase for an epic (read-modify-write with file lock + atomic rename).
///
/// Uses advisory file locking via `fs2` to prevent race conditions when
/// multiple processes call this concurrently. The lock is held for the
/// entire read-modify-write cycle and released when `_lock` is dropped.
pub fn pipeline_write(flow_dir: &Path, epic_id: &str, phase: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = pipeline_path(flow_dir);
    let _lock = acquire_lock(&path)?;
    let mut map: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::Map::new()
    };
    map.insert(
        epic_id.to_string(),
        serde_json::Value::String(phase.to_string()),
    );
    let content = serde_json::to_string_pretty(&map)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(())
}

// ── Phase progress (.flow/.state/phases.json) ──────────────────────

fn phases_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("phases.json")
}

/// Get completed phases for a task.
pub fn phases_completed(flow_dir: &Path, task_id: &str) -> Result<Vec<String>> {
    let path = phases_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)?;
    match map.get(task_id) {
        Some(serde_json::Value::Array(arr)) => Ok(arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()),
        _ => Ok(vec![]),
    }
}

/// Mark a phase as done for a task (file-locked read-modify-write).
pub fn phase_mark_done(flow_dir: &Path, task_id: &str, phase: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = phases_path(flow_dir);
    let _lock = acquire_lock(&path)?;
    let mut map: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str(&content)?
    } else {
        serde_json::Map::new()
    };
    let phases = map
        .entry(task_id.to_string())
        .or_insert_with(|| serde_json::Value::Array(vec![]));
    if let serde_json::Value::Array(arr) = phases {
        let phase_val = serde_json::Value::String(phase.to_string());
        if !arr.contains(&phase_val) {
            arr.push(phase_val);
        }
    }
    let content = serde_json::to_string_pretty(&map)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(())
}

/// Reset all phase progress for a task (file-locked read-modify-write).
pub fn phases_reset(flow_dir: &Path, task_id: &str) -> Result<()> {
    let path = phases_path(flow_dir);
    if path.exists() {
        let _lock = acquire_lock(&path)?;
        let content = fs::read_to_string(&path)?;
        let mut map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content)?;
        map.remove(task_id);
        let content = serde_json::to_string_pretty(&map)?;
        atomic_write(&path, content.as_bytes())?;
    }
    let receipts_dir = worker_phase_receipts_dir(flow_dir);
    if receipts_dir.exists() {
        for entry in fs::read_dir(receipts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| name.starts_with(&format!("{task_id}.")) && name.ends_with(".json"))
                    .unwrap_or(false)
            {
                fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

// ── Phase receipts (.flow/.state/*_receipts/) ──────────────────────

fn pipeline_receipts_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("pipeline_receipts")
}

fn worker_phase_receipts_dir(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("worker_phase_receipts")
}

fn pipeline_receipt_path(flow_dir: &Path, epic_id: &str, phase: &str) -> PathBuf {
    pipeline_receipts_dir(flow_dir).join(format!("{epic_id}.{phase}.json"))
}

fn worker_phase_receipt_path(flow_dir: &Path, task_id: &str, phase: &str) -> PathBuf {
    worker_phase_receipts_dir(flow_dir).join(format!("{task_id}.{phase}.json"))
}

pub fn pipeline_phase_receipt_read(
    flow_dir: &Path,
    epic_id: &str,
    phase: &str,
) -> Result<Option<serde_json::Value>> {
    let path = pipeline_receipt_path(flow_dir, epic_id, phase);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let value = serde_json::from_str(&content)?;
    Ok(Some(value))
}

pub fn pipeline_phase_receipt_write(
    flow_dir: &Path,
    epic_id: &str,
    phase: &str,
    receipt: &serde_json::Value,
) -> Result<()> {
    ensure_dir(&pipeline_receipts_dir(flow_dir))?;
    let path = pipeline_receipt_path(flow_dir, epic_id, phase);
    let content = serde_json::to_vec_pretty(receipt)?;
    atomic_write(&path, &content)?;
    Ok(())
}

pub fn worker_phase_receipt_read(
    flow_dir: &Path,
    task_id: &str,
    phase: &str,
) -> Result<Option<serde_json::Value>> {
    let path = worker_phase_receipt_path(flow_dir, task_id, phase);
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    let value = serde_json::from_str(&content)?;
    Ok(Some(value))
}

pub fn worker_phase_receipt_write(
    flow_dir: &Path,
    task_id: &str,
    phase: &str,
    receipt: &serde_json::Value,
) -> Result<()> {
    ensure_dir(&worker_phase_receipts_dir(flow_dir))?;
    let path = worker_phase_receipt_path(flow_dir, task_id, phase);
    let content = serde_json::to_vec_pretty(receipt)?;
    atomic_write(&path, &content)?;
    Ok(())
}

// ── File locks (.flow/.state/locks.json) ───────────────────────────

fn locks_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("locks.json")
}

/// A file lock entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub file_path: String,
    pub task_id: String,
    pub mode: String,
    pub locked_at: String,
}

/// Read all current locks.
pub fn locks_read(flow_dir: &Path) -> Result<Vec<LockEntry>> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    let locks: Vec<LockEntry> = serde_json::from_str(&content)?;
    Ok(locks)
}

/// Acquire a lock on a file for a task (file-locked read-modify-write).
pub fn lock_acquire(flow_dir: &Path, file_path: &str, task_id: &str, mode: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = locks_path(flow_dir);
    let _flock = acquire_lock(&path)?;
    let mut locks = locks_read(flow_dir)?;
    // Remove existing lock by same task on same file (idempotent)
    locks.retain(|l| !(l.file_path == file_path && l.task_id == task_id));
    locks.push(LockEntry {
        file_path: file_path.to_string(),
        task_id: task_id.to_string(),
        mode: mode.to_string(),
        locked_at: Utc::now().to_rfc3339(),
    });
    let content = serde_json::to_string_pretty(&locks)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(())
}

/// Release all locks held by a task. Returns number released (file-locked).
pub fn lock_release_task(flow_dir: &Path, task_id: &str) -> Result<u32> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(0);
    }
    let _flock = acquire_lock(&path)?;
    let mut locks = locks_read(flow_dir)?;
    let before = locks.len();
    locks.retain(|l| l.task_id != task_id);
    let removed = (before - locks.len()) as u32;
    let content = serde_json::to_string_pretty(&locks)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(removed)
}

/// Clear all locks. Returns number cleared.
pub fn locks_clear(flow_dir: &Path) -> Result<u32> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(0);
    }
    let locks = locks_read(flow_dir)?;
    let count = locks.len() as u32;
    atomic_write(&path, b"[]")?;
    Ok(count)
}

// ── Memory (.flow/memory/entries.jsonl) ────────────────────────────

fn memory_entries_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(MEMORY_DIR).join("entries.jsonl")
}

/// Append a JSON memory entry.
pub fn memory_append(flow_dir: &Path, entry_json: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(MEMORY_DIR))?;
    let path = memory_entries_path(flow_dir);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(f, "{}", entry_json.trim_end()).map_err(StoreError::Io)?;
    Ok(())
}

/// Read all memory entries.
pub fn memory_read_all(flow_dir: &Path) -> Result<Vec<String>> {
    let path = memory_entries_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Search memory entries by case-insensitive substring match.
pub fn memory_search_text(flow_dir: &Path, query: &str) -> Result<Vec<String>> {
    let query_lower = query.to_lowercase();
    let all = memory_read_all(flow_dir)?;
    Ok(all
        .into_iter()
        .filter(|line| line.to_lowercase().contains(&query_lower))
        .collect())
}

// ── Approvals (.flow/.state/approvals.json) ────────────────────────

fn approvals_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("approvals.json")
}

/// Read all approvals.
pub fn approvals_read(flow_dir: &Path) -> Result<Vec<serde_json::Value>> {
    let path = approvals_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    let approvals: Vec<serde_json::Value> = serde_json::from_str(&content)?;
    Ok(approvals)
}

/// Write approvals (atomic).
pub fn approvals_write(flow_dir: &Path, approvals: &[serde_json::Value]) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = approvals_path(flow_dir);
    let content = serde_json::to_string_pretty(approvals)?;
    atomic_write(&path, content.as_bytes())?;
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
            GapEntry {
                id: 1,
                capability: "auth".into(),
                priority: "required".into(),
                source: "test".into(),
                resolved: false,
            },
            GapEntry {
                id: 2,
                capability: "logging".into(),
                priority: "nice-to-have".into(),
                source: "test".into(),
                resolved: true,
            },
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
        assert!(
            !tmp_path.exists(),
            "temporary file should be cleaned up by rename"
        );
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
        assert!(flow_dir.join(STATE_DIR).exists());
        assert!(flow_dir.join(MEMORY_DIR).exists());
    }

    // ── Events tests ───────────────────────────────────────────────

    #[test]
    fn test_events_append_and_read() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        events_append(flow_dir, r#"{"stream_id":"s1","type":"created"}"#).unwrap();
        events_append(flow_dir, r#"{"stream_id":"s2","type":"updated"}"#).unwrap();
        events_append(flow_dir, r#"{"stream_id":"s1","type":"done"}"#).unwrap();

        let all = events_read_all(flow_dir).unwrap();
        assert_eq!(all.len(), 3);

        let s1 = events_read_by_stream(flow_dir, "s1").unwrap();
        assert_eq!(s1.len(), 2);
        assert!(s1[0].contains("created"));
        assert!(s1[1].contains("done"));

        let s2 = events_read_by_stream(flow_dir, "s2").unwrap();
        assert_eq!(s2.len(), 1);
    }

    #[test]
    fn test_events_empty() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert!(events_read_all(flow_dir).unwrap().is_empty());
        assert!(events_read_by_stream(flow_dir, "nope").unwrap().is_empty());
    }

    // ── Pipeline tests ─────────────────────────────────────────────

    #[test]
    fn test_pipeline_read_write() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert_eq!(pipeline_read(flow_dir, "fn-1").unwrap(), None);

        pipeline_write(flow_dir, "fn-1", "plan").unwrap();
        assert_eq!(
            pipeline_read(flow_dir, "fn-1").unwrap().as_deref(),
            Some("plan")
        );

        pipeline_write(flow_dir, "fn-1", "work").unwrap();
        assert_eq!(
            pipeline_read(flow_dir, "fn-1").unwrap().as_deref(),
            Some("work")
        );

        pipeline_write(flow_dir, "fn-2", "plan").unwrap();
        assert_eq!(
            pipeline_read(flow_dir, "fn-2").unwrap().as_deref(),
            Some("plan")
        );
        assert_eq!(
            pipeline_read(flow_dir, "fn-1").unwrap().as_deref(),
            Some("work")
        );
    }

    // ── Phases tests ───────────────────────────────────────────────

    #[test]
    fn test_phases_mark_and_read() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert!(phases_completed(flow_dir, "t1").unwrap().is_empty());

        phase_mark_done(flow_dir, "t1", "1").unwrap();
        phase_mark_done(flow_dir, "t1", "2").unwrap();
        phase_mark_done(flow_dir, "t1", "2").unwrap(); // duplicate — no-op

        let completed = phases_completed(flow_dir, "t1").unwrap();
        assert_eq!(completed, vec!["1", "2"]);
    }

    #[test]
    fn test_phases_reset() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        phase_mark_done(flow_dir, "t1", "1").unwrap();
        phase_mark_done(flow_dir, "t1", "5").unwrap();
        phase_mark_done(flow_dir, "t2", "1").unwrap();

        phases_reset(flow_dir, "t1").unwrap();
        assert!(phases_completed(flow_dir, "t1").unwrap().is_empty());
        assert_eq!(phases_completed(flow_dir, "t2").unwrap(), vec!["1"]);
    }

    // ── Locks tests ────────────────────────────────────────────────

    #[test]
    fn test_locks_acquire_read_release() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert!(locks_read(flow_dir).unwrap().is_empty());

        lock_acquire(flow_dir, "src/a.rs", "t1", "write").unwrap();
        lock_acquire(flow_dir, "src/b.rs", "t1", "read").unwrap();
        lock_acquire(flow_dir, "src/c.rs", "t2", "write").unwrap();

        let all = locks_read(flow_dir).unwrap();
        assert_eq!(all.len(), 3);

        let released = lock_release_task(flow_dir, "t1").unwrap();
        assert_eq!(released, 2);

        let remaining = locks_read(flow_dir).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].task_id, "t2");
    }

    #[test]
    fn test_locks_clear() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        lock_acquire(flow_dir, "a", "t1", "write").unwrap();
        lock_acquire(flow_dir, "b", "t2", "read").unwrap();

        let cleared = locks_clear(flow_dir).unwrap();
        assert_eq!(cleared, 2);
        assert!(locks_read(flow_dir).unwrap().is_empty());
    }

    #[test]
    fn test_lock_acquire_idempotent() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        lock_acquire(flow_dir, "a", "t1", "write").unwrap();
        lock_acquire(flow_dir, "a", "t1", "read").unwrap(); // re-lock same file+task

        let locks = locks_read(flow_dir).unwrap();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].mode, "read");
    }

    // ── Memory tests ───────────────────────────────────────────────

    #[test]
    fn test_memory_append_and_search() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        memory_append(flow_dir, r#"{"text":"Rust is great"}"#).unwrap();
        memory_append(flow_dir, r#"{"text":"Python is also nice"}"#).unwrap();
        memory_append(flow_dir, r#"{"text":"rust patterns"}"#).unwrap();

        let all = memory_read_all(flow_dir).unwrap();
        assert_eq!(all.len(), 3);

        let found = memory_search_text(flow_dir, "rust").unwrap();
        assert_eq!(found.len(), 2);

        let none = memory_search_text(flow_dir, "javascript").unwrap();
        assert!(none.is_empty());
    }

    // ── Approvals tests ────────────────────────────────────────────

    #[test]
    fn test_approvals_round_trip() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path();

        assert!(approvals_read(flow_dir).unwrap().is_empty());

        let approvals = vec![
            serde_json::json!({"reviewer": "alice", "status": "approved"}),
            serde_json::json!({"reviewer": "bob", "status": "needs_work"}),
        ];
        approvals_write(flow_dir, &approvals).unwrap();

        let read_back = approvals_read(flow_dir).unwrap();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0]["reviewer"], "alice");
        assert_eq!(read_back[1]["status"], "needs_work");
    }
}
