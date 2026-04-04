//! Lifecycle commands: start, done, block, fail, restart.

use std::fs;

use chrono::Utc;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::frontmatter;
use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::state_machine::{Status, Transition};
use flowctl_core::types::{
    EpicStatus, Evidence, RuntimeState, Task, REVIEWS_DIR, TASKS_DIR,
};

use super::{
    ensure_flow_exists, find_dependents, get_max_retries, get_md_section, get_runtime,
    handle_task_failure, load_epic, load_task, patch_md_section, resolve_actor, try_open_db,
};

pub fn cmd_start(json_mode: bool, id: String, force: bool, _note: Option<String>) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Validate dependencies unless --force
    if !force {
        for dep in &task.depends_on {
            let dep_task = match load_task(&flow_dir, dep) {
                Some(t) => t,
                None => error_exit(&format!(
                    "Cannot start task {}: dependency {} not found",
                    id, dep
                )),
            };
            if !dep_task.status.is_satisfied() {
                error_exit(&format!(
                    "Cannot start task {}: dependency {} is '{}', not 'done'. \
                     Complete dependencies first or use --force to override.",
                    id, dep, dep_task.status
                ));
            }
        }
    }

    let current_actor = resolve_actor();
    let existing_rt = get_runtime(&id);
    let existing_assignee = existing_rt.as_ref().and_then(|rt| rt.assignee.clone());

    // Validate state machine transition (unless --force)
    if !force && !Transition::is_valid(task.status, Status::InProgress) {
        error_exit(&format!(
            "Cannot start task {}: invalid transition '{}' → 'in_progress'. Use --force to override.",
            id, task.status
        ));
    }

    // Check if claimed by someone else
    if !force {
        if let Some(ref assignee) = existing_assignee {
            if assignee != &current_actor {
                error_exit(&format!(
                    "Cannot start task {}: claimed by '{}'. Use --force to override.",
                    id, assignee
                ));
            }
        }
    }

    // Validate task is in todo status (unless --force or resuming own task)
    if !force && task.status != Status::Todo {
        let can_resume = task.status == Status::InProgress
            && existing_assignee
                .as_ref()
                .map(|a| a == &current_actor)
                .unwrap_or(false);
        if !can_resume {
            error_exit(&format!(
                "Cannot start task {}: status is '{}', expected 'todo'. Use --force to override.",
                id, task.status
            ));
        }
    }

    // Build runtime state
    let now = Utc::now();
    let force_takeover = force
        && existing_assignee
            .as_ref()
            .map(|a| a != &current_actor)
            .unwrap_or(false);
    let new_assignee = if existing_assignee.is_none() || force_takeover {
        current_actor.clone()
    } else {
        existing_assignee.clone().unwrap_or_else(|| current_actor.clone())
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
        task_id: id.clone(),
        assignee: Some(new_assignee),
        claimed_at,
        completed_at: None,
        duration_secs: None,
        blocked_reason: None,
        baseline_rev: existing_rt.as_ref().and_then(|rt| rt.baseline_rev.clone()),
        final_rev: None,
        retry_count: existing_rt.as_ref().map(|rt| rt.retry_count).unwrap_or(0),
    };

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        if let Err(e) = task_repo.update_status(&id, Status::InProgress) {
            error_exit(&format!("Failed to update task status: {}", e));
        }
        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        if let Err(e) = runtime_repo.upsert(&runtime_state) {
            error_exit(&format!("Failed to update runtime state: {}", e));
        }
    }

    // Update Markdown frontmatter
    let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_path.exists() {
        if let Ok(content) = fs::read_to_string(&task_path) {
            if let Ok(mut doc) = frontmatter::parse::<Task>(&content) {
                doc.frontmatter.status = Status::InProgress;
                doc.frontmatter.updated_at = now;
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_path, new_content);
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "in_progress",
            "message": format!("Task {} started", id),
        }));
    } else {
        println!("Task {} started", id);
    }
}

