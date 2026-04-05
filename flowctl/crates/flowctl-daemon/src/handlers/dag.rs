//! DAG visualization, dependency management, and mutation handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use flowctl_core::state_machine::{Status, Transition};

use flowctl_scheduler::FlowEvent;

use super::common::{check_version, touch_updated_at, AppError, AppState};

/// A node in the DAG visualization.
#[derive(Debug, serde::Serialize)]
pub struct DagNode {
    pub id: String,
    pub title: String,
    pub status: String,
    pub domain: String,
    pub x: f64,
    pub y: f64,
}

/// An edge in the DAG visualization.
#[derive(Debug, serde::Serialize)]
pub struct DagEdge {
    pub from: String,
    pub to: String,
}

/// Response for the DAG endpoint.
#[derive(Debug, serde::Serialize)]
pub struct DagResponse {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
}

/// GET /api/v1/dag?epic_id=X -- returns DAG layout for visualization.
pub async fn dag_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<DagQuery>,
) -> Result<Json<DagResponse>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::TaskRepo::new(conn);
    let tasks = repo.list_by_epic(&params.epic_id).await?;

    if tasks.is_empty() {
        return Ok(Json(DagResponse {
            nodes: vec![],
            edges: vec![],
        }));
    }

    let dag = flowctl_core::TaskDag::from_tasks(&tasks)
        .map_err(|e| AppError::Internal(format!("DAG build error: {e}")))?;

    let task_map: std::collections::HashMap<&str, &flowctl_core::types::Task> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    let task_ids = dag.task_ids();

    // Compute layers via longest-path from roots.
    let mut layer: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for id in &task_ids {
        let deps = dag.dependencies(id);
        if deps.is_empty() {
            layer.insert(id.clone(), 0);
        } else {
            let max_dep_layer = deps
                .iter()
                .map(|d| layer.get(d.as_str()).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            layer.insert(id.clone(), max_dep_layer + 1);
        }
    }

    let max_layer = layer.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![vec![]; max_layer + 1];
    for (id, &l) in &layer {
        layers[l].push(id.clone());
    }
    for l in &mut layers {
        l.sort();
    }

    let node_spacing_x = 200.0;
    let node_spacing_y = 100.0;

    let mut nodes = Vec::with_capacity(tasks.len());
    for (layer_idx, layer_nodes) in layers.iter().enumerate() {
        let layer_height = layer_nodes.len() as f64 * node_spacing_y;
        let y_offset = -layer_height / 2.0 + node_spacing_y / 2.0;
        for (pos, id) in layer_nodes.iter().enumerate() {
            let task = task_map.get(id.as_str());
            nodes.push(DagNode {
                id: id.clone(),
                title: task.map(|t| t.title.clone()).unwrap_or_default(),
                status: task
                    .map(|t| format!("{:?}", t.status).to_lowercase())
                    .unwrap_or_else(|| "todo".to_string()),
                domain: task
                    .map(|t| t.domain.to_string())
                    .unwrap_or_else(|| "general".to_string()),
                x: layer_idx as f64 * node_spacing_x,
                y: y_offset + pos as f64 * node_spacing_y,
            });
        }
    }

    let mut edges = Vec::new();
    for id in &task_ids {
        for dep in dag.dependencies(id) {
            edges.push(DagEdge {
                from: dep,
                to: id.clone(),
            });
        }
    }

    Ok(Json(DagResponse { nodes, edges }))
}

/// GET /api/v1/dag/:id -- returns DAG with critical path for a specific epic.
pub async fn dag_detail_handler(
    State(state): State<AppState>,
    axum::extract::Path(epic_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::TaskRepo::new(conn);
    let tasks = repo.list_by_epic(&epic_id).await?;

    if tasks.is_empty() {
        return Ok(Json(serde_json::json!({
            "nodes": [],
            "edges": [],
            "critical_path": []
        })));
    }

    let dag = flowctl_core::TaskDag::from_tasks(&tasks)
        .map_err(|e| AppError::Internal(format!("DAG build error: {e}")))?;

    let task_map: std::collections::HashMap<&str, &flowctl_core::types::Task> =
        tasks.iter().map(|t| (t.id.as_str(), t)).collect();

    let task_ids = dag.task_ids();
    let critical_path = dag.critical_path();

    let nodes: Vec<serde_json::Value> = task_ids.iter().map(|id| {
        let task = task_map.get(id.as_str());
        serde_json::json!({
            "id": id,
            "title": task.map(|t| t.title.clone()).unwrap_or_default(),
            "status": task.map(|t| format!("{:?}", t.status).to_lowercase()).unwrap_or_else(|| "todo".to_string()),
            "domain": task.map(|t| t.domain.to_string()).unwrap_or_else(|| "general".to_string()),
        })
    }).collect();

    let mut edges = Vec::new();
    for id in &task_ids {
        for dep in dag.dependencies(id) {
            edges.push(serde_json::json!({"from": dep, "to": id}));
        }
    }

    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "critical_path": critical_path
    })))
}

