//! Lifecycle service functions: start, done, block, fail, restart.
//!
//! These functions contain the business logic extracted from the CLI
//! lifecycle commands. Each accepts a request struct and returns
//! `ServiceResult<Response>`, using SQLite as the sole source of truth.

use std::fs;
use std::path::Path;

use chrono::Utc;
use libsql::Connection;

use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::state_machine::{Status, Transition};
use flowctl_core::types::{
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
async fn load_task(_conn: Option<&Connection>, flow_dir: &Path, task_id: &str) -> Option<Task> {
    flowctl_core::json_store::task_read(flow_dir, task_id).ok()
}

async fn load_epic(_conn: Option<&Connection>, flow_dir: &Path, epic_id: &str) -> Option<Epic> {
    flowctl_core::json_store::epic_read(flow_dir, epic_id).ok()
}

async fn get_runtime(conn: Option<&Connection>, _flow_dir: &Path, task_id: &str) -> Option<RuntimeState> {
    let conn = conn?;
    let repo = flowctl_db::RuntimeRepo::new(conn.clone());
    repo.get(task_id).await.ok().flatten()
}

/// Load all tasks for an epic from JSON files.
async fn load_tasks_for_epic(
    _conn: Option<&Connection>,
    flow_dir: &Path,
    epic_id: &str,
) -> std::collections::HashMap<String, Task> {
    use std::collections::HashMap;

    if let Ok(tasks) = flowctl_core::json_store::task_list_by_epic(flow_dir, epic_id) {
        let mut map = HashMap::new();
        for task in tasks {
            map.insert(task.id.clone(), task);
        }
        return map;
    }

    HashMap::new()
}

/// Find all downstream dependents of a task within the same epic.
async fn find_dependents(
    conn: Option<&Connection>,
    flow_dir: &Path,
    task_id: &str,
) -> Vec<String> {
    let epic_id = match epic_id_from_task(task_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(conn, flow_dir, &epic_id).await;
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

/// Read max_retries from .flow/config.json (defaults to 0 = no retries).
fn get_max_retries(flow_dir: &Path) -> u32 {
    let config_path = flow_dir.join("config.json");
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(max) = config.get("max_retries").and_then(|v| v.as_u64()) {
                return max as u32;
            }
        }
    }
    0
}

/// Propagate upstream_failed to all transitive downstream tasks.
async fn propagate_upstream_failure(
    conn: Option<&Connection>,
    flow_dir: &Path,
    failed_id: &str,
) -> Vec<String> {
    let epic_id = match epic_id_from_task(failed_id) {
        Ok(eid) => eid,
        Err(_) => return Vec::new(),
    };

    let tasks = load_tasks_for_epic(conn, flow_dir, &epic_id).await;
    let task_list: Vec<Task> = tasks.values().cloned().collect();

    let dag = match flowctl_core::TaskDag::from_tasks(&task_list) {
        Ok(d) => d,
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

        // Update DB (sole source of truth)
        if let Some(conn) = conn {
            let task_repo = flowctl_db::TaskRepo::new(conn.clone());
            let _ = task_repo.update_status(tid, Status::UpstreamFailed).await;
        }

        affected.push(tid.clone());
    }

    affected
}

/// Handle task failure: check retries, set up_for_retry or failed + propagate.
async fn handle_task_failure(
    conn: Option<&Connection>,
    flow_dir: &Path,
    task_id: &str,
    runtime: &Option<RuntimeState>,
) -> (Status, Vec<String>) {
    let max_retries = get_max_retries(flow_dir);
    let current_retry_count = runtime.as_ref().map(|r| r.retry_count).unwrap_or(0);

    if max_retries > 0 && current_retry_count < max_retries {
        let new_retry_count = current_retry_count + 1;

        if let Some(conn) = conn {
            let task_repo = flowctl_db::TaskRepo::new(conn.clone());
            let _ = task_repo.update_status(task_id, Status::UpForRetry).await;

            let runtime_repo = flowctl_db::RuntimeRepo::new(conn.clone());
            let rt = RuntimeState {
                task_id: task_id.to_string(),
                assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
                claimed_at: None,
                completed_at: None,
                duration_secs: None,
                blocked_reason: None,
                baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
                final_rev: None,
                retry_count: new_retry_count,
            };
            let _ = runtime_repo.upsert(&rt).await;
        }

        (Status::UpForRetry, Vec::new())
    } else {
        if let Some(conn) = conn {
            let task_repo = flowctl_db::TaskRepo::new(conn.clone());
            let _ = task_repo.update_status(task_id, Status::Failed).await;
        }

        let affected = propagate_upstream_failure(conn, flow_dir, task_id).await;
        (Status::Failed, affected)
    }
}

// ── Service functions ──────────────────────────────────────────────

/// Start a task: validate deps, state machine, actor, update DB + Markdown.
pub async fn start_task(
    conn: Option<&Connection>,
    flow_dir: &Path,
    req: StartTaskRequest,
) -> ServiceResult<StartTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(conn, flow_dir, &req.task_id).await.ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    // Validate dependencies unless --force
    if !req.force {
        for dep in &task.depends_on {
            let dep_task = load_task(conn, flow_dir, dep).await.ok_or_else(|| {
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

    let existing_rt = get_runtime(conn, flow_dir, &req.task_id).await;
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
        existing_rt.as_ref().unwrap().claimed_at
    } else {
        Some(now)
    };

    let runtime_state = RuntimeState {
        task_id: req.task_id.clone(),
        assignee: Some(new_assignee),
        claimed_at,
        completed_at: None,
        duration_secs: None,
        blocked_reason: None,
        baseline_rev: existing_rt
            .as_ref()
            .and_then(|rt| rt.baseline_rev.clone()),
        final_rev: None,
        retry_count: existing_rt
            .as_ref()
            .map(|rt| rt.retry_count)
            .unwrap_or(0),
    };

    // Write SQLite (authoritative)
    if let Some(conn) = conn {
        let task_repo = flowctl_db::TaskRepo::new(conn.clone());
        task_repo
            .update_status(&req.task_id, Status::InProgress)
            .await
            .map_err(ServiceError::from)?;
        let runtime_repo = flowctl_db::RuntimeRepo::new(conn.clone());
        runtime_repo
            .upsert(&runtime_state)
            .await
            .map_err(ServiceError::from)?;
    }

    Ok(StartTaskResponse {
        task_id: req.task_id,
        status: Status::InProgress,
    })
}

/// Complete a task: validate status/actor, collect evidence, update DB + Markdown.
pub async fn done_task(
    conn: Option<&Connection>,
    flow_dir: &Path,
    req: DoneTaskRequest,
) -> ServiceResult<DoneTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(conn, flow_dir, &req.task_id).await.ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    // Require in_progress status (unless --force)
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
    }

    // Prevent cross-actor completion (unless --force)
    let runtime = get_runtime(conn, flow_dir, &req.task_id).await;
    if !req.force {
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

    // Get summary (validate files are readable, even though no longer written to MD)
    if let Some(ref file) = req.summary_file {
        fs::read_to_string(file).map_err(|e| {
            ServiceError::IoError(std::io::Error::new(e.kind(), format!("Cannot read summary file: {}", e)))
        })?;
    }

    // Get evidence
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

    // Calculate duration from claimed_at
    let duration_seconds: Option<u64> = runtime
        .as_ref()
        .and_then(|rt| rt.claimed_at)
        .map(|start| {
            let dur = Utc::now() - start;
            dur.num_seconds().max(0) as u64
        });

    // Validate workspace_changes if present
    let ws_changes = evidence_obj.get("workspace_changes");
    let mut ws_warning: Option<String> = None;
    if let Some(wc) = ws_changes {
        if !wc.is_object() {
            ws_warning = Some("workspace_changes must be an object".to_string());
        } else {
            let required = [
                "baseline_rev",
                "final_rev",
                "files_changed",
                "insertions",
                "deletions",
            ];
            let missing: Vec<&str> = required
                .iter()
                .filter(|k| !wc.as_object().unwrap().contains_key(**k))
                .copied()
                .collect();
            if !missing.is_empty() {
                ws_warning = Some(format!(
                    "workspace_changes missing keys: {}",
                    missing.join(", ")
                ));
            }
        }
    }

    // Extract evidence lists for DB storage
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

    // Write SQLite (sole source of truth)
    if let Some(conn) = conn {
        let task_repo = flowctl_db::TaskRepo::new(conn.clone());
        let _ = task_repo.update_status(&req.task_id, Status::Done).await;

        let runtime_repo = flowctl_db::RuntimeRepo::new(conn.clone());
        let now = Utc::now();
        let rt = RuntimeState {
            task_id: req.task_id.clone(),
            assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: runtime.as_ref().and_then(|r| r.claimed_at),
            completed_at: Some(now),
            duration_secs: duration_seconds,
            blocked_reason: None,
            baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: runtime.as_ref().and_then(|r| r.final_rev.clone()),
            retry_count: runtime.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt).await;

        let ev = Evidence {
            commits: commits.clone(),
            tests: tests.clone(),
            prs: prs.clone(),
            ..Evidence::default()
        };
        let evidence_repo = flowctl_db::EvidenceRepo::new(conn.clone());
        let _ = evidence_repo.upsert(&req.task_id, &ev).await;
    }

    // Archive review receipt if present
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
            let filename = format!("{}-{}-{}.json", rtype, req.task_id, mode);
            if let Ok(content) = serde_json::to_string_pretty(receipt) {
                let _ = fs::write(reviews_dir.join(filename), content);
            }
        }
    }

    Ok(DoneTaskResponse {
        task_id: req.task_id,
        status: Status::Done,
        duration_seconds,
        ws_warning,
    })
}