pub fn cmd_done(
    json_mode: bool,
    id: String,
    summary_file: Option<String>,
    summary: Option<String>,
    evidence_json: Option<String>,
    evidence: Option<String>,
    force: bool,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Require in_progress status (unless --force)
    if !force {
        match task.status {
            Status::InProgress => {}
            Status::Done => error_exit(&format!("Task {} is already done.", id)),
            other => error_exit(&format!(
                "Task {} is '{}', not 'in_progress'. Use --force to override.",
                id, other
            )),
        }
    }

    // Prevent cross-actor completion (unless --force)
    let current_actor = resolve_actor();
    let runtime = get_runtime(&id);
    if !force {
        if let Some(ref rt) = runtime {
            if let Some(ref assignee) = rt.assignee {
                if assignee != &current_actor {
                    error_exit(&format!(
                        "Cannot complete task {}: claimed by '{}'. Use --force to override.",
                        id, assignee
                    ));
                }
            }
        }
    }

    // Get summary
    let summary_text = if let Some(ref file) = summary_file {
        match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => error_exit(&format!("Cannot read summary file: {}", e)),
        }
    } else if let Some(ref s) = summary {
        s.clone()
    } else {
        "- Task completed".to_string()
    };

    // Get evidence
    let evidence_obj: serde_json::Value = if let Some(ref ev) = evidence_json {
        let raw = if ev.trim().starts_with('{') {
            ev.clone()
        } else {
            match fs::read_to_string(ev) {
                Ok(s) => s,
                Err(e) => error_exit(&format!("Cannot read evidence file: {}", e)),
            }
        };
        match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => error_exit(&format!("Evidence JSON invalid: {}", e)),
        }
    } else if let Some(ref ev) = evidence {
        match serde_json::from_str(ev) {
            Ok(v) => v,
            Err(e) => error_exit(&format!("Evidence invalid JSON: {}", e)),
        }
    } else {
        json!({"commits": [], "tests": [], "prs": []})
    };

    if !evidence_obj.is_object() {
        error_exit("Evidence JSON must be an object with keys: commits/tests/prs");
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

    // Format evidence as markdown
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

    let mut evidence_md = Vec::new();
    if commits.is_empty() {
        evidence_md.push("- Commits:".to_string());
    } else {
        evidence_md.push(format!("- Commits: {}", commits.join(", ")));
    }
    if tests.is_empty() {
        evidence_md.push("- Tests:".to_string());
    } else {
        evidence_md.push(format!("- Tests: {}", tests.join(", ")));
    }
    if prs.is_empty() {
        evidence_md.push("- PRs:".to_string());
    } else {
        evidence_md.push(format!("- PRs: {}", prs.join(", ")));
    }

    if ws_warning.is_none() {
        if let Some(wc) = ws_changes {
            if wc.is_object() {
                let fc = wc.get("files_changed").and_then(|v| v.as_u64()).unwrap_or(0);
                let ins = wc.get("insertions").and_then(|v| v.as_u64()).unwrap_or(0);
                let del = wc.get("deletions").and_then(|v| v.as_u64()).unwrap_or(0);
                let br = wc
                    .get("baseline_rev")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let fr = wc
                    .get("final_rev")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                evidence_md.push(format!(
                    "- Workspace: {} files changed, +{} -{} ({}..{})",
                    fc,
                    ins,
                    del,
                    &br[..br.len().min(7)],
                    &fr[..fr.len().min(7)]
                ));
            }
        }
    }

    if let Some(dur) = duration_seconds {
        let mins = dur / 60;
        let secs = dur % 60;
        let dur_str = if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        };
        evidence_md.push(format!("- Duration: {}", dur_str));
    }
    let evidence_content = evidence_md.join("\n");

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        let _ = task_repo.update_status(&id, Status::Done);

        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        let now = Utc::now();
        let rt = RuntimeState {
            task_id: id.clone(),
            assignee: runtime.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: runtime.as_ref().and_then(|r| r.claimed_at),
            completed_at: Some(now),
            duration_secs: duration_seconds,
            blocked_reason: None,
            baseline_rev: runtime.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: runtime.as_ref().and_then(|r| r.final_rev.clone()),
            retry_count: runtime.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt);

        // Store evidence
        let ev = Evidence {
            commits: commits.clone(),
            tests: tests.clone(),
            prs: prs.clone(),
            ..Evidence::default()
        };
        let evidence_repo = flowctl_db::EvidenceRepo::new(&conn);
        let _ = evidence_repo.upsert(&id, &ev);
    }

    // Update Markdown spec
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(current_spec) = fs::read_to_string(&task_spec_path) {
            let mut updated = current_spec;
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &summary_text) {
                updated = patched;
            }
            if let Some(patched) = patch_md_section(&updated, "## Evidence", &evidence_content) {
                updated = patched;
            }

            // Update frontmatter status
            if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                doc.frontmatter.status = Status::Done;
                doc.frontmatter.updated_at = Utc::now();
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_spec_path, new_content);
                }
            } else {
                let _ = fs::write(&task_spec_path, updated);
            }
        }
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
            let filename = format!("{}-{}-{}.json", rtype, id, mode);
            if let Ok(content) = serde_json::to_string_pretty(receipt) {
                let _ = fs::write(reviews_dir.join(filename), content);
            }
        }
    }

    if json_mode {
        let mut result = json!({
            "id": id,
            "status": "done",
            "message": format!("Task {} completed", id),
        });
        if let Some(dur) = duration_seconds {
            result["duration_seconds"] = json!(dur);
        }
        if let Some(ref warn) = ws_warning {
            result["warning"] = json!(warn);
        }
        json_output(result);
    } else {
        let dur_str = duration_seconds.map(|dur| {
            let mins = dur / 60;
            let secs = dur % 60;
            if mins > 0 {
                format!(" ({}m {}s)", mins, secs)
            } else {
                format!(" ({}s)", secs)
            }
        });
        println!("Task {} completed{}", id, dur_str.unwrap_or_default());
        if let Some(warn) = ws_warning {
            println!("  warning: {}", warn);
        }
    }
}

