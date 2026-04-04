//! Broadcast event bus for flowctl scheduler events.
//!
//! Dual-channel architecture:
//! - `tokio::sync::broadcast` for non-critical consumers (TUI, WebSocket)
//! - `tokio::sync::mpsc` for critical consumers (SQLite logger)
//!
//! Consumers that fall behind on broadcast will receive `Lagged` and
//! skip missed events (acceptable for live dashboards).

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

/// Default broadcast channel capacity.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// Events emitted by the scheduler and subsystems.
///
/// Each variant carries enough context for consumers to act without
/// needing to query the database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum FlowEvent {
    /// A task's dependencies are satisfied; it is ready for dispatch.
    TaskReady { task_id: String, epic_id: String },

    /// A task has been dispatched to a worker.
    TaskStarted { task_id: String, epic_id: String },

    /// A task completed successfully.
    TaskCompleted { task_id: String, epic_id: String },

    /// A task failed (with optional error message).
    TaskFailed {
        task_id: String,
        epic_id: String,
        error: Option<String>,
    },

    /// Watchdog detected a zombie task (no heartbeat within timeout).
    TaskZombie { task_id: String, epic_id: String },

    /// A new wave of tasks has started.
    WaveStarted { wave: u32, task_count: usize },

    /// All tasks in the current wave completed.
    WaveCompleted { wave: u32 },

    /// All tasks in the epic are done.
    EpicCompleted { epic_id: String },

    /// Guard check passed.
    GuardPassed { task_id: String },

    /// Guard check failed.
    GuardFailed {
        task_id: String,
        error: Option<String>,
    },

    /// File lock conflict detected.
    LockConflict {
        task_id: String,
        file: String,
        held_by: String,
    },

    /// Circuit breaker opened (too many consecutive failures).
    CircuitOpen { consecutive_failures: u32 },

    /// Daemon started successfully.
    DaemonStarted { pid: u32 },

    /// Daemon is shutting down.
    DaemonShutdown,
}

impl fmt::Display for FlowEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlowEvent::TaskReady { task_id, .. } => write!(f, "task_ready:{task_id}"),
            FlowEvent::TaskStarted { task_id, .. } => write!(f, "task_started:{task_id}"),
            FlowEvent::TaskCompleted { task_id, .. } => write!(f, "task_completed:{task_id}"),
            FlowEvent::TaskFailed { task_id, error, .. } => {
                write!(f, "task_failed:{task_id}")?;
                if let Some(e) = error {
                    write!(f, " ({e})")?;
                }
                Ok(())
            }
            FlowEvent::TaskZombie { task_id, .. } => write!(f, "task_zombie:{task_id}"),
            FlowEvent::WaveStarted { wave, task_count } => {
                write!(f, "wave_started:{wave} ({task_count} tasks)")
            }
            FlowEvent::WaveCompleted { wave } => write!(f, "wave_completed:{wave}"),
            FlowEvent::EpicCompleted { epic_id } => write!(f, "epic_completed:{epic_id}"),
            FlowEvent::GuardPassed { task_id } => write!(f, "guard_passed:{task_id}"),
            FlowEvent::GuardFailed { task_id, .. } => write!(f, "guard_failed:{task_id}"),
            FlowEvent::LockConflict { task_id, file, .. } => {
                write!(f, "lock_conflict:{task_id}:{file}")
            }
            FlowEvent::CircuitOpen {
                consecutive_failures,
            } => write!(f, "circuit_open:{consecutive_failures}"),
            FlowEvent::DaemonStarted { pid } => write!(f, "daemon_started:{pid}"),
            FlowEvent::DaemonShutdown => write!(f, "daemon_shutdown"),
        }
    }
}

/// Timestamped wrapper around a FlowEvent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedEvent {
    /// When the event was emitted.
    pub timestamp: DateTime<Utc>,
    /// The event payload.
    pub event: FlowEvent,
}

/// The event bus: broadcast for non-critical consumers, mpsc for critical ones.
#[derive(Clone)]
pub struct EventBus {
    /// Broadcast sender for non-critical consumers (TUI, WebSocket).
    broadcast_tx: broadcast::Sender<TimestampedEvent>,
    /// MPSC sender for critical consumers (SQLite logger).
    critical_tx: mpsc::Sender<TimestampedEvent>,
}

