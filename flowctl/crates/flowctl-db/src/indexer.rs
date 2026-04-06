//! Async reindex engine (port of flowctl-db::indexer for libSQL).
//!
//! Scans `.flow/` Markdown/JSON and rebuilds index tables via async
//! libSQL calls. Idempotent: running twice produces the same result.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use libsql::{params, Connection};
use tracing::{info, warn};

use flowctl_core::frontmatter;
use flowctl_core::id::{is_epic_id, is_task_id};
use flowctl_core::types::{Epic, Task};

use crate::error::DbError;
use crate::repo::{EpicRepo, TaskRepo};

/// Result of a reindex operation.
#[derive(Debug, Default)]
pub struct ReindexResult {
    pub epics_indexed: usize,
    pub tasks_indexed: usize,
    pub files_skipped: usize,
    pub runtime_states_migrated: usize,
    pub warnings: Vec<String>,
}

/// Perform a full reindex of `.flow/` Markdown files into libSQL.
pub async fn reindex(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: Option<&Path>,
) -> Result<ReindexResult, DbError> {
    let mut result = ReindexResult::default();

    // libSQL doesn't currently support BEGIN EXCLUSIVE; use BEGIN.
    conn.execute_batch("BEGIN").await?;

    let outcome = reindex_inner(conn, flow_dir, state_dir, &mut result).await;

    match outcome {
        Ok(()) => {
            conn.execute_batch("COMMIT").await?;
            info!(
                epics = result.epics_indexed,
                tasks = result.tasks_indexed,
                skipped = result.files_skipped,
                runtime = result.runtime_states_migrated,
                "reindex complete"
            );
            Ok(result)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK").await;
            Err(e)
        }
    }
}

async fn reindex_inner(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: Option<&Path>,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    disable_triggers(conn).await?;
    clear_indexed_tables(conn).await?;

    let epics_dir = flow_dir.join("epics");
    let indexed_epics = if epics_dir.is_dir() {
        index_epics(conn, &epics_dir, result).await?
    } else {
        HashMap::new()
    };

    let tasks_dir = flow_dir.join("tasks");
    if tasks_dir.is_dir() {
        index_tasks(conn, &tasks_dir, &indexed_epics, result).await?;
    }

    if let Some(sd) = state_dir {
        migrate_runtime_state(conn, sd, result).await?;
    }

    enable_triggers(conn).await?;
    Ok(())
}

async fn disable_triggers(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch("DROP TRIGGER IF EXISTS trg_daily_rollup;")
        .await?;
    Ok(())
}

async fn enable_triggers(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS trg_daily_rollup AFTER INSERT ON events
         WHEN NEW.event_type IN ('task_completed', 'task_failed', 'task_started')
         BEGIN
             INSERT INTO daily_rollup (day, epic_id, tasks_completed, tasks_failed, tasks_started)
             VALUES (DATE(NEW.timestamp), NEW.epic_id,
                     CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
                     CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
                     CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END)
             ON CONFLICT(day, epic_id) DO UPDATE SET
                 tasks_completed = tasks_completed +
                     CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
                 tasks_failed = tasks_failed +
                     CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
                 tasks_started = tasks_started +
                     CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END;
         END;",
    )
    .await?;
    Ok(())
}

async fn clear_indexed_tables(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        "DELETE FROM file_ownership;
         DELETE FROM task_deps;
         DELETE FROM epic_deps;
         DELETE FROM tasks;
         DELETE FROM epics;",
    )
    .await?;
    Ok(())
}

