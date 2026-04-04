//! File watcher for `.flow/` directory changes.
//!
//! Uses the `notify` crate to watch for filesystem events. Debounces
//! events with a 500ms window — git checkout can fire many events at once.
//!
//! Feature-gated behind `#[cfg(feature = "daemon")]`.

#[cfg(feature = "daemon")]
mod inner {
    use std::path::PathBuf;
    use std::time::Duration;

    use notify::{
        Config, Event, RecommendedWatcher, RecursiveMode, Watcher,
    };
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use tracing::{debug, info, warn};

    /// Debounce window: batch events arriving within this duration.
    const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

    /// A debounced filesystem change notification.
    #[derive(Debug, Clone)]
    pub struct FlowChange {
        /// Paths that changed (deduplicated within the debounce window).
        pub paths: Vec<PathBuf>,
    }

    /// Start watching the `.flow/` directory for changes.
    ///
    /// Returns a receiver that emits debounced `FlowChange` notifications.
    /// The watcher runs until the `CancellationToken` is cancelled.
    pub async fn watch_flow_dir(
        flow_dir: PathBuf,
        cancel: CancellationToken,
    ) -> Result<mpsc::UnboundedReceiver<FlowChange>, notify::Error> {
        let (change_tx, change_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        // Create the filesystem watcher.
        let event_tx_clone = event_tx.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        let _ = event_tx_clone.send(event);
                    }
                    Err(e) => {
                        warn!("filesystem watch error: {}", e);
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(&flow_dir, RecursiveMode::Recursive)?;
        info!(path = %flow_dir.display(), "watching .flow/ for changes");

        // Spawn debounce task.
        tokio::spawn(async move {
            let _watcher = watcher; // Keep watcher alive.

            loop {
                // Wait for the first event or cancellation.
                let first_event = tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("file watcher shutting down");
                        return;
                    }
                    event = event_rx.recv() => {
                        match event {
                            Some(e) => e,
                            None => return,
                        }
                    }
                };

                // Collect paths from the first event.
                let mut changed_paths: Vec<PathBuf> = first_event.paths;

                // Debounce: collect more events within the window.
                let deadline = tokio::time::Instant::now() + DEBOUNCE_WINDOW;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            info!("file watcher shutting down during debounce");
                            return;
                        }
                        _ = tokio::time::sleep_until(deadline) => {
                            break; // Debounce window elapsed.
                        }
                        event = event_rx.recv() => {
                            match event {
                                Some(e) => {
                                    changed_paths.extend(e.paths);
                                }
                                None => return,
                            }
                        }
                    }
                }

                // Deduplicate paths.
                changed_paths.sort();
                changed_paths.dedup();

                debug!(
                    count = changed_paths.len(),
                    "debounced filesystem change batch"
                );

                let change = FlowChange {
                    paths: changed_paths,
                };

                if change_tx.send(change).is_err() {
                    // Receiver dropped.
                    return;
                }
            }
        });

        Ok(change_rx)
    }
}

#[cfg(feature = "daemon")]
pub use inner::*;

// Stub types when daemon feature is not enabled, so downstream code
// can reference the module without feature gates everywhere.
#[cfg(not(feature = "daemon"))]
mod stub {
    /// Placeholder for when the daemon feature is disabled.
    #[derive(Debug, Clone)]
    pub struct FlowChange {
        pub paths: Vec<std::path::PathBuf>,
    }
}

#[cfg(not(feature = "daemon"))]
pub use stub::*;

#[cfg(all(test, feature = "daemon"))]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn test_watch_detects_file_creation() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path().join(".flow");
        std::fs::create_dir_all(&flow_dir).unwrap();

        let cancel = CancellationToken::new();
        let mut rx = watch_flow_dir(flow_dir.clone(), cancel.clone())
            .await
            .unwrap();

        // Create a file in the watched directory.
        let test_file = flow_dir.join("test.json");
        std::fs::write(&test_file, "{}").unwrap();

        // Wait for the debounced notification.
        let change = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            rx.recv(),
        )
        .await
        .expect("timeout waiting for change")
        .expect("channel closed");

        assert!(!change.paths.is_empty());

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_watch_cancellation() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path().join(".flow");
        std::fs::create_dir_all(&flow_dir).unwrap();

        let cancel = CancellationToken::new();
        let _rx = watch_flow_dir(flow_dir, cancel.clone()).await.unwrap();

        cancel.cancel();

        // Give the watcher task a moment to shut down.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn test_debounce_batches_events() {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path().join(".flow");
        std::fs::create_dir_all(&flow_dir).unwrap();

        let cancel = CancellationToken::new();
        let mut rx = watch_flow_dir(flow_dir.clone(), cancel.clone())
            .await
            .unwrap();

        // Create multiple files rapidly (should be batched).
        for i in 0..5 {
            std::fs::write(flow_dir.join(format!("file-{i}.json")), "{}").unwrap();
        }

        // Should receive one debounced batch.
        let change = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            rx.recv(),
        )
        .await
        .expect("timeout")
        .expect("channel closed");

        // The batch should contain multiple paths (at least some of them).
        assert!(!change.paths.is_empty());

        cancel.cancel();
    }
}