pub fn cmd_block(json_mode: bool, id: String, reason_file: String) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    if task.status == Status::Done {
        error_exit(&format!("Cannot block task {}: status is 'done'.", id));
    }

    let reason = match fs::read_to_string(&reason_file) {
        Ok(s) => s.trim().to_string(),
        Err(e) => error_exit(&format!("Cannot read reason file: {}", e)),
    };

    if reason.is_empty() {
        error_exit("Reason file is empty");
    }

    // Write SQLite first (authoritative)
    if let Some(conn) = try_open_db() {
        let task_repo = flowctl_db::TaskRepo::new(&conn);
        let _ = task_repo.update_status(&id, Status::Blocked);

        let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
        let existing = runtime_repo.get(&id).ok().flatten();
        let rt = RuntimeState {
            task_id: id.clone(),
            assignee: existing.as_ref().and_then(|r| r.assignee.clone()),
            claimed_at: existing.as_ref().and_then(|r| r.claimed_at),
            completed_at: None,
            duration_secs: None,
            blocked_reason: Some(reason.clone()),
            baseline_rev: existing.as_ref().and_then(|r| r.baseline_rev.clone()),
            final_rev: None,
            retry_count: existing.as_ref().map(|r| r.retry_count).unwrap_or(0),
        };
        let _ = runtime_repo.upsert(&rt);
    }

    // Update Markdown spec
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(current_spec) = fs::read_to_string(&task_spec_path) {
            let existing_summary = get_md_section(&current_spec, "## Done summary");
            let new_summary = if existing_summary.is_empty()
                || existing_summary.to_lowercase() == "tbd"
            {
                format!("Blocked:\n{}", reason)
            } else {
                format!("{}\n\nBlocked:\n{}", existing_summary, reason)
            };

            let mut updated = current_spec;
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &new_summary) {
                updated = patched;
            }

            // Update frontmatter
            if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                doc.frontmatter.status = Status::Blocked;
                doc.frontmatter.updated_at = Utc::now();
                if let Ok(new_content) = frontmatter::write(&doc) {
                    let _ = fs::write(&task_spec_path, new_content);
                }
            } else {
                let _ = fs::write(&task_spec_path, updated);
            }
        }
    }

    if json_mode {
        json_output(json!({
            "id": id,
            "status": "blocked",
            "message": format!("Task {} blocked", id),
        }));
    } else {
        println!("Task {} blocked", id);
    }
}

pub fn cmd_fail(json_mode: bool, id: String, reason: Option<String>, force: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M (e.g., fn-1.2, fn-1-add-auth.2)",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    if !force && task.status != Status::InProgress {
        error_exit(&format!(
            "Task {} is '{}', not 'in_progress'. Use --force to override.",
            id, task.status
        ));
    }

    let runtime = get_runtime(&id);
    let reason_text = reason.unwrap_or_else(|| "Task failed".to_string());

    let (final_status, upstream_failed_ids) = handle_task_failure(&flow_dir, &id, &runtime);

    // Update Done summary with failure reason
    let task_spec_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", id));
    if task_spec_path.exists() {
        if let Ok(content) = fs::read_to_string(&task_spec_path) {
            let mut updated = content;
            let summary = format!("Failed:\n{}", reason_text);
            if let Some(patched) = patch_md_section(&updated, "## Done summary", &summary) {
                updated = patched;
            }
            // Frontmatter was already updated by handle_task_failure, just write body changes
            let _ = fs::write(&task_spec_path, updated);
        }
    }

    if json_mode {
        let mut result = json!({
            "id": id,
            "status": final_status.to_string(),
            "message": format!("Task {} {}", id, final_status),
            "reason": reason_text,
        });
        if !upstream_failed_ids.is_empty() {
            result["upstream_failed"] = json!(upstream_failed_ids);
        }
        json_output(result);
    } else {
        println!("Task {} {}", id, final_status);
        if final_status == Status::UpForRetry {
            let max = get_max_retries();
            let count = runtime.as_ref().map(|r| r.retry_count).unwrap_or(0) + 1;
            println!("  retry {}/{} — will be retried by scheduler", count, max);
        }
        if !upstream_failed_ids.is_empty() {
            println!(
                "  upstream_failed propagated to {} downstream task(s):",
                upstream_failed_ids.len()
            );
            for tid in &upstream_failed_ids {
                println!("    {}", tid);
            }
        }
    }
}

