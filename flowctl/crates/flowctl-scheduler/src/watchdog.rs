//! Heartbeat-based zombie task detection.
//!
//! Workers emit heartbeats every 10s (write to heartbeats table).
//! The watchdog checks every 15s. If no heartbeat within 60s:
//! - If retries remain → `up_for_retry`
//! - Else → `failed` + propagate downstream

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Default heartbeat interval for workers.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// How often the watchdog checks for zombies.
pub const WATCHDOG_CHECK_INTERVAL: Duration = Duration::from_secs(15);

/// Time without a heartbeat before a task is considered zombie.
pub const ZOMBIE_TIMEOUT: Duration = Duration::from_secs(60);

/// Action the watchdog recommends for a zombie task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZombieAction {
    /// Task should be retried (retries remaining).
    Retry(String),
    /// Task has exhausted retries — mark failed.
    Fail(String),
}

/// Heartbeat record for a single task.
#[derive(Debug, Clone)]
struct HeartbeatEntry {
    /// When the last heartbeat was received.
    last_seen: Instant,
    /// How many times this task has been retried.
    retry_count: u32,
}

/// Thread-safe heartbeat table. Workers call `heartbeat()`, the watchdog
/// calls `check_zombies()`.
#[derive(Debug, Clone)]
pub struct HeartbeatTable {
    inner: Arc<Mutex<HashMap<String, HeartbeatEntry>>>,
    max_retries: u32,
}

impl HeartbeatTable {
    /// Create a new heartbeat table.
    pub fn new(max_retries: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            max_retries,
        }
    }

    /// Record a heartbeat for a task. Called by workers every HEARTBEAT_INTERVAL.
    pub async fn heartbeat(&self, task_id: &str) {
        let mut table = self.inner.lock().await;
        let entry = table.entry(task_id.to_string()).or_insert(HeartbeatEntry {
            last_seen: Instant::now(),
            retry_count: 0,
        });
        entry.last_seen = Instant::now();
        debug!(task_id, "heartbeat received");
    }

    /// Register a task as active (called when the scheduler dispatches it).
    pub async fn register(&self, task_id: &str) {
        let mut table = self.inner.lock().await;
        table.insert(
            task_id.to_string(),
            HeartbeatEntry {
                last_seen: Instant::now(),
                retry_count: 0,
            },
        );
    }

    /// Remove a task from the heartbeat table (called on completion).
    pub async fn deregister(&self, task_id: &str) {
        let mut table = self.inner.lock().await;
        table.remove(task_id);
    }

    /// Check for zombie tasks — those with no heartbeat within ZOMBIE_TIMEOUT.
    /// Returns recommended actions for each zombie.
    pub async fn check_zombies(&self) -> Vec<ZombieAction> {
        let now = Instant::now();
        let mut table = self.inner.lock().await;
        let mut actions = Vec::new();

        let zombie_ids: Vec<String> = table
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_seen) > ZOMBIE_TIMEOUT)
            .map(|(id, _)| id.clone())
            .collect();

        for id in zombie_ids {
            if let Some(entry) = table.get_mut(&id) {
                if entry.retry_count < self.max_retries {
                    entry.retry_count += 1;
                    entry.last_seen = Instant::now(); // Reset timer for retry.
                    warn!(task_id = %id, retry = entry.retry_count, "zombie detected, scheduling retry");
                    actions.push(ZombieAction::Retry(id));
                } else {
                    warn!(task_id = %id, "zombie detected, max retries exhausted — failing");
                    table.remove(&id);
                    actions.push(ZombieAction::Fail(id));
                }
            }
        }

        actions
    }
}

/// Run the watchdog loop. Checks the heartbeat table every WATCHDOG_CHECK_INTERVAL
/// and sends zombie actions through the returned channel.
pub async fn run_watchdog(
    heartbeats: HeartbeatTable,
    cancel: CancellationToken,
    action_tx: tokio::sync::mpsc::UnboundedSender<ZombieAction>,
) {
    info!("watchdog started (check interval: {}s, timeout: {}s)",
        WATCHDOG_CHECK_INTERVAL.as_secs(),
        ZOMBIE_TIMEOUT.as_secs(),
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("watchdog shutting down");
                return;
            }
            _ = tokio::time::sleep(WATCHDOG_CHECK_INTERVAL) => {
                let actions = heartbeats.check_zombies().await;
                for action in actions {
                    if action_tx.send(action).is_err() {
                        // Receiver dropped — scheduler is gone.
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_registers_task() {
        let table = HeartbeatTable::new(2);
        table.register("task-1").await;
        // No zombies immediately after registration.
        let actions = table.check_zombies().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_heartbeat_resets_timer() {
        let table = HeartbeatTable::new(2);
        table.register("task-1").await;
        table.heartbeat("task-1").await;
        let actions = table.check_zombies().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_deregister_removes_task() {
        let table = HeartbeatTable::new(2);
        table.register("task-1").await;
        table.deregister("task-1").await;
        let actions = table.check_zombies().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_zombie_detection_retry() {
        let table = HeartbeatTable::new(2);

        // Manually insert with old timestamp.
        {
            let mut inner = table.inner.lock().await;
            inner.insert(
                "task-1".to_string(),
                HeartbeatEntry {
                    last_seen: Instant::now() - Duration::from_secs(120),
                    retry_count: 0,
                },
            );
        }

        let actions = table.check_zombies().await;
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], ZombieAction::Retry("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_zombie_detection_fail_after_max_retries() {
        let table = HeartbeatTable::new(1);

        // Insert with old timestamp and 1 retry already used.
        {
            let mut inner = table.inner.lock().await;
            inner.insert(
                "task-1".to_string(),
                HeartbeatEntry {
                    last_seen: Instant::now() - Duration::from_secs(120),
                    retry_count: 1,
                },
            );
        }

        let actions = table.check_zombies().await;
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], ZombieAction::Fail("task-1".to_string()));
    }

    #[tokio::test]
    async fn test_watchdog_cancellation() {
        let table = HeartbeatTable::new(2);
        let cancel = CancellationToken::new();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        cancel.cancel(); // Cancel immediately.

        run_watchdog(table, cancel, tx).await;

        // No actions should have been sent.
        assert!(rx.try_recv().is_err());
    }
}
