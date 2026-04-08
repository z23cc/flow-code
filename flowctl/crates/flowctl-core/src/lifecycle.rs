//! Lifecycle service functions: start, done, block, fail, restart.
//!
//! These functions contain the business logic extracted from the CLI
//! lifecycle commands. Each accepts a request struct and returns
//! `ServiceResult<Response>`, using JSON file store as the sole source of truth.

use std::fs;
use std::path::Path;

use chrono::Utc;

use crate::id::{epic_id_from_task, is_task_id};
use crate::state_machine::{Status, Transition};
use crate::types::{
    Epic, EpicStatus, Evidence, RuntimeState, Task, REVIEWS_DIR,
};

use crate::error::{ServiceError, ServiceResult};

// ── Request / Response types ───────────────────────────────────────

/// Request to start a task.
pub struct StartTaskRequest {
    pub task_id: String,
    pub force: bool,
    pub actor: String,
}

/// Response from starting a task.
pub struct StartTaskResponse {
    pub task_id: String,
    pub status: Status,
}

/// Request to complete a task.
pub struct DoneTaskRequest {
    pub task_id: String,
    pub summary: Option<String>,
    pub summary_file: Option<String>,
    pub evidence_json: Option<String>,
    pub evidence_inline: Option<String>,
    pub force: bool,
    pub actor: String,
}

/// Response from completing a task.
pub struct DoneTaskResponse {
    pub task_id: String,
    pub status: Status,
    pub duration_seconds: Option<u64>,
    pub ws_warning: Option<String>,
}

/// Request to block a task.
pub struct BlockTaskRequest {
    pub task_id: String,
    /// Block reason text (not a file path).
    pub reason: String,
}

/// Response from blocking a task.
pub struct BlockTaskResponse {
    pub task_id: String,
    pub status: Status,
}

/// Request to fail a task.
pub struct FailTaskRequest {
    pub task_id: String,
    pub reason: Option<String>,
    pub force: bool,
}

/// Response from failing a task.
pub struct FailTaskResponse {
    pub task_id: String,
    pub final_status: Status,
    pub reason: String,
    pub upstream_failed_ids: Vec<String>,
    pub retry_count: Option<u32>,
    pub max_retries: Option<u32>,
}

/// Request to restart a task (and cascade to dependents).
pub struct RestartTaskRequest {
    pub task_id: String,
    pub dry_run: bool,
    pub force: bool,
}

/// Response from restarting a task.
pub struct RestartTaskResponse {
    pub cascade_from: String,
    pub reset_ids: Vec<String>,
    pub skipped_ids: Vec<String>,
    pub in_progress_overridden: Vec<String>,
}

// ── Helpers (internal to service) ──────────────────────────────────

fn validate_task_id(id: &str) -> ServiceResult<()> {
    if !is_task_id(id) {
        return Err(ServiceError::ValidationError(format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        )));
    }
    Ok(())
}

/// Load a task from JSON files.
fn load_task(flow_dir: &Path, task_id: &str) -> Option<Task> {
    crate::json_store::task_read(flow_dir, task_id).ok()
}

fn load_epic(flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    crate::json_store::epic_read(flow_dir, epic_id).ok()
}

fn get_runtime(flow_dir: &Path, task_id: &str) -> Option<RuntimeState> {
    let state = crate::json_store::state_read(flow_dir, task_id).ok()?;
    Some(RuntimeState {
        task_id: task_id.to_string(),
        assignee: state.assignee,
        claimed_at: state.claimed_at,
        completed_at: state.completed_at,
        duration_secs: state.duration_seconds,
        blocked_reason: state.blocked_reason,
        baseline_rev: state.baseline_rev,
        final_rev: state.final_rev,
        retry_count: state.retry_count,
    })
}

/// Load all tasks for an epic from JSON files.
fn load_tasks_for_epic(
    flow_dir: &Path,
    epic_id: &str,
) -> std::collections::HashMap<String, Task> {
    use std::collections::HashMap;

    if let Ok(tasks) = crate::json_store::task_list_by_epic(flow_dir, epic_id) {
        let mut map = HashMap::new();
        for task in tasks {
            map.insert(task.id.clone(), task);
        }
        return map;
    }

    HashMap::new()
}