/// Block a task: validate status, read reason, update DB + Markdown.
pub async fn block_task(
    conn: Option<&Connection>,
    flow_dir: &Path,
    req: BlockTaskRequest,
) -> ServiceResult<BlockTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(conn, flow_dir, &req.task_id).await.ok_or_else(|| {
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

    // Write SQLite (authoritative)
    if let Some(conn) = conn {
        let task_repo = flowctl_db::TaskRepo::new(conn.clone());
        let _ = task_repo.update_status(&req.task_id, Status::Blocked).await;

        let runtime_repo = flowctl_db::RuntimeRepo::new(conn.clone());
        let existing = runtime_repo.get(&req.task_id).await.ok().flatten();
        let rt = RuntimeState {
            task_id: req.task_id.clone(),
            assignee: existing.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: existing.as_ref().and_then(|r| r.claimed_at),
            completed_at: None,
            duration_secs: None,
            blocked_reason: Some(reason.clone()),
            baseline_rev: existing.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: None,
            retry_count: existing.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt).await;
    }

    Ok(BlockTaskResponse {
        task_id: req.task_id,
        status: Status::Blocked,
    })
}

/// Fail a task: check retries, propagate upstream failure, update DB + Markdown.
pub async fn fail_task(
    conn: Option<&Connection>,
    flow_dir: &Path,
    req: FailTaskRequest,
) -> ServiceResult<FailTaskResponse> {
    validate_task_id(&req.task_id)?;

    let task = load_task(conn, flow_dir, &req.task_id).await.ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    if !req.force && task.status != Status::InProgress {
        return Err(ServiceError::InvalidTransition(format!(
            "Task {} is '{}', not 'in_progress'",
            req.task_id, task.status
        )));
    }

    let runtime = get_runtime(conn, flow_dir, &req.task_id).await;
    let reason_text = req.reason.unwrap_or_else(|| "Task failed".to_string());

    let (final_status, upstream_failed_ids) =
        handle_task_failure(conn, flow_dir, &req.task_id, &runtime).await;

    let max_retries = get_max_retries(flow_dir);
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
pub async fn restart_task(
    conn: Option<&Connection>,
    flow_dir: &Path,
    req: RestartTaskRequest,
) -> ServiceResult<RestartTaskResponse> {
    validate_task_id(&req.task_id)?;

    let _task = load_task(conn, flow_dir, &req.task_id).await.ok_or_else(|| {
        ServiceError::TaskNotFound(req.task_id.clone())
    })?;

    // Check epic not closed
    if let Ok(epic_id) = epic_id_from_task(&req.task_id) {
        if let Some(epic) = load_epic(conn, flow_dir, &epic_id).await {
            if epic.status == EpicStatus::Done {
                return Err(ServiceError::ValidationError(format!(
                    "Cannot restart task in closed epic {}",
                    epic_id
                )));
            }
        }
    }

    // Find all downstream dependents
    let dependents = find_dependents(conn, flow_dir, &req.task_id).await;

    // Check for in_progress tasks
    let mut in_progress_ids = Vec::new();
    if _task.status == Status::InProgress {
        in_progress_ids.push(req.task_id.clone());
    }
    for dep_id in &dependents {
        if let Some(dep_task) = load_task(conn, flow_dir, dep_id).await {
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
        let t = match load_task(conn, flow_dir, tid).await {
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
        if let Some(conn) = conn {
            let task_repo = flowctl_db::TaskRepo::new(conn.clone());
            let _ = task_repo.update_status(tid, Status::Todo).await;

            let runtime_repo = flowctl_db::RuntimeRepo::new(conn.clone());
            let rt = RuntimeState {
                task_id: tid.clone(),
                assignee: None,
                claimed_at: None,
                completed_at: None,
                duration_secs: None,
                blocked_reason: None,
                baseline_rev: None,
                final_rev: None,
                retry_count: 0,
            };
            let _ = runtime_repo.upsert(&rt).await;
        }

        reset_ids.push(tid.clone());
    }

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