pub fn cmd_restart(json_mode: bool, id: String, dry_run: bool, force: bool) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(&id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            id
        ));
    }

    let task = match load_task(&flow_dir, &id) {
        Some(t) => t,
        None => error_exit(&format!("Task {} not found", id)),
    };

    // Check epic not closed
    if let Ok(epic_id) = epic_id_from_task(&id) {
        if let Some(epic) = load_epic(&flow_dir, &epic_id) {
            if epic.status == EpicStatus::Done {
                error_exit(&format!("Cannot restart task in closed epic {}", epic_id));
            }
        }
    }

    // Find all downstream dependents
    let dependents = find_dependents(&flow_dir, &id);

    // Check for in_progress tasks
    let mut in_progress_ids = Vec::new();
    if task.status == Status::InProgress {
        in_progress_ids.push(id.clone());
    }
    for dep_id in &dependents {
        if let Some(dep_task) = load_task(&flow_dir, dep_id) {
            if dep_task.status == Status::InProgress {
                in_progress_ids.push(dep_id.clone());
            }
        }
    }

    if !in_progress_ids.is_empty() && !force {
        error_exit(&format!(
            "Cannot restart: tasks in progress: {}. Use --force to override.",
            in_progress_ids.join(", ")
        ));
    }

    // Build full reset list
    let all_ids: Vec<String> = std::iter::once(id.clone())
        .chain(dependents.iter().cloned())
        .collect();
    let mut to_reset = Vec::new();
    let mut skipped = Vec::new();

    for tid in &all_ids {
        let t = match load_task(&flow_dir, tid) {
            Some(t) => t,
            None => continue,
        };
        if t.status == Status::Todo {
            skipped.push(tid.clone());
        } else {
            to_reset.push(tid.clone());
        }
    }

    // Dry-run mode
    if dry_run {
        if json_mode {
            json_output(json!({
                "dry_run": true,
                "would_reset": to_reset,
                "already_todo": skipped,
                "in_progress_overridden": if force { in_progress_ids.clone() } else { Vec::<String>::new() },
            }));
        } else {
            println!(
                "Dry run \u{2014} would restart {} task(s):",
                to_reset.len()
            );
            for tid in &to_reset {
                if let Some(t) = load_task(&flow_dir, tid) {
                    let marker = if in_progress_ids.contains(tid) {
                        " (force)"
                    } else {
                        ""
                    };
                    println!("  {}  {} -> todo{}", tid, t.status, marker);
                }
            }
            if !skipped.is_empty() {
                println!("Already todo: {}", skipped.join(", "));
            }
        }
        return;
    }

    // Execute reset
    let mut reset_ids = Vec::new();
    for tid in &to_reset {
        // Reset in SQLite
        if let Some(conn) = try_open_db() {
            let task_repo = flowctl_db::TaskRepo::new(&conn);
            let _ = task_repo.update_status(tid, Status::Todo);

            // Clear runtime state
            let runtime_repo = flowctl_db::RuntimeRepo::new(&conn);
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
            let _ = runtime_repo.upsert(&rt);
        }

        // Update Markdown frontmatter + clear evidence
        let task_path = flow_dir.join(TASKS_DIR).join(format!("{}.md", tid));
        if task_path.exists() {
            if let Ok(content) = fs::read_to_string(&task_path) {
                let mut updated = content;

                // Clear sections
                if let Some(patched) = patch_md_section(&updated, "## Done summary", "TBD") {
                    updated = patched;
                }
                if let Some(patched) = patch_md_section(&updated, "## Evidence", "TBD") {
                    updated = patched;
                }

                // Update frontmatter status
                if let Ok(mut doc) = frontmatter::parse::<Task>(&updated) {
                    doc.frontmatter.status = Status::Todo;
                    doc.frontmatter.updated_at = Utc::now();
                    if let Ok(new_content) = frontmatter::write(&doc) {
                        updated = new_content;
                    }
                }

                let _ = fs::write(&task_path, updated);
            }
        }

        reset_ids.push(tid.clone());
    }

    if json_mode {
        json_output(json!({
            "reset": reset_ids,
            "skipped": skipped,
            "cascade_from": id,
        }));
    } else if reset_ids.is_empty() {
        println!(
            "Nothing to restart \u{2014} {} and dependents already todo.",
            id
        );
    } else {
        let downstream_count =
            reset_ids.len() - if reset_ids.contains(&id) { 1 } else { 0 };
        println!(
            "Restarted from {} (cascade: {} downstream):\n",
            id, downstream_count
        );
        for tid in &reset_ids {
            let marker = if *tid == id { " (target)" } else { "" };
            println!("  {}  -> todo{}", tid, marker);
        }
    }
}
