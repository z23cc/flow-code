//! HTTP API route handlers for the daemon.
//!
//! Provides REST endpoints for status, epics, tasks, and a WebSocket
//! endpoint for streaming live events to connected TUI clients.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use flowctl_scheduler::TimestampedEvent;

use crate::lifecycle::DaemonRuntime;

/// Shared application state for all handlers.
pub type AppState = Arc<DaemonState>;

/// Combined daemon state: runtime + event bus.
pub struct DaemonState {
    pub runtime: DaemonRuntime,
    pub event_bus: flowctl_scheduler::EventBus,
}

/// GET /api/v1/health -- simple liveness check.
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
}

/// GET /api/v1/metrics -- daemon health metrics.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = state.runtime.health();
    (StatusCode::OK, Json(metrics))
}

/// POST /api/v1/shutdown -- initiate graceful shutdown.
pub async fn shutdown_handler(State(state): State<AppState>) -> impl IntoResponse {
    state.runtime.initiate_shutdown();
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "shutting_down"})),
    )
}

/// GET /api/v1/status -- combined daemon status overview.
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let health = state.runtime.health();
    let subscribers = state.event_bus.subscriber_count();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "running",
            "uptime_secs": health.uptime_secs,
            "pid": health.pid,
            "memory_bytes": health.memory_bytes,
            "wal_size_bytes": health.wal_size_bytes,
            "event_subscribers": subscribers,
        })),
    )
}

/// GET /api/v1/epics -- list epics from the database.
///
/// Returns a JSON array of epics. On database errors, returns 500.
pub async fn epics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let db_path = state
        .runtime
        .paths
        .state_dir
        .parent()
        .map(|flow_dir| flow_dir.join("flowctl.db"));

    let Some(db_path) = db_path else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "cannot resolve db path"})),
        );
    };

    match flowctl_db::open(&db_path) {
        Ok(conn) => {
            let repo = flowctl_db::EpicRepo::new(&conn);
            match repo.list(None) {
                Ok(epics) => (StatusCode::OK, Json(serde_json::to_value(&epics).unwrap())),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("db open failed: {e}")})),
        ),
    }
}

/// GET /api/v1/tasks -- list tasks, optionally filtered by epic_id query param.
pub async fn tasks_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<TasksQuery>,
) -> impl IntoResponse {
    let db_path = state
        .runtime
        .paths
        .state_dir
        .parent()
        .map(|flow_dir| flow_dir.join("flowctl.db"));

    let Some(db_path) = db_path else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "cannot resolve db path"})),
        );
    };

    match flowctl_db::open(&db_path) {
        Ok(conn) => {
            let repo = flowctl_db::TaskRepo::new(&conn);
            let result = if let Some(ref epic_id) = params.epic_id {
                repo.list_by_epic(epic_id)
            } else {
                repo.list_all(None, None)
            };
            match result {
                Ok(tasks) => (StatusCode::OK, Json(serde_json::to_value(&tasks).unwrap())),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                ),
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("db open failed: {e}")})),
        ),
    }
}

/// Query parameters for the tasks endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct TasksQuery {
    pub epic_id: Option<String>,
}

/// GET /api/v1/events -- WebSocket upgrade for live event streaming.
pub async fn events_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rx = state.event_bus.subscribe();
    let cancel = state.runtime.cancel.clone();
    ws.on_upgrade(move |socket| handle_event_socket(socket, rx, cancel))
}

/// Handle a single WebSocket connection: stream events until the client
/// disconnects or the daemon shuts down.
async fn handle_event_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<TimestampedEvent>,
    cancel: tokio_util::sync::CancellationToken,
) {
    info!("WebSocket client connected for event streaming");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!("daemon shutting down, closing WebSocket");
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        match serde_json::to_string(&event) {
                            Ok(json) => {
                                if socket.send(Message::Text(json.into())).await.is_err() {
                                    debug!("WebSocket client disconnected");
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("failed to serialize event: {e}");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "WebSocket client lagged, skipping events");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!("event bus closed, closing WebSocket");
                        let _ = socket.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
            // Also handle incoming messages (ping/pong, close).
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        debug!("WebSocket client disconnected");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = socket.send(Message::Pong(data)).await;
                    }
                    Some(Ok(_)) => {
                        // Ignore other messages from client.
                    }
                    Some(Err(e)) => {
                        debug!("WebSocket error: {e}");
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}