/// POST /api/v1/dag/mutate -- apply a DAG mutation with optimistic locking.
pub async fn dag_mutate_handler(
    State(state): State<AppState>,
    Json(body): Json<DagMutateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::TaskRepo::new(conn.clone());

    match body.action.as_str() {
        "add_dep" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;
            let depends_on = body.params.get("depends_on")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.depends_on".into()))?;

            let task = repo.get(task_id).await
                .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;
            let _dep = repo.get(depends_on).await
                .map_err(|_| AppError::NotFound(format!("dependency task not found: {depends_on}")))?;

            check_version(&task, &body.version)?;

            let epic_tasks = repo.list_by_epic(&task.epic).await?;
            let test_tasks: Vec<flowctl_core::types::Task> = epic_tasks.into_iter().map(|mut t| {
                if t.id == task_id && !t.depends_on.contains(&depends_on.to_string()) {
                    t.depends_on.push(depends_on.to_string());
                }
                t
            }).collect();

            if let Err(e) = flowctl_core::TaskDag::from_tasks(&test_tasks) {
                return Err(AppError::InvalidInput(format!("would create cycle: {e}")));
            }

            conn.execute(
                "INSERT OR IGNORE INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                libsql::params![task_id.to_string(), depends_on.to_string()],
            ).await.map_err(|e| AppError::Db(e.to_string()))?;
            touch_updated_at(&conn, task_id).await?;

            state.event_bus.emit(FlowEvent::DagMutated {
                mutation: "dep_added".to_string(),
                details: serde_json::json!({"from": depends_on, "to": task_id}),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "add_dep"})))
        }

        "remove_dep" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;
            let depends_on = body.params.get("depends_on")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.depends_on".into()))?;

            let task = repo.get(task_id).await
                .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            conn.execute(
                "DELETE FROM task_deps WHERE task_id = ?1 AND depends_on = ?2",
                libsql::params![task_id.to_string(), depends_on.to_string()],
            ).await.map_err(|e| AppError::Db(e.to_string()))?;
            touch_updated_at(&conn, task_id).await?;

            state.event_bus.emit(FlowEvent::DagMutated {
                mutation: "dep_removed".to_string(),
                details: serde_json::json!({"from": depends_on, "to": task_id}),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "remove_dep"})))
        }

        "retry_task" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;

            let task = repo.get(task_id).await
                .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            Transition::new(task.status, Status::Todo).map_err(|e| {
                AppError::InvalidTransition(format!("cannot retry task '{}': {}", task_id, e))
            })?;

            let from_status = format!("{:?}", task.status).to_lowercase();
            repo.update_status(task_id, Status::Todo).await?;

            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
                from_status,
                to_status: "todo".to_string(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "retry_task"})))
        }

        "skip_task" => {
            let task_id = body.params.get("task_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::InvalidInput("missing params.task_id".into()))?;

            let task = repo.get(task_id).await
                .map_err(|_| AppError::NotFound(format!("task not found: {task_id}")))?;
            check_version(&task, &body.version)?;

            Transition::new(task.status, Status::Skipped).map_err(|e| {
                AppError::InvalidTransition(format!("cannot skip task '{}': {}", task_id, e))
            })?;

            let from_status = format!("{:?}", task.status).to_lowercase();
            repo.update_status(task_id, Status::Skipped).await?;

            state.event_bus.emit(FlowEvent::TaskStatusChanged {
                task_id: task_id.to_string(),
                epic_id: task.epic.clone(),
                from_status,
                to_status: "skipped".to_string(),
            });

            Ok(Json(serde_json::json!({"success": true, "action": "skip_task"})))
        }

        other => Err(AppError::InvalidInput(format!("unknown action: {other}"))),
    }
}

/// POST /api/v1/deps -- add a dependency edge.
pub async fn add_dep_handler(
    State(state): State<AppState>,
    Json(body): Json<AddDepRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let conn = state.db.clone();
    let repo = flowctl_db_lsql::TaskRepo::new(conn.clone());

    let from_task = repo.get(&body.from).await
        .map_err(|_| AppError::NotFound(format!("task not found: {}", body.from)))?;
    let _to_task = repo.get(&body.to).await
        .map_err(|_| AppError::NotFound(format!("task not found: {}", body.to)))?;

    // Cycle check: build hypothetical task list with new dep.
    let epic_tasks = repo.list_by_epic(&from_task.epic).await?;
    let test_tasks: Vec<flowctl_core::types::Task> = epic_tasks.into_iter().map(|mut t| {
        if t.id == body.to && !t.depends_on.contains(&body.from) {
            t.depends_on.push(body.from.clone());
        }
        t
    }).collect();

    if let Err(e) = flowctl_core::TaskDag::from_tasks(&test_tasks) {
        return Err(AppError::InvalidTransition(format!("would create cycle: {e}")));
    }

    conn.execute(
        "INSERT OR IGNORE INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
        libsql::params![body.to.clone(), body.from.clone()],
    ).await.map_err(|e| AppError::Db(e.to_string()))?;
    touch_updated_at(&conn, &body.to).await?;

    state.event_bus.emit(FlowEvent::DagMutated {
        mutation: "dep_added".to_string(),
        details: serde_json::json!({"from": body.from, "to": body.to}),
    });

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"from": body.from, "to": body.to})),
    ))
}

/// DELETE /api/v1/deps/:from/:to -- remove a dependency edge.
pub async fn remove_dep_handler(
    State(state): State<AppState>,
    axum::extract::Path((from, to)): axum::extract::Path<(String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    let conn = state.db.clone();

    let changed = conn.execute(
        "DELETE FROM task_deps WHERE task_id = ?1 AND depends_on = ?2",
        libsql::params![to.clone(), from.clone()],
    ).await.map_err(|e| AppError::Db(e.to_string()))?;

    if changed == 0 {
        return Err(AppError::NotFound(format!(
            "dependency not found: {from} → {to}"
        )));
    }

    touch_updated_at(&conn, &to).await?;

    state.event_bus.emit(FlowEvent::DagMutated {
        mutation: "dep_removed".to_string(),
        details: serde_json::json!({"from": from, "to": to}),
    });

    Ok(Json(serde_json::json!({"from": from, "to": to})))
}

// ── Request types ─────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct DagQuery {
    pub epic_id: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct DagMutateRequest {
    pub action: String,
    pub params: serde_json::Value,
    pub version: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct AddDepRequest {
    pub from: String,
    pub to: String,
}
