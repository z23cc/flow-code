//! Task lifecycle handlers: create, start, done, block, skip, restart.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use flowctl_core::id::{epic_id_from_task, is_task_id};
use flowctl_core::state_machine::{Status, Transition};
use flowctl_core::types::FLOW_DIR;
use flowctl_scheduler::FlowEvent;
use flowctl_service::lifecycle::{BlockTaskRequest, DoneTaskRequest, RestartTaskRequest, StartTaskRequest};

use super::common::{service_error_to_app_error, AppError, AppState};

fn flow_dir() -> std::path::PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(FLOW_DIR)
}

/// POST /api/v1/tasks/create -- create a new task.
pub async fn create_task_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Validate task ID format.
    if !is_task_id(&body.id) {
        return Err(AppError::InvalidInput(format!(
            "invalid task ID format: '{}'. Expected format: epic-id.N",
            body.id
        )));
    }

    let conn = state.db.clone();
    let task = flowctl_core::types::Task {
        schema_version: 1,
        id: body.id.clone(),
        epic: body.epic_id.clone(),
        title: body.title.clone(),
        status: Status::Todo,
        priority: None,
        domain: flowctl_core::types::Domain::General,
        depends_on: body.depends_on.unwrap_or_default(),
        files: vec![],
        r#impl: None,
        review: None,
        sync: None,
        file_path: Some(format!("tasks/{}.md", body.id)),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let repo = flowctl_db::TaskRepo::new(conn);
    repo.upsert_with_body(&task, &body.body.unwrap_or_default()).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"success": true, "id": body.id})),
    ))
}

/// POST /api/v1/tasks/start -- start a task via service layer.
pub async fn start_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = StartTaskRequest {
        task_id,
        force: false,
        actor: "daemon".to_string(),
    };

    match flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "todo".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(
                serde_json::json!({"success": true, "id": resp.task_id}),
            ))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/:id/start -- RESTful start a task.