/// Find all downstream dependents of a task within the same epic.
fn find_dependents(
    flow_dir: &Path,
    task_id: &str,
) -> Vec<String> {
    let epic_id = match epic_id_from_task(task_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(flow_dir, &epic_id);
    let mut dependents = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![task_id.to_string()];

    while let Some(current) = queue.pop() {
        for (tid, task) in &tasks {
            if visited.contains(tid.as_str()) {
                continue;
            }
            if task.depends_on.contains(&current) {
                visited.insert(tid.clone());
                dependents.push(tid.clone());
                queue.push(tid.clone());
            }
        }
    }

    dependents.sort();
    dependents
}

/// Load config.json from .flow/ once, returning the parsed value.
fn load_config(flow_dir: &Path) -> Option<serde_json::Value> {
    let config_path = flow_dir.join("config.json");
    let content = fs::read_to_string(&config_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Read max_retries from a pre-loaded config (defaults to 0 = no retries).
fn get_max_retries_from_config(config: Option<&serde_json::Value>) -> u32 {
    config
        .and_then(|c| c.get("max_retries"))
        .and_then(serde_json::Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(0)
}

/// Propagate upstream_failed to all transitive downstream tasks.
fn propagate_upstream_failure(
    flow_dir: &Path,
    failed_id: &str,
) -> Vec<String> {
    let epic_id = match epic_id_from_task(failed_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(flow_dir, &epic_id);
    let task_list: Vec<Task> = tasks.values().cloned().collect();

    let dag = match crate::TaskDag::from_tasks(&task_list) {
        Ok(d) => {
            if d.detect_cycles().is_some() {
                return Vec::new();
            }
            d
        }
        Err(_) => return Vec::new(),
    };

    let downstream = dag.propagate_failure(failed_id);
    let mut affected = Vec::new();

    for tid in &downstream {
        let task = match tasks.get(tid) {
            Some(t) => t,
            None => continue,
        };

        if task.status.is_satisfied() || task.status.is_failed() {
            continue;
        }

        if let Ok(mut state) = crate::json_store::state_read(flow_dir, tid) {
            state.status = Status::UpstreamFailed;
            state.updated_at = Utc::now();
            if let Err(e) = crate::json_store::state_write(flow_dir, tid, &state) {
                eprintln!("warning: failed to write upstream_failed state for {tid}: {e}");
            }
        } else {
            let state = crate::json_store::TaskState {
                status: Status::UpstreamFailed,
                ..Default::default()
            };
            if let Err(e) = crate::json_store::state_write(flow_dir, tid, &state) {
                eprintln!("warning: failed to write upstream_failed state for {tid}: {e}");
            }
        }

        affected.push(tid.clone());
    }

    affected
}

/// Handle task failure: check retries, set up_for_retry or failed + propagate.
fn handle_task_failure(
    flow_dir: &Path,
    task_id: &str,
    runtime: &Option<RuntimeState>,
    config: Option<&serde_json::Value>,
) -> std::io::Result<(Status, Vec<String>)> {
    let max_retries = get_max_retries_from_config(config);
    let current_retry_count = runtime.as_ref().map(|r| r.retry_count).unwrap_or(0);

    if max_retries > 0 && current_retry_count < max_retries {
        let new_retry_count = current_retry_count + 1;

        let task_state = crate::json_store::TaskState {
            status: Status::UpForRetry,
            assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: None,
            completed_at: None,
            evidence: None,
            blocked_reason: None,
            duration_seconds: None,
            baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: None,
            retry_count: new_retry_count,
            updated_at: Utc::now(),
        };
        crate::json_store::state_write(flow_dir, task_id, &task_state)
            .map_err(|e| std::io::Error::other(format!("failed to write retry state for {task_id}: {e}")))?;

        log_audit_event(flow_dir, task_id, "task_failed");

        Ok((Status::UpForRetry, Vec::new()))
    } else {
        let task_state = crate::json_store::TaskState {
            status: Status::Failed,
            ..Default::default()
        };
        crate::json_store::state_write(flow_dir, task_id, &task_state)
            .map_err(|e| std::io::Error::other(format!("failed to write failed state for {task_id}: {e}")))?;

        log_audit_event(flow_dir, task_id, "task_failed");

        let affected = propagate_upstream_failure(flow_dir, task_id);
        Ok((Status::Failed, affected))
    }
}

// ── done_task sub-functions ───────────────────────────────────────

/// Validate that a task can be completed: check status and actor.
fn validate_done_request(
    task: &Task,
    runtime: &Option<RuntimeState>,
    req: &DoneTaskRequest,
) -> ServiceResult<()> {
    if !req.force {
        match task.status {
            Status::InProgress => {}
            Status::Done => {
                return Err(ServiceError::InvalidTransition(format!(
                    "Task {} is already done",
                    req.task_id
                )));
            }
            other => {
                return Err(ServiceError::InvalidTransition(format!(
                    "Task {} is '{}', not 'in_progress'",
                    req.task_id, other
                )));
            }
        }

        if let Some(ref rt) = runtime {
            if let Some(ref assignee) = rt.assignee {
                if assignee != &req.actor {
                    return Err(ServiceError::CrossActorViolation(format!(
                        "Cannot complete task {}: claimed by '{}'",
                        req.task_id, assignee
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Parse evidence from JSON string or file path, returning the parsed value.
fn parse_evidence(req: &DoneTaskRequest) -> ServiceResult<serde_json::Value> {
    let evidence_obj: serde_json::Value = if let Some(ref ev) = req.evidence_json {
        let raw = if ev.trim().starts_with('{') {
            ev.clone()
        } else {
            fs::read_to_string(ev).map_err(|e| {
                ServiceError::IoError(std::io::Error::new(
                    e.kind(),
                    format!("Cannot read evidence file: {}", e),
                ))
            })?
        };
        serde_json::from_str(&raw).map_err(|e| {
            ServiceError::ValidationError(format!("Evidence JSON invalid: {}", e))
        })?
    } else if let Some(ref ev) = req.evidence_inline {
        serde_json::from_str(ev).map_err(|e| {
            ServiceError::ValidationError(format!("Evidence invalid JSON: {}", e))
        })?
    } else {
        serde_json::json!({"commits": [], "tests": [], "prs": []})
    };

    if !evidence_obj.is_object() {
        return Err(ServiceError::ValidationError(
            "Evidence JSON must be an object with keys: commits/tests/prs".to_string(),
        ));
    }

    Ok(evidence_obj)
}

/// Calculate duration in seconds from claimed_at to now.
fn compute_duration(runtime: &Option<RuntimeState>) -> Option<u64> {
    runtime
        .as_ref()
        .and_then(|rt| rt.claimed_at)
        .map(|start| {
            let dur = Utc::now() - start;
            dur.num_seconds().max(0) as u64
        })
}

/// Archive a review receipt from evidence to the reviews directory.
fn archive_review_receipt(
    flow_dir: &Path,
    task_id: &str,
    evidence_obj: &serde_json::Value,
) {
    if let Some(receipt) = evidence_obj.get("review_receipt") {
        if receipt.is_object() {
            let reviews_dir = flow_dir.join(REVIEWS_DIR);
            let _ = fs::create_dir_all(&reviews_dir);
            let mode = receipt
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let rtype = receipt
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("review");
            let filename = format!("{}-{}-{}.json", rtype, task_id, mode);
            if let Ok(content) = serde_json::to_string_pretty(receipt) {
                let _ = fs::write(reviews_dir.join(filename), content);
            }
        }
    }
}

// ── Audit event helper ───────────────────────────────────────────

/// Log an audit event to the JSONL event log. Failures are silently ignored.
fn log_audit_event(
    flow_dir: &Path,
    task_id: &str,
    event_type: &str,
) {
    let epic_id = epic_id_from_task(task_id).unwrap_or_default();
    let event = serde_json::json!({
        "stream_id": format!("task:{task_id}"),
        "type": event_type,
        "epic_id": epic_id,
        "task_id": task_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let _ = crate::json_store::events_append(flow_dir, &event.to_string());
}

/// Emit a task event to the event store. Failures are silently ignored.
fn emit_task_event(
    flow_dir: &Path,
    task_id: &str,
    event_type: &str,
    source_cmd: &str,
) {
    let stream_id = format!("task:{task_id}");
    let event = serde_json::json!({
        "stream_id": stream_id,
        "type": event_type,
        "source_cmd": source_cmd,
        "actor": "lifecycle",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    let _ = crate::json_store::events_append(flow_dir, &event.to_string());
}

// ── Service functions ──────────────────────────────────────────────

/// Start a task: validate deps, state machine, actor, update state.
pub fn start_task(
    flow_dir: &Path,
    req: StartTaskRequest,
) -> ServiceResult<StartTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(flow_dir, &req.task_id).ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    // Validate dependencies unless --force
    if !req.force {
        for dep in &task.depends_on {
            let dep_task = load_task(flow_dir, dep).ok_or_else(|| {
                ServiceError::DependencyUnsatisfied {
                    task: req.task_id.clone(),
                    dependency: format!("{} not found", dep),
                }
            })?;
            if !dep_task.status.is_satisfied() {
                return Err(ServiceError::DependencyUnsatisfied {
                    task: req.task_id.clone(),
                    dependency: format!("{} is '{}', not 'done'", dep, dep_task.status),
                });
            }
        }
    }

    let existing_rt = get_runtime(flow_dir, &req.task_id);
    let existing_assignee = existing_rt.as_ref().and_then(|rt| rt.assignee.clone());

    // Validate state machine transition (unless --force)
    if !req.force && !Transition::is_valid(task.status, Status::InProgress) {
        return Err(ServiceError::InvalidTransition(format!(
            "Cannot start task {}: invalid transition '{}' → 'in_progress'",
            req.task_id, task.status
        )));
    }

    // Check if claimed by someone else
    if !req.force {
        if let Some(ref assignee) = existing_assignee {
            if assignee != &req.actor {
                return Err(ServiceError::CrossActorViolation(format!(
                    "Cannot start task {}: claimed by '{}'",
                    req.task_id, assignee
                )));
            }
        }
    }

    // Validate task is in todo status (unless --force or resuming own task)
    if !req.force && task.status != Status::Todo {
        let can_resume = task.status == Status::InProgress
            && existing_assignee
                .as_ref()
                .map(|a| a == &req.actor)
                .unwrap_or(false);
        if !can_resume {
            return Err(ServiceError::InvalidTransition(format!(
                "Cannot start task {}: status is '{}', expected 'todo'",
                req.task_id, task.status
            )));
        }
    }

    // Build runtime state
    let now = Utc::now();
    let force_takeover = req.force
        && existing_assignee
            .as_ref()
            .map(|a| a != &req.actor)
            .unwrap_or(false);
    let new_assignee = if existing_assignee.is_none() || force_takeover {
        req.actor.clone()
    } else {
        existing_assignee
            .clone()
            .unwrap_or_else(|| req.actor.clone())
    };

    let claimed_at = if existing_rt
        .as_ref()
        .and_then(|rt| rt.claimed_at)
        .is_some()
        && !force_takeover
    {
        existing_rt.as_ref().expect("existing_rt verified as Some above").claimed_at
    } else {
        Some(now)
    };

    let task_state = crate::json_store::TaskState {
        status: Status::InProgress,
        assignee: Some(new_assignee),
        claimed_at,
        completed_at: None,
        evidence: None,
        blocked_reason: None,
        duration_seconds: None,
        baseline_rev: existing_rt
            .as_ref()
            .and_then(|rt| rt.baseline_rev.clone()),
        final_rev: None,
        retry_count: existing_rt
            .as_ref()
            .map(|rt| rt.retry_count)
            .unwrap_or(0),
        updated_at: Utc::now(),
    };

    crate::json_store::state_write(flow_dir, &req.task_id, &task_state)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;

    log_audit_event(flow_dir, &req.task_id, "task_started");
    emit_task_event(flow_dir, &req.task_id, "started", "flowctl start");

    Ok(StartTaskResponse {
        task_id: req.task_id,
        status: Status::InProgress,
    })
}

/// Complete a task: validate status/actor, collect evidence, update state.
pub fn done_task(
    flow_dir: &Path,
    req: DoneTaskRequest,
) -> ServiceResult<DoneTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(flow_dir, &req.task_id).ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    let runtime = get_runtime(flow_dir, &req.task_id);

    // 1. Validate status + actor
    validate_done_request(&task, &runtime, &req)?;

    // 2. Validate summary file is readable
    if let Some(ref file) = req.summary_file {
        fs::read_to_string(file).map_err(|e| {
            ServiceError::IoError(std::io::Error::new(e.kind(), format!("Cannot read summary file: {}", e)))
        })?;
    }

    // 3. Parse evidence
    let evidence_obj = parse_evidence(&req)?;

    // 4. Compute duration
    let duration_seconds = compute_duration(&runtime);

    // 5. Validate workspace_changes if present
    let ws_warning = validate_workspace_changes(&evidence_obj);

    // 6. Extract evidence lists and write state
    let to_list = |val: Option<&serde_json::Value>| -> Vec<String> {
        match val {
            None => Vec::new(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Some(serde_json::Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    };

    let commits = to_list(evidence_obj.get("commits"));
    let tests = to_list(evidence_obj.get("tests"));
    let prs = to_list(evidence_obj.get("prs"));

    let now = Utc::now();
    let ev = Evidence {
        commits: commits.clone(),
        tests: tests.clone(),
        prs: prs.clone(),
        ..Evidence::default()
    };
    let task_state = crate::json_store::TaskState {
        status: Status::Done,
        assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
        claimed_at: runtime.as_ref().and_then(|r| r.claimed_at),
        completed_at: Some(now),
        evidence: Some(ev),
        blocked_reason: None,
        duration_seconds,
        baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
        final_rev: runtime.as_ref().and_then(|r| r.final_rev.clone()),
        retry_count: runtime.as_ref().map(|r| r.retry_count).unwrap_or(0),
        updated_at: now,
    };
    crate::json_store::state_write(flow_dir, &req.task_id, &task_state)
        .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;

    // 7. Archive review receipt
    archive_review_receipt(flow_dir, &req.task_id, &evidence_obj);

    // 8. Audit event
    log_audit_event(flow_dir, &req.task_id, "task_completed");
    emit_task_event(flow_dir, &req.task_id, "completed", "flowctl done");

    Ok(DoneTaskResponse {
        task_id: req.task_id,
        status: Status::Done,
        duration_seconds,
        ws_warning,
    })
}

/// Validate workspace_changes in evidence, returning a warning if invalid.
fn validate_workspace_changes(evidence_obj: &serde_json::Value) -> Option<String> {
    let wc = evidence_obj.get("workspace_changes")?;
    if !wc.is_object() {
        return Some("workspace_changes must be an object".to_string());
    }
    let required = [
        "baseline_rev",
        "final_rev",
        "files_changed",
        "insertions",
        "deletions",
    ];
    let missing: Vec<&str> = required
        .iter()
        .filter(|k| !wc.as_object().expect("wc confirmed as object above").contains_key(**k))
        .copied()
        .collect();
    if !missing.is_empty() {
        Some(format!(
            "workspace_changes missing keys: {}",
            missing.join(", ")
        ))
    } else {
        None
    }
}

/// Block a task: validate status, read reason, update state.
pub fn block_task(
    flow_dir: &Path,
    req: BlockTaskRequest,
) -> ServiceResult<BlockTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(flow_dir, &req.task_id).ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    if task.status == Status::Done {
        return Err(ServiceError::InvalidTransition(format!(
            "Cannot block task {}: status is 'done'",
            req.task_id
        )));
    }

    let reason = req.reason.trim().to_string();

    if reason.is_empty() {
        return Err(ServiceError::ValidationError(
            "Reason file is empty".to_string(),
        ));
    }

    // Write to JSON state file
    {
        let existing = crate::json_store::state_read(flow_dir, &req.task_id).ok();
        let task_state = crate::json_store::TaskState {
            status: Status::Blocked,
            assignee: existing.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: existing.as_ref().and_then(|r| r.claimed_at),
            completed_at: None,
            evidence: existing.as_ref().and_then(|r| r.evidence.clone()),
            blocked_reason: Some(reason.clone()),
            duration_seconds: None,
            baseline_rev: existing.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: None,
            retry_count: existing.as_ref().map(|r| r.retry_count).unwrap_or(0),
            updated_at: Utc::now(),
        };
        crate::json_store::state_write(flow_dir, &req.task_id, &task_state)
            .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;
    }

    emit_task_event(flow_dir, &req.task_id, "blocked", "flowctl block");

    Ok(BlockTaskResponse {
        task_id: req.task_id,
        status: Status::Blocked,
    })
}

/// Fail a task: check retries, propagate upstream failure, update state.
pub fn fail_task(
    flow_dir: &Path,
    req: FailTaskRequest,
) -> ServiceResult<FailTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(flow_dir, &req.task_id).ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    if !req.force && task.status != Status::InProgress {
        return Err(ServiceError::InvalidTransition(format!(
            "Task {} is '{}', not 'in_progress'",
            req.task_id, task.status
        )));
    }

    let runtime = get_runtime(flow_dir, &req.task_id);
    let reason_text = req.reason.unwrap_or_else(|| "Task failed".to_string());

    let config = load_config(flow_dir);
    let (final_status, upstream_failed_ids) =
        handle_task_failure(flow_dir, &req.task_id, &runtime, config.as_ref())
            .map_err(ServiceError::IoError)?;

    emit_task_event(flow_dir, &req.task_id, "failed", "flowctl fail");

    let max_retries = get_max_retries_from_config(config.as_ref());
    let retry_count = if final_status == Status::UpForRetry {
        Some(runtime.as_ref().map(|r| r.retry_count).unwrap_or(0) + 1)
    } else {
        None
    };

    Ok(FailTaskResponse {
        task_id: req.task_id,
        final_status,
        reason: reason_text,
        upstream_failed_ids,
        retry_count,
        max_retries: if max_retries > 0 {
            Some(max_retries)
        } else {
            None
        },
    })
}

/// Restart a task and cascade to all downstream dependents.
pub fn restart_task(
    flow_dir: &Path,
    req: RestartTaskRequest,
) -> ServiceResult<RestartTaskResponse> {
    validate_task_id(&req.task_id)?;

    let _task = load_task(flow_dir, &req.task_id).ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    // Check epic not closed
    if let Ok(epic_id) = epic_id_from_task(&req.task_id) {
        if let Some(epic) = load_epic(flow_dir, &epic_id) {
            if epic.status == EpicStatus::Done {
                return Err(ServiceError::ValidationError(format!(
                    "Cannot restart task in closed epic {}",
                    epic_id
                )));
            }
        }
    }

    // Find all downstream dependents
    let dependents = find_dependents(flow_dir, &req.task_id);

    // Check for in_progress tasks
    let mut in_progress_ids = Vec::new();
    if _task.status == Status::InProgress {
        in_progress_ids.push(req.task_id.clone());
    }
    for dep_id in &dependents {
        if let Some(dep_task) = load_task(flow_dir, dep_id) {
            if dep_task.status == Status::InProgress {
                in_progress_ids.push(dep_id.clone());
            }
        }
    }

    if !in_progress_ids.is_empty() && !req.force {
        return Err(ServiceError::ValidationError(format!(
            "Cannot restart: tasks in progress: {}. Use --force to override.",
            in_progress_ids.join(", ")
        )));
    }

    // Build full reset list
    let all_ids: Vec<String> = std::iter::once(req.task_id.clone())
        .chain(dependents.iter().cloned())
        .collect();
    let mut to_reset = Vec::new();
    let mut skipped = Vec::new();

    for tid in &all_ids {
        let t = match load_task(flow_dir, tid) {
            Some(t) => t,
            None => continue,
        };
        if t.status == Status::Todo {
            skipped.push(tid.clone());
        } else {
            to_reset.push(tid.clone());
        }
    }

    // Dry-run mode: return what would happen without changing anything
    if req.dry_run {
        return Ok(RestartTaskResponse {
            cascade_from: req.task_id,
            reset_ids: to_reset,
            skipped_ids: skipped,
            in_progress_overridden: if req.force {
                in_progress_ids
            } else {
                Vec::new()
            },
        });
    }

    // Execute reset
    let mut reset_ids = Vec::new();
    for tid in &to_reset {
        let blank = crate::json_store::TaskState::default();
        crate::json_store::state_write(flow_dir, tid, &blank)
            .map_err(|e| ServiceError::IoError(std::io::Error::other(e.to_string())))?;

        reset_ids.push(tid.clone());
    }

    emit_task_event(flow_dir, &req.task_id, "started", "flowctl restart");

    Ok(RestartTaskResponse {
        cascade_from: req.task_id,
        reset_ids,
        skipped_ids: skipped,
        in_progress_overridden: if req.force {
            in_progress_ids
        } else {
            Vec::new()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::Status;
    use crate::types::{Domain, RuntimeState, Task};

    fn make_task(id: &str, status: Status) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: "fn-1".to_string(),
            title: format!("Task {id}"),
            status,
            priority: None,
            domain: Domain::General,
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

    fn make_done_req(task_id: &str, actor: &str) -> DoneTaskRequest {
        DoneTaskRequest {
            task_id: task_id.to_string(),
            summary: None,
            summary_file: None,
            evidence_json: None,
            evidence_inline: None,
            force: false,
            actor: actor.to_string(),
        }
    }

    #[test]
    fn test_validate_done_request_in_progress_ok() {
        let task = make_task("fn-1.1", Status::InProgress);
        let rt = Some(RuntimeState {
            task_id: "fn-1.1".to_string(),
            assignee: Some("alice".to_string()),
            claimed_at: Some(Utc::now()),
            completed_at: None,
            duration_secs: None,
            blocked_reason: None,
            baseline_rev: None,
            final_rev: None,
            retry_count: 0,
        });
        let req = make_done_req("fn-1.1", "alice");
        assert!(validate_done_request(&task, &rt, &req).is_ok());
    }

    #[test]
    fn test_validate_done_request_already_done() {
        let task = make_task("fn-1.1", Status::Done);
        let req = make_done_req("fn-1.1", "alice");
        let err = validate_done_request(&task, &None, &req).unwrap_err();
        assert!(matches!(err, ServiceError::InvalidTransition(_)));
    }

    #[test]
    fn test_validate_done_request_wrong_actor() {
        let task = make_task("fn-1.1", Status::InProgress);
        let rt = Some(RuntimeState {
            task_id: "fn-1.1".to_string(),
            assignee: Some("bob".to_string()),
            claimed_at: None,
            completed_at: None,
            duration_secs: None,
            blocked_reason: None,
            baseline_rev: None,
            final_rev: None,
            retry_count: 0,
        });
        let req = make_done_req("fn-1.1", "alice");
        let err = validate_done_request(&task, &rt, &req).unwrap_err();
        assert!(matches!(err, ServiceError::CrossActorViolation(_)));
    }

    #[test]
    fn test_validate_done_request_force_bypasses() {
        let task = make_task("fn-1.1", Status::Todo);
        let mut req = make_done_req("fn-1.1", "alice");
        req.force = true;
        assert!(validate_done_request(&task, &None, &req).is_ok());
    }

    #[test]
    fn test_parse_evidence_default() {
        let req = make_done_req("fn-1.1", "alice");
        let ev = parse_evidence(&req).unwrap();
        assert!(ev.is_object());
        assert!(ev.get("commits").unwrap().is_array());
    }

    #[test]
    fn test_parse_evidence_json_string() {
        let mut req = make_done_req("fn-1.1", "alice");
        req.evidence_json = Some(r#"{"commits":["abc"],"tests":[]}"#.to_string());
        let ev = parse_evidence(&req).unwrap();
        assert_eq!(ev["commits"][0], "abc");
    }

    #[test]
    fn test_parse_evidence_invalid_json() {
        let mut req = make_done_req("fn-1.1", "alice");
        req.evidence_json = Some("not json".to_string());
        assert!(parse_evidence(&req).is_err());
    }

    #[test]
    fn test_parse_evidence_not_object() {
        let mut req = make_done_req("fn-1.1", "alice");
        req.evidence_json = Some(r#"[1,2,3]"#.to_string());
        assert!(parse_evidence(&req).is_err());
    }

    #[test]
    fn test_compute_duration_none() {
        assert!(compute_duration(&None).is_none());
    }

    #[test]
    fn test_compute_duration_some() {
        let rt = Some(RuntimeState {
            task_id: "fn-1.1".to_string(),
            assignee: None,
            claimed_at: Some(Utc::now() - chrono::Duration::seconds(120)),
            completed_at: None,
            duration_secs: None,
            blocked_reason: None,
            baseline_rev: None,
            final_rev: None,
            retry_count: 0,
        });
        let dur = compute_duration(&rt).unwrap();
        assert!(dur >= 119 && dur <= 121);
    }

    #[test]
    fn test_validate_workspace_changes_none() {
        let ev = serde_json::json!({"commits": []});
        assert!(validate_workspace_changes(&ev).is_none());
    }

    #[test]
    fn test_validate_workspace_changes_not_object() {
        let ev = serde_json::json!({"workspace_changes": "bad"});
        assert_eq!(
            validate_workspace_changes(&ev).unwrap(),
            "workspace_changes must be an object"
        );
    }

    #[test]
    fn test_validate_workspace_changes_missing_keys() {
        let ev = serde_json::json!({"workspace_changes": {"baseline_rev": "abc"}});
        let warning = validate_workspace_changes(&ev).unwrap();
        assert!(warning.contains("missing keys"));
        assert!(warning.contains("final_rev"));
    }

    #[test]
    fn test_validate_workspace_changes_complete() {
        let ev = serde_json::json!({
            "workspace_changes": {
                "baseline_rev": "a",
                "final_rev": "b",
                "files_changed": 1,
                "insertions": 10,
                "deletions": 5
            }
        });
        assert!(validate_workspace_changes(&ev).is_none());
    }

    #[test]
    fn test_archive_review_receipt_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let flow_dir = tmp.path();
        let ev = serde_json::json!({
            "review_receipt": {
                "type": "impl",
                "mode": "rp",
                "verdict": "SHIP"
            }
        });
        archive_review_receipt(flow_dir, "fn-1.1", &ev);
        let path = flow_dir.join(REVIEWS_DIR).join("impl-fn-1.1-rp.json");
        assert!(path.exists());
    }

    #[test]
    fn test_archive_review_receipt_no_receipt() {
        let tmp = tempfile::tempdir().unwrap();
        let ev = serde_json::json!({"commits": []});
        archive_review_receipt(tmp.path(), "fn-1.1", &ev);
        assert!(!tmp.path().join(REVIEWS_DIR).exists());
    }

    #[test]
    fn test_get_max_retries_from_config_none() {
        assert_eq!(get_max_retries_from_config(None), 0);
    }

    #[test]
    fn test_get_max_retries_from_config_present() {
        let config = serde_json::json!({"max_retries": 3});
        assert_eq!(get_max_retries_from_config(Some(&config)), 3);
    }

    #[test]
    fn test_load_config_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(load_config(tmp.path()).is_none());
    }

    #[test]
    fn test_load_config_valid() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("config.json"), r#"{"max_retries": 2}"#).unwrap();
        let config = load_config(tmp.path()).unwrap();
        assert_eq!(config["max_retries"], 2);
    }
}