async fn index_epics(
    conn: &Connection,
    epics_dir: &Path,
    result: &mut ReindexResult,
) -> Result<HashMap<String, PathBuf>, DbError> {
    let repo = EpicRepo::new(conn.clone());
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    // .md files
    for path in read_files_with_ext(epics_dir, "md") {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_epic_id(stem) {
            let msg = format!("skipping non-epic file: {}", path.display());
            warn!("{}", msg);
            result.warnings.push(msg);
            result.files_skipped += 1;
            continue;
        }

        let doc: frontmatter::Document<Epic> = match frontmatter::parse(&content) {
            Ok(d) => d,
            Err(e) => {
                let msg = format!("invalid frontmatter in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };
        let mut epic = doc.frontmatter;
        let body = doc.body;

        if let Some(prev_path) = seen.get(&epic.id) {
            return Err(DbError::Constraint(format!(
                "duplicate epic ID '{}' in {} and {}",
                epic.id,
                prev_path.display(),
                path.display()
            )));
        }

        epic.file_path = Some(format!(
            "epics/{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        repo.upsert_with_body(&epic, &body).await?;
        seen.insert(epic.id.clone(), path.clone());
        result.epics_indexed += 1;
    }

    // .json files (Python legacy format)
    for path in read_files_with_ext(epics_dir, "json") {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_epic_id(stem) {
            result.files_skipped += 1;
            continue;
        }

        if seen.contains_key(stem) {
            continue;
        }

        let mut epic = match try_parse_json_epic(&content) {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("invalid JSON epic in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        epic.file_path = Some(format!(
            "epics/{}",
            path.file_name().unwrap().to_string_lossy()
        ));
        repo.upsert_with_body(&epic, "").await?;
        seen.insert(epic.id.clone(), path.clone());
        result.epics_indexed += 1;
    }

    Ok(seen)
}

async fn index_tasks(
    conn: &Connection,
    tasks_dir: &Path,
    indexed_epics: &HashMap<String, PathBuf>,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    let task_repo = TaskRepo::new(conn.clone());
    let mut seen: HashMap<String, PathBuf> = HashMap::new();

    for path in read_files_with_ext(tasks_dir, "md") {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                result.files_skipped += 1;
                continue;
            }
        };

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_task_id(stem) {
            let msg = format!("skipping non-task file: {}", path.display());
            warn!("{}", msg);
            result.warnings.push(msg);
            result.files_skipped += 1;
            continue;
        }

        let (mut task, body) = if content.starts_with("---") {
            match frontmatter::parse::<Task>(&content) {
                Ok(doc) => (doc.frontmatter, doc.body),
                Err(e) => {
                    let msg = format!("invalid frontmatter in {}: {e}", path.display());
                    warn!("{}", msg);
                    result.warnings.push(msg);
                    result.files_skipped += 1;
                    continue;
                }
            }
        } else {
            match try_parse_python_task_md(&content, stem) {
                Ok((t, b)) => (t, b),
                Err(e) => {
                    let msg =
                        format!("cannot parse Python-format task {}: {e}", path.display());
                    warn!("{}", msg);
                    result.warnings.push(msg);
                    result.files_skipped += 1;
                    continue;
                }
            }
        };

        if let Some(prev_path) = seen.get(&task.id) {
            return Err(DbError::Constraint(format!(
                "duplicate task ID '{}' in {} and {}",
                task.id,
                prev_path.display(),
                path.display()
            )));
        }

        if !indexed_epics.contains_key(&task.epic) {
            let msg = format!(
                "orphan task '{}' references non-existent epic '{}' (indexing anyway)",
                task.id, task.epic
            );
            warn!("{}", msg);
            result.warnings.push(msg);
            insert_placeholder_epic(conn, &task.epic).await?;
        }

        task.file_path = Some(format!(
            "tasks/{}",
            path.file_name().unwrap().to_string_lossy()
        ));

        task_repo.upsert_with_body(&task, &body).await?;
        seen.insert(task.id.clone(), path.clone());
        result.tasks_indexed += 1;
    }

    Ok(())
}

async fn insert_placeholder_epic(conn: &Connection, epic_id: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT OR IGNORE INTO epics (id, title, status, file_path, created_at, updated_at)
         VALUES (?1, ?2, 'open', '', datetime('now'), datetime('now'))",
        params![epic_id.to_string(), format!("[placeholder] {}", epic_id)],
    )
    .await?;
    Ok(())
}

async fn migrate_runtime_state(
    conn: &Connection,
    state_dir: &Path,
    result: &mut ReindexResult,
) -> Result<(), DbError> {
    let tasks_state_dir = state_dir.join("tasks");
    if !tasks_state_dir.is_dir() {
        return Ok(());
    }

    let entries = match fs::read_dir(&tasks_state_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.ends_with(".state.json") {
            continue;
        }

        let task_id = name.trim_end_matches(".state.json");
        if !is_task_id(task_id) {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("failed to read runtime state {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                continue;
            }
        };

        let state: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("invalid JSON in {}: {e}", path.display());
                warn!("{}", msg);
                result.warnings.push(msg);
                continue;
            }
        };

        conn.execute(
            "INSERT OR REPLACE INTO runtime_state
             (task_id, assignee, claimed_at, completed_at, duration_secs, blocked_reason, baseline_rev, final_rev)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                task_id.to_string(),
                state.get("assignee").and_then(|v| v.as_str()).map(String::from),
                state.get("claimed_at").and_then(|v| v.as_str()).map(String::from),
                state.get("completed_at").and_then(|v| v.as_str()).map(String::from),
                state
                    .get("duration_secs")
                    .or_else(|| state.get("duration_seconds"))
                    .and_then(|v| v.as_i64()),
                state.get("blocked_reason").and_then(|v| v.as_str()).map(String::from),
                state.get("baseline_rev").and_then(|v| v.as_str()).map(String::from),
                state.get("final_rev").and_then(|v| v.as_str()).map(String::from),
            ],
        )
        .await?;

        result.runtime_states_migrated += 1;
    }

    Ok(())
}

