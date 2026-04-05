//! WebSocket handler for live event streaming.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use flowctl_scheduler::TimestampedEvent;

use super::common::AppState;

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
