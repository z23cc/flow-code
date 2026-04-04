//! Event-driven notifications: sound alerts and webhook delivery.
//!
//! Subscribes to the daemon's event bus and reacts to key events:
//! - `EpicCompleted` → play a sound (macOS only, configurable)
//! - `TaskCompleted` / `TaskFailed` → POST to webhook URL (fire-and-forget)
//!
//! Feature-gated behind `#[cfg(feature = "daemon")]`.

use std::path::Path;

use tokio::sync::broadcast;
use tracing::{debug, info, warn};

#[cfg(feature = "webhook")]
use tracing::error;

use flowctl_scheduler::{FlowEvent, TimestampedEvent};

/// Notification configuration read from `.flow/config.json`.
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    /// Play a system sound on epic completion (default: true).
    pub sound: bool,
    /// Optional webhook URL for task events.
    pub webhook_url: Option<String>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            sound: true,
            webhook_url: None,
        }
    }
}

/// Load notification config from `.flow/config.json`.
///
/// Reads `notifications.sound` (bool, default true) and
/// `notifications.webhook_url` (string, optional).
pub fn load_config(flow_dir: &Path) -> NotificationConfig {
    let config_path = flow_dir.join("config.json");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return NotificationConfig::default(),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return NotificationConfig::default(),
    };

    let notifications = parsed.get("notifications");

    let sound = notifications
        .and_then(|n| n.get("sound"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let webhook_url = notifications
        .and_then(|n| n.get("webhook_url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    NotificationConfig { sound, webhook_url }
}

/// Spawn the notification listener task.
///
/// Subscribes to the event bus broadcast channel and handles sound/webhook
/// notifications until the receiver is closed or lagged beyond recovery.
pub fn spawn_listener(
    tracker: &tokio_util::task::TaskTracker,
    rx: broadcast::Receiver<TimestampedEvent>,
    config: NotificationConfig,
) {
    tracker.spawn(notification_loop(rx, config));
}

async fn notification_loop(
    mut rx: broadcast::Receiver<TimestampedEvent>,
    config: NotificationConfig,
) {
    info!("notification listener started");

    loop {
        match rx.recv().await {
            Ok(stamped) => handle_event(&stamped.event, &config).await,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("notification listener lagged, skipped {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => {
                debug!("notification listener shutting down (channel closed)");
                break;
            }
        }
    }
}

async fn handle_event(event: &FlowEvent, config: &NotificationConfig) {
    match event {
        FlowEvent::EpicCompleted { epic_id } => {
            info!("epic completed: {epic_id}");
            if config.sound {
                play_completion_sound();
            }
            if let Some(url) = &config.webhook_url {
                send_webhook(url, "epic_completed", epic_id, epic_id, "done").await;
            }
        }
        FlowEvent::TaskCompleted { task_id, epic_id } => {
            if let Some(url) = &config.webhook_url {
                send_webhook(url, "task_completed", task_id, epic_id, "done").await;
            }
        }
        FlowEvent::TaskFailed {
            task_id, epic_id, ..
        } => {
            if let Some(url) = &config.webhook_url {
                send_webhook(url, "task_failed", task_id, epic_id, "failed").await;
            }
        }
        _ => {}
    }
}

/// Play a system sound on macOS. Silently no-ops on other platforms.
fn play_completion_sound() {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let sound_path = "/System/Library/Sounds/Glass.aiff";
        match Command::new("afplay").arg(sound_path).spawn() {
            Ok(_child) => {
                debug!("playing completion sound: {sound_path}");
            }
            Err(e) => {
                warn!("failed to play sound: {e}");
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        debug!("sound notifications not supported on this platform");
    }
}

/// Webhook payload sent on task/epic events.
#[cfg(feature = "webhook")]
#[derive(Debug, serde::Serialize)]
struct WebhookPayload {
    event_type: String,
    task_id: String,
    epic_id: String,
    status: String,
    timestamp: String,
}

/// POST a webhook payload. Fire-and-forget: logs errors but never fails.
#[cfg(feature = "webhook")]
async fn send_webhook(url: &str, event_type: &str, task_id: &str, epic_id: &str, status: &str) {
    let payload = WebhookPayload {
        event_type: event_type.to_string(),
        task_id: task_id.to_string(),
        epic_id: epic_id.to_string(),
        status: status.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    debug!("sending webhook to {url}: {event_type} for {task_id}");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            error!("failed to create HTTP client for webhook: {e}");
            return;
        }
    };

    // Fire-and-forget: spawn so we don't block the event loop
    let url_owned = url.to_string();
    tokio::spawn(async move {
        match client.post(&url_owned).json(&payload).send().await {
            Ok(resp) => {
                debug!("webhook response: {} {}", resp.status(), url_owned);
            }
            Err(e) => {
                error!("webhook POST to {url_owned} failed: {e}");
            }
        }
    });
}

/// Stub when webhook feature is not enabled.
#[cfg(not(feature = "webhook"))]
async fn send_webhook(url: &str, event_type: &str, task_id: &str, _epic_id: &str, _status: &str) {
    warn!(
        "webhook feature not enabled, cannot send {event_type} for {task_id} to {url}. \
         Rebuild with --features webhook to enable."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn default_config() {
        let config = NotificationConfig::default();
        assert!(config.sound);
        assert!(config.webhook_url.is_none());
    }

    #[test]
    fn load_config_missing_file() {
        let tmp = TempDir::new().unwrap();
        let config = load_config(tmp.path());
        assert!(config.sound);
        assert!(config.webhook_url.is_none());
    }

    #[test]
    fn load_config_with_sound_disabled() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"notifications": {"sound": false}}"#,
        )
        .unwrap();
        let config = load_config(tmp.path());
        assert!(!config.sound);
        assert!(config.webhook_url.is_none());
    }

    #[test]
    fn load_config_with_webhook() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"notifications": {"sound": true, "webhook_url": "https://example.com/hook"}}"#,
        )
        .unwrap();
        let config = load_config(tmp.path());
        assert!(config.sound);
        assert_eq!(
            config.webhook_url.as_deref(),
            Some("https://example.com/hook")
        );
    }

    #[test]
    fn load_config_no_notifications_key() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.json");
        std::fs::write(&config_path, r#"{"memory": {"enabled": true}}"#).unwrap();
        let config = load_config(tmp.path());
        assert!(config.sound);
        assert!(config.webhook_url.is_none());
    }

    #[tokio::test]
    async fn notification_loop_handles_epic_completed() {
        let (tx, rx) = broadcast::channel(16);
        let config = NotificationConfig {
            sound: false, // don't actually play sound in test
            webhook_url: None,
        };

        // Spawn listener
        let handle = tokio::spawn(notification_loop(rx, config));

        // Send an event
        tx.send(TimestampedEvent {
            timestamp: Utc::now(),
            event: FlowEvent::EpicCompleted {
                epic_id: "fn-1-test".into(),
            },
        })
        .unwrap();

        // Drop sender to close the channel
        drop(tx);

        // Listener should exit cleanly
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("timeout")
            .expect("task panicked");
    }

    #[tokio::test]
    async fn notification_loop_handles_task_events() {
        let (tx, rx) = broadcast::channel(16);
        let config = NotificationConfig {
            sound: false,
            webhook_url: None,
        };

        let handle = tokio::spawn(notification_loop(rx, config));

        tx.send(TimestampedEvent {
            timestamp: Utc::now(),
            event: FlowEvent::TaskCompleted {
                task_id: "fn-1.1".into(),
                epic_id: "fn-1".into(),
            },
        })
        .unwrap();

        tx.send(TimestampedEvent {
            timestamp: Utc::now(),
            event: FlowEvent::TaskFailed {
                task_id: "fn-1.2".into(),
                epic_id: "fn-1".into(),
                error: Some("boom".into()),
            },
        })
        .unwrap();

        drop(tx);

        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("timeout")
            .expect("task panicked");
    }
}
