//! Epic CRUD handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use flowctl_core::id::slugify;

use flowctl_scheduler::FlowEvent;

use super::common::{AppError, AppState};

/// POST /api/v1/epics/create -- create a new epic.
pub async fn create_epic_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateEpicRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let title = body.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::InvalidInput("title is required".to_string()));
    }

    let conn = state.db.clone();

    // Determine next epic number from DB.
    let mut rows = conn
        .query(
            "SELECT COALESCE(MAX(CAST(SUBSTR(id, 4, INSTR(SUBSTR(id, 4), '-') - 1) AS INTEGER)), 0) FROM epics WHERE id LIKE 'fn-%'",
            (),
        )
        .await
        .map_err(|e| AppError::Db(e.to_string()))?;
    let max_num: i64 = match rows.next().await.map_err(|e| AppError::Db(e.to_string()))? {
        Some(row) => row.get::<i64>(0).unwrap_or(0),
        None => 0,
    };
    let epic_num = (max_num + 1) as u32;

    let slug = slugify(&title, 40).unwrap_or_else(|| format!("epic{epic_num}"));
    let epic_id = format!("fn-{epic_num}-{slug}");

    let epic = flowctl_core::types::Epic {
        schema_version: 1,
        id: epic_id.clone(),
        title: title.clone(),
        status: flowctl_core::types::EpicStatus::Open,
        branch_name: None,
        plan_review: flowctl_core::types::ReviewStatus::Unknown,
        completion_review: flowctl_core::types::ReviewStatus::Unknown,
        depends_on_epics: vec![],
        default_impl: None,
        default_review: None,
        default_sync: None,
        file_path: Some(format!("epics/{epic_id}.md")),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let repo = flowctl_db::EpicRepo::new(conn);
    repo.upsert(&epic).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "id": epic_id, "title": title})),
    ))
}

/// POST /api/v1/epics/:id/plan -- set epic plan text.
///
/// Writes the plan to the epic's markdown file in .flow/.
pub async fn set_epic_plan_handler(
    State(state): State<AppState>,
    axum::extract::Path(epic_id): axum::extract::Path<String>,
    Json(body): Json<SetPlanRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db::EpicRepo::new(conn.clone());

    // Verify epic exists.
    let epic = repo
        .get(&epic_id)
        .await
        .map_err(|_| AppError::NotFound(format!("epic not found: {epic_id}")))?;

    // Write plan to the epic's file in the .flow directory.
    let flow_dir = state
        .runtime
        .paths
        .state_dir
        .parent()
        .ok_or_else(|| AppError::Internal("cannot resolve .flow/ directory".to_string()))?;

    if let Some(ref file_path) = epic.file_path {
        let plan_path = flow_dir.join(file_path);
        if let Some(parent) = plan_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Internal(format!("failed to create dir: {e}")))?;
        }
        std::fs::write(&plan_path, &body.plan)
            .map_err(|e| AppError::Internal(format!("failed to write plan: {e}")))?;
    }

    // Touch updated_at.
    conn.execute(
        "UPDATE epics SET updated_at = ?1 WHERE id = ?2",
        libsql::params![chrono::Utc::now().to_rfc3339(), epic_id.clone()],
    )
    .await
    .map_err(|e| AppError::Db(e.to_string()))?;

    state.event_bus.emit(FlowEvent::EpicUpdated {
        epic_id: epic_id.clone(),
        field: "plan".to_string(),
        value: serde_json::Value::String("updated".to_string()),
    });

    Ok(Json(serde_json::json!({"id": epic_id})))
}

/// POST /api/v1/epics/:id/work -- start epic execution.
pub async fn start_epic_work_handler(
    State(state): State<AppState>,
    axum::extract::Path(epic_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let epic_repo = flowctl_db::EpicRepo::new(conn.clone());

    // Verify epic exists.
    let _epic = epic_repo
        .get(&epic_id)
        .await
        .map_err(|_| AppError::NotFound(format!("epic not found: {epic_id}")))?;

    // Precondition: epic must have tasks.
    let task_repo = flowctl_db::TaskRepo::new(conn);
    let tasks = task_repo.list_by_epic(&epic_id).await?;
    if tasks.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "epic {epic_id} has no tasks — cannot start work"
        )));
    }

    // Count tasks that are in todo status and start them.
    let todo_tasks: Vec<_> = tasks
        .iter()
        .filter(|t| matches!(t.status, flowctl_core::state_machine::Status::Todo))
        .collect();

    // Mark the first wave of ready tasks (those with no unsatisfied deps) as in_progress.
    let mut tasks_started = 0u32;
    for task in &todo_tasks {
        let deps_satisfied = task.depends_on.iter().all(|dep_id| {
            tasks.iter().any(|t| {
                t.id == *dep_id
                    && matches!(
                        t.status,
                        flowctl_core::state_machine::Status::Done
                            | flowctl_core::state_machine::Status::Skipped
                    )
            })
        });
        if deps_satisfied {
            task_repo
                .update_status(&task.id, flowctl_core::state_machine::Status::InProgress)
                .await?;
            tasks_started += 1;
        }
    }

    state.event_bus.emit(FlowEvent::EpicUpdated {
        epic_id: epic_id.clone(),
        field: "work_started".to_string(),
        value: serde_json::json!({"tasks_started": tasks_started}),
    });

    Ok(Json(serde_json::json!({
        "id": epic_id,
        "tasks_started": tasks_started
    })))
}

// ── Request types ─────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct CreateEpicRequest {
    pub title: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct SetPlanRequest {
    pub plan: String,
}
