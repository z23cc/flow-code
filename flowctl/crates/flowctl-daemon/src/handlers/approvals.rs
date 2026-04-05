//! Approval lifecycle handlers: create, list, get, approve, reject.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

use flowctl_core::approvals::{
    ApprovalStatus, CreateApprovalRequest, ResolveRequest,
};
use flowctl_scheduler::FlowEvent;
use flowctl_service::approvals::{ApprovalStore, LibSqlApprovalStore};

use super::common::{service_error_to_app_error, AppError, AppState};

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
}

/// GET /api/v1/approvals -- list approvals, optionally filtered by status.
pub async fn list_approvals_handler(
    State(state): State<AppState>,
    Query(params): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let status_filter = match params.status.as_deref() {
        Some(s) => Some(
            ApprovalStatus::parse(s)
                .ok_or_else(|| AppError::InvalidInput(format!("invalid status: {s}")))?,
        ),
        None => None,
    };
    let store = LibSqlApprovalStore::new(state.db.clone());
    let approvals = store
        .list(status_filter)
        .await
        .map_err(service_error_to_app_error)?;
    let value = serde_json::to_value(&approvals)
        .map_err(|e| AppError::Internal(format!("serialize: {e}")))?;
    Ok(Json(value))
}

/// GET /api/v1/approvals/{id} -- fetch a single approval.
pub async fn get_approval_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let store = LibSqlApprovalStore::new(state.db.clone());
    let approval = store.get(&id).await.map_err(service_error_to_app_error)?;
    let value = serde_json::to_value(&approval)
        .map_err(|e| AppError::Internal(format!("serialize: {e}")))?;
    Ok(Json(value))
}

/// POST /api/v1/approvals -- create a new pending approval.
pub async fn create_approval_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateApprovalRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let store = LibSqlApprovalStore::new(state.db.clone());
    let created = store
        .create(body)
        .await
        .map_err(service_error_to_app_error)?;

    state.event_bus.emit(FlowEvent::ApprovalCreated {
        id: created.id.clone(),
        task_id: created.task_id.clone(),
    });

    let value = serde_json::to_value(&created)
        .map_err(|e| AppError::Internal(format!("serialize: {e}")))?;
    Ok((StatusCode::CREATED, Json(value)))
}

/// POST /api/v1/approvals/{id}/approve -- mark an approval as approved.
pub async fn approve_approval_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Option<ResolveRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let body = body.unwrap_or_default();
    // Browser clients typically don't send a resolver; fall back to
    // "dashboard" so the audit trail still records *something* rather
    // than a null. CLI callers should pass --resolver explicitly.
    let resolver = body.resolver.or_else(|| Some("dashboard".to_string()));
    let store = LibSqlApprovalStore::new(state.db.clone());
    let resolved = store
        .approve(&id, resolver)
        .await
        .map_err(service_error_to_app_error)?;

    state.event_bus.emit(FlowEvent::ApprovalResolved {
        id: resolved.id.clone(),
        status: resolved.status.as_str().to_string(),
    });

    let value = serde_json::to_value(&resolved)
        .map_err(|e| AppError::Internal(format!("serialize: {e}")))?;
    Ok(Json(value))
}

/// POST /api/v1/approvals/{id}/reject -- mark an approval as rejected.
pub async fn reject_approval_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Option<ResolveRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let body = body.unwrap_or_default();
    // Browser fallback (see approve_approval_handler for rationale).
    let resolver = body.resolver.or_else(|| Some("dashboard".to_string()));
    let store = LibSqlApprovalStore::new(state.db.clone());
    let resolved = store
        .reject(&id, resolver, body.reason)
        .await
        .map_err(service_error_to_app_error)?;

    state.event_bus.emit(FlowEvent::ApprovalResolved {
        id: resolved.id.clone(),
        status: resolved.status.as_str().to_string(),
    });

    let value = serde_json::to_value(&resolved)
        .map_err(|e| AppError::Internal(format!("serialize: {e}")))?;
    Ok(Json(value))
}
