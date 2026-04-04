//! WebSocket client for connecting to the daemon's event stream.
//!
//! Connects to the daemon's Unix socket at `/api/v1/events` and
//! deserializes incoming events as `TimestampedEvent` for the TUI.

use std::path::Path;

use anyhow::{Context, Result};
use tokio::net::UnixStream;
use tokio_tungstenite::{client_async, tungstenite::Message};
use futures_util::StreamExt;
use tracing::{debug, info, warn};

use flowctl_scheduler::TimestampedEvent;

use crate::action::{Action, ActionSender};

/// Connect to the daemon WebSocket and forward events to the TUI action channel.
///
/// Returns a task handle that runs until the connection drops or the sender closes.
pub fn spawn_ws_listener(
    socket_path: &Path,
    action_tx: ActionSender,
) -> tokio::task::JoinHandle<()> {
    let socket_path = socket_path.to_path_buf();

    tokio::spawn(async move {
        match connect_and_stream(&socket_path, &action_tx).await {
            Ok(()) => info!("WebSocket connection closed normally"),
            Err(e) => warn!("WebSocket connection error: {e}"),
        }
    })
}

async fn connect_and_stream(socket_path: &Path, action_tx: &ActionSender) -> Result<()> {
    let stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("failed to connect to daemon socket: {}", socket_path.display()))?;

    let uri = "ws://localhost/api/v1/events";
    let (ws_stream, _response) = client_async(uri, stream)
        .await
        .context("WebSocket handshake failed")?;

    info!("connected to daemon event stream via WebSocket");

    let (_, mut read) = ws_stream.split();

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<TimestampedEvent>(&text) {
                    Ok(event) => {
                        if action_tx.send(Action::FlowEvent(event)).is_err() {
                            debug!("action channel closed, stopping WebSocket listener");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("failed to parse event from daemon: {e}");
                    }
                }
            }
            Ok(Message::Close(_)) => {
                debug!("daemon closed WebSocket connection");
                break;
            }
            Ok(Message::Ping(_)) => {
                // tungstenite auto-responds to pings
            }
            Ok(_) => {}
            Err(e) => {
                warn!("WebSocket read error: {e}");
                break;
            }
        }
    }

    Ok(())
}