impl EventBus {
    /// Create a new event bus with the given channel capacity.
    ///
    /// Returns the bus and the critical receiver (caller must spawn a
    /// consumer task that drains it to SQLite).
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<TimestampedEvent>) {
        let (broadcast_tx, _) = broadcast::channel(capacity);
        let (critical_tx, critical_rx) = mpsc::channel(capacity);

        let bus = Self {
            broadcast_tx,
            critical_tx,
        };

        (bus, critical_rx)
    }

    /// Create a new event bus with the default capacity.
    pub fn with_default_capacity() -> (Self, mpsc::Receiver<TimestampedEvent>) {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }

    /// Emit an event to all consumers.
    ///
    /// The event is sent to both the broadcast channel (best-effort) and
    /// the critical mpsc channel (guaranteed delivery unless full).
    pub fn emit(&self, event: FlowEvent) {
        let stamped = TimestampedEvent {
            timestamp: Utc::now(),
            event,
        };

        // Broadcast: best-effort (no receivers = no-op).
        let broadcast_count = self.broadcast_tx.send(stamped.clone()).unwrap_or(0);
        debug!(broadcast_count, event = %stamped.event, "event emitted");

        // Critical: warn if the channel is full (should not happen in practice).
        if let Err(e) = self.critical_tx.try_send(stamped) {
            warn!("critical event channel full, event dropped: {e}");
        }
    }

    /// Subscribe to the broadcast channel. Returns a receiver that will
    /// get all events emitted after this call.
    pub fn subscribe(&self) -> broadcast::Receiver<TimestampedEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Get the current number of broadcast subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.broadcast_tx.receiver_count()
    }
}

impl fmt::Debug for EventBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventBus")
            .field("subscribers", &self.broadcast_tx.receiver_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_emit_to_broadcast_subscriber() {
        let (bus, _critical_rx) = EventBus::with_default_capacity();
        let mut rx = bus.subscribe();

        bus.emit(FlowEvent::DaemonStarted { pid: 42 });

        let received = rx.recv().await.unwrap();
        assert_eq!(received.event, FlowEvent::DaemonStarted { pid: 42 });
    }

    #[tokio::test]
    async fn test_emit_to_critical_channel() {
        let (bus, mut critical_rx) = EventBus::with_default_capacity();

        bus.emit(FlowEvent::DaemonShutdown);

        let received = critical_rx.recv().await.unwrap();
        assert_eq!(received.event, FlowEvent::DaemonShutdown);
    }

    #[tokio::test]
    async fn test_multiple_broadcast_subscribers() {
        let (bus, _critical_rx) = EventBus::with_default_capacity();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(FlowEvent::TaskStarted {
            task_id: "t1".into(),
            epic_id: "e1".into(),
        });

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.event, e2.event);
    }

    #[tokio::test]
    async fn test_no_subscribers_does_not_panic() {
        let (bus, _critical_rx) = EventBus::with_default_capacity();
        // No broadcast subscribers — should not panic.
        bus.emit(FlowEvent::DaemonShutdown);
    }

    #[tokio::test]
    async fn test_subscriber_count() {
        let (bus, _critical_rx) = EventBus::with_default_capacity();
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        // Note: broadcast receiver count may not update immediately.
    }

    #[test]
    fn test_flow_event_display() {
        let e = FlowEvent::TaskFailed {
            task_id: "t1".into(),
            epic_id: "e1".into(),
            error: Some("boom".into()),
        };
        assert_eq!(e.to_string(), "task_failed:t1 (boom)");
    }

    #[test]
    fn test_flow_event_serde_roundtrip() {
        let event = FlowEvent::WaveStarted {
            wave: 3,
            task_count: 5,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: FlowEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, event);
    }

    #[test]
    fn test_timestamped_event_serde() {
        let stamped = TimestampedEvent {
            timestamp: Utc::now(),
            event: FlowEvent::DaemonStarted { pid: 1234 },
        };
        let json = serde_json::to_string(&stamped).unwrap();
        let deserialized: TimestampedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event, FlowEvent::DaemonStarted { pid: 1234 });
    }

    #[test]
    fn test_event_bus_is_clone() {
        let (bus, _rx) = EventBus::with_default_capacity();
        let _bus2 = bus.clone();
    }

    #[test]
    fn test_custom_capacity() {
        let (bus, _rx) = EventBus::new(16);
        let _sub = bus.subscribe();
        bus.emit(FlowEvent::DaemonShutdown);
    }
}