pub async fn start_task_rest_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(body): Json<Option<StartTaskRestRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let body = body.unwrap_or_default();
    let force = body.force.unwrap_or(false);
    let actor = body.actor.unwrap_or_else(|| "daemon".to_string());
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = StartTaskRequest { task_id, force, actor };

    match flowctl_service::lifecycle::start_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "todo".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(serde_json::json!({
                "id": resp.task_id,
                "status": format!("{:?}", resp.status).to_lowercase()
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/:id/done -- RESTful complete a task.
pub async fn done_task_rest_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(body): Json<Option<DoneTaskRestRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let body = body.unwrap_or_default();
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = DoneTaskRequest {
        task_id,
        summary: body.summary,
        summary_file: None,
        evidence_json: body.evidence,
        evidence_inline: None,
        force: false,
        actor: "daemon".to_string(),
    };

    match flowctl_service::lifecycle::done_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "in_progress".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(serde_json::json!({
                "id": resp.task_id,
                "status": format!("{:?}", resp.status).to_lowercase(),
                "duration_seconds": resp.duration_seconds
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/:id/block -- RESTful block a task.
pub async fn block_task_rest_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(body): Json<BlockTaskRestRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let reason = body.reason;
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = BlockTaskRequest { task_id, reason };

    match flowctl_service::lifecycle::block_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "in_progress".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(serde_json::json!({
                "id": resp.task_id,
                "status": format!("{:?}", resp.status).to_lowercase()
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/:id/restart -- RESTful restart a task.
pub async fn restart_task_rest_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(body): Json<Option<RestartTaskRestRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let body = body.unwrap_or_default();
    let force = body.force.unwrap_or(true);
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = RestartTaskRequest {
        task_id,
        dry_run: false,
        force,
    };

    match flowctl_service::lifecycle::restart_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.cascade_from).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.cascade_from.clone(),
                epic_id,
                from_status: "done".to_string(),
                to_status: "todo".to_string(),
            });
            Ok(Json(serde_json::json!({
                "id": resp.cascade_from,
                "status": "todo",
                "cascaded": resp.reset_ids
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/:id/skip -- RESTful skip a task.
pub async fn skip_task_rest_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
    Json(_body): Json<SkipTaskRestRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db::TaskRepo::new(conn);
    let task = repo
        .get(&task_id)
        .await
        .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;

    let from_status = format!("{:?}", task.status).to_lowercase();
    Transition::new(task.status, Status::Skipped).map_err(|e| {
        AppError::InvalidTransition(format!("cannot skip task '{task_id}': {e}"))
    })?;

    repo.update_status(&task_id, Status::Skipped).await?;

    let epic_id = epic_id_from_task(&task_id).unwrap_or_default();
    state.event_bus.emit(FlowEvent::TaskStatusChanged {
        task_id: task_id.clone(),
        epic_id,
        from_status,
        to_status: "skipped".to_string(),
    });

    Ok(Json(serde_json::json!({
        "id": task_id,
        "status": "skipped"
    })))
}

/// POST /api/v1/tasks/done -- complete a task (legacy flat endpoint).
pub async fn done_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskDoneRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let summary = body.summary.clone();
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = DoneTaskRequest {
        task_id,
        summary,
        summary_file: None,
        evidence_json: None,
        evidence_inline: None,
        force: false,
        actor: "daemon".to_string(),
    };

    match flowctl_service::lifecycle::done_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "in_progress".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(
                serde_json::json!({"success": true, "id": resp.task_id}),
            ))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/skip -- skip a task (legacy flat endpoint).
pub async fn skip_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskReasonRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db::TaskRepo::new(conn);
    let task = repo
        .get(&body.task_id)
        .await
        .map_err(|_| AppError::NotFound(format!("task not found: {}", body.task_id)))?;

    let from_status = format!("{:?}", task.status).to_lowercase();
    Transition::new(task.status, Status::Skipped).map_err(|e| {
        AppError::InvalidTransition(format!("cannot skip task '{}': {}", body.task_id, e))
    })?;

    repo.update_status(&body.task_id, Status::Skipped).await?;

    let epic_id = epic_id_from_task(&body.task_id).unwrap_or_default();
    state.event_bus.emit(FlowEvent::TaskStatusChanged {
        task_id: body.task_id.clone(),
        epic_id,
        from_status,
        to_status: "skipped".to_string(),
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "id": body.task_id,
        "status": "skipped"
    })))
}

/// POST /api/v1/tasks/block -- block a task (legacy flat endpoint).
pub async fn block_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskReasonRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let reason = body.reason.clone();
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = BlockTaskRequest { task_id, reason };

    match flowctl_service::lifecycle::block_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.task_id).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.task_id.clone(),
                epic_id,
                from_status: "in_progress".to_string(),
                to_status: format!("{:?}", resp.status).to_lowercase(),
            });
            Ok(Json(serde_json::json!({
                "success": true,
                "id": resp.task_id,
                "status": "blocked"
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// POST /api/v1/tasks/restart -- restart a task (legacy flat endpoint).
pub async fn restart_task_handler(
    State(state): State<AppState>,
    Json(body): Json<TaskIdRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let task_id = body.task_id.clone();
    let conn = state.db.clone();
    let flow_dir = flow_dir();
    let req = RestartTaskRequest {
        task_id,
        dry_run: false,
        force: true,
    };

    match flowctl_service::lifecycle::restart_task(Some(&conn), &flow_dir, req).await {
        Ok(resp) => {
            let epic_id = epic_id_from_task(&resp.cascade_from).unwrap_or_default();
            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: resp.cascade_from.clone(),
                epic_id,
                from_status: "done".to_string(),
                to_status: "todo".to_string(),
            });
            Ok(Json(serde_json::json!({
                "success": true,
                "reset": resp.reset_ids
            })))
        }
        Err(e) => Err(service_error_to_app_error(e)),
    }
}

/// GET /api/v1/tasks/:id -- fetch full task details + evidence + runtime state.
pub async fn get_task_handler(
    State(state): State<AppState>,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let task_repo = flowctl_db::TaskRepo::new(conn.clone());
    let (task, body) = task_repo
        .get_with_body(&task_id)
        .await
        .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;

    let evidence_repo = flowctl_db::EvidenceRepo::new(conn.clone());
    let evidence = evidence_repo
        .get(&task_id)
        .await
        .map_err(|e| AppError::Internal(format!("evidence fetch error: {e}")))?;

    let runtime_repo = flowctl_db::RuntimeRepo::new(conn);
    let runtime = runtime_repo
        .get(&task_id)
        .await
        .map_err(|e| AppError::Internal(format!("runtime fetch error: {e}")))?;

    let mut value = serde_json::to_value(&task)
        .map_err(|e| AppError::Internal(format!("serialization error: {e}")))?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("body".to_string(), serde_json::Value::String(body));
        obj.insert(
            "evidence".to_string(),
            serde_json::to_value(&evidence)
                .map_err(|e| AppError::Internal(format!("evidence serialization error: {e}")))?,
        );
        obj.insert(
            "runtime".to_string(),
            serde_json::to_value(&runtime)
                .map_err(|e| AppError::Internal(format!("runtime serialization error: {e}")))?,
        );
        obj.insert(
            "duration_seconds".to_string(),
            match runtime.as_ref().and_then(|r| r.duration_secs) {
                Some(d) => serde_json::Value::Number(d.into()),
                None => serde_json::Value::Null,
            },
        );
    }
    Ok(Json(value))
}

// ── Request types ─────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct CreateTaskRequest {
    pub id: String,
    pub epic_id: String,
    pub title: String,
    pub depends_on: Option<Vec<String>>,
    pub body: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskIdRequest {
    pub task_id: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskDoneRequest {
    pub task_id: String,
    pub summary: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskReasonRequest {
    pub task_id: String,
    pub reason: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct StartTaskRestRequest {
    pub force: Option<bool>,
    pub actor: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct DoneTaskRestRequest {
    pub summary: Option<String>,
    pub evidence: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct BlockTaskRestRequest {
    pub reason: String,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct RestartTaskRestRequest {
    pub force: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
pub struct SkipTaskRestRequest {
    pub reason: String,
}