fn read_files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

fn try_parse_json_epic(content: &str) -> Result<Epic, String> {
    let v: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let obj = v.as_object().ok_or("not an object")?;

    let id = obj.get("id").and_then(|v| v.as_str()).ok_or("missing id")?;
    let title = obj.get("title").and_then(|v| v.as_str()).unwrap_or(id);
    let status_str = obj
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("open");
    let status = match status_str {
        "closed" | "done" => flowctl_core::types::EpicStatus::Done,
        _ => flowctl_core::types::EpicStatus::Open,
    };
    let branch_name = obj
        .get("branch_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let created_at = obj
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let updated_at = obj
        .get("updated_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(created_at);

    Ok(Epic {
        schema_version: 1,
        id: id.to_string(),
        title: title.to_string(),
        status,
        branch_name,
        plan_review: Default::default(),
        completion_review: Default::default(),
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        auto_execute_pending: None,
        auto_execute_set_at: None,
        archived: false,
        file_path: None,
        created_at,
        updated_at,
    })
}

fn try_parse_python_task_md(content: &str, filename_stem: &str) -> Result<(Task, String), String> {
    let first_line = content.lines().next().unwrap_or("");
    let title = if first_line.starts_with("# ") {
        let after_hash = first_line.trim_start_matches("# ");
        after_hash
            .split_once(' ')
            .map(|x| x.1)
            .unwrap_or(filename_stem)
            .to_string()
    } else {
        filename_stem.to_string()
    };

    let epic_id = flowctl_core::id::epic_id_from_task(filename_stem)
        .map_err(|e| format!("cannot extract epic from {}: {e}", filename_stem))?;

    let status = if content.contains("## Done summary") && !content.contains("## Done summary\nTBD")
    {
        flowctl_core::state_machine::Status::Done
    } else {
        flowctl_core::state_machine::Status::Todo
    };

    let body = content.lines().skip(1).collect::<Vec<_>>().join("\n");

    let task = Task {
        schema_version: 1,
        id: filename_stem.to_string(),
        epic: epic_id,
        title,
        status,
        priority: None,
        domain: flowctl_core::types::Domain::General,
        depends_on: vec![],
        files: vec![],
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("tasks/{}.md", filename_stem)),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    Ok((task, body))
}
