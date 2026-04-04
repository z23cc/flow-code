//! Daemon lifecycle: PID lock, stale detection, socket management, graceful shutdown.
//!
//! Follows the Docker-style CLI → Unix socket → Daemon pattern.
//! Feature-gated behind `#[cfg(feature = "daemon")]`.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use nix::sys::signal;
use nix::unistd::Pid;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{info, warn};

/// Default drain timeout before force-killing subsystems.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Paths for daemon state files within `.flow/.state/`.
#[derive(Debug, Clone)]
pub struct DaemonPaths {
    /// PID file: `.flow/.state/flowctl.pid`
    pub pid_file: PathBuf,
    /// Unix socket: `.flow/.state/flowctl.sock`
    pub socket_file: PathBuf,
    /// State directory: `.flow/.state/`
    pub state_dir: PathBuf,
}

impl DaemonPaths {
    /// Create paths rooted at the given `.flow/` directory.
    pub fn new(flow_dir: &Path) -> Self {
        let state_dir = flow_dir.join(".state");
        Self {
            pid_file: state_dir.join("flowctl.pid"),
            socket_file: state_dir.join("flowctl.sock"),
            state_dir,
        }
    }

    /// Ensure the state directory exists.
    pub fn ensure_state_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("failed to create state dir: {}", self.state_dir.display()))
    }
}

/// Health metrics tracked by the running daemon.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthMetrics {
    /// Daemon uptime in seconds.
    pub uptime_secs: u64,
    /// PID of the daemon process.
    pub pid: u32,
    /// Resident memory in bytes (0 if unavailable).
    pub memory_bytes: u64,
    /// WAL file size in bytes (0 if no WAL).
    pub wal_size_bytes: u64,
}

/// Runtime handle for the daemon, managing shutdown coordination.
pub struct DaemonRuntime {
    /// Token propagated to all subsystems for cooperative cancellation.
    pub cancel: CancellationToken,
    /// Tracks all spawned subsystem tasks for graceful drain.
    pub tracker: TaskTracker,
    /// Daemon state paths.
    pub paths: DaemonPaths,
    /// When the daemon started.
    pub started_at: Instant,
    /// Sender for shutdown signal (value = true means shutting down).
    shutdown_tx: watch::Sender<bool>,
    /// Receiver cloneable by subsystems.
    pub shutdown_rx: watch::Receiver<bool>,
}

impl DaemonRuntime {
    /// Create a new daemon runtime.
    pub fn new(paths: DaemonPaths) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            cancel: CancellationToken::new(),
            tracker: TaskTracker::new(),
            paths,
            started_at: Instant::now(),
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Get current health metrics.
    pub fn health(&self) -> HealthMetrics {
        let wal_size = self
            .paths
            .state_dir
            .parent()
            .map(|flow_dir| flow_dir.join("flowctl.db-wal"))
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        HealthMetrics {
            uptime_secs: self.started_at.elapsed().as_secs(),
            pid: std::process::id(),
            memory_bytes: get_resident_memory(),
            wal_size_bytes: wal_size,
        }
    }

    /// Initiate graceful shutdown: cancel token + notify watchers.
    pub fn initiate_shutdown(&self) {
        info!("initiating graceful shutdown");
        self.cancel.cancel();
        let _ = self.shutdown_tx.send(true);
    }

    /// Wait for all tracked tasks to complete, with drain timeout.
    pub async fn drain(&self) -> Result<()> {
        self.tracker.close();
        info!("waiting for subsystems to drain (timeout: {}s)", DRAIN_TIMEOUT.as_secs());

        let result = tokio::time::timeout(DRAIN_TIMEOUT, self.tracker.wait()).await;

        match result {
            Ok(()) => {
                info!("all subsystems drained cleanly");
                Ok(())
            }
            Err(_) => {
                warn!("drain timeout exceeded, forcing shutdown");
                // CancellationToken already cancelled — tasks should be aborting.
                // TaskTracker will drop remaining tasks when we return.
                Ok(())
            }
        }
    }

    /// Clean up PID and socket files on shutdown.
    pub fn cleanup(&self) {
        if let Err(e) = fs::remove_file(&self.paths.pid_file) {
            if e.kind() != io::ErrorKind::NotFound {
                warn!("failed to remove PID file: {}", e);
            }
        }
        if let Err(e) = fs::remove_file(&self.paths.socket_file) {
            if e.kind() != io::ErrorKind::NotFound {
                warn!("failed to remove socket file: {}", e);
            }
        }
    }
}

/// Acquire the PID lock file. Returns error if another daemon is running.
///
/// Handles stale PID detection: if the PID file exists but the process
/// is dead, cleans up and proceeds.
pub fn acquire_pid_lock(paths: &DaemonPaths) -> Result<()> {
    paths.ensure_state_dir()?;

    // Check for existing PID file
    if paths.pid_file.exists() {
        let contents = fs::read_to_string(&paths.pid_file)
            .with_context(|| format!("failed to read PID file: {}", paths.pid_file.display()))?;

        if let Ok(pid) = contents.trim().parse::<i32>() {
            if is_process_alive(pid) {
                bail!(
                    "daemon already running (PID {}). Use `flowctl daemon stop` to stop it.",
                    pid
                );
            }
            // Stale PID — process is dead, clean up
            warn!("stale PID file found (PID {} is dead), cleaning up", pid);
            let _ = fs::remove_file(&paths.pid_file);
        } else {
            warn!("corrupt PID file, removing");
            let _ = fs::remove_file(&paths.pid_file);
        }
    }

    // Write our PID
    let pid = std::process::id();
    fs::write(&paths.pid_file, pid.to_string())
        .with_context(|| format!("failed to write PID file: {}", paths.pid_file.display()))?;

    info!("acquired PID lock (PID {})", pid);
    Ok(())
}

/// Clean up orphaned socket file from a previous unclean shutdown.
pub fn cleanup_orphaned_socket(paths: &DaemonPaths) -> Result<()> {
    if paths.socket_file.exists() {
        warn!(
            "removing orphaned socket: {}",
            paths.socket_file.display()
        );
        fs::remove_file(&paths.socket_file).with_context(|| {
            format!(
                "failed to remove orphaned socket: {}",
                paths.socket_file.display()
            )
        })?;
    }
    Ok(())
}

/// Set socket file permissions to 0600 (owner read/write only).
pub fn set_socket_permissions(socket_path: &Path) -> Result<()> {
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(socket_path, perms)
        .with_context(|| format!("failed to set socket permissions: {}", socket_path.display()))
}

/// Check if a process with the given PID is alive.
fn is_process_alive(pid: i32) -> bool {
    // Signal 0 checks existence without sending a signal
    signal::kill(Pid::from_raw(pid), None).is_ok()
}

/// Read the PID from the PID file, if it exists and is valid.
pub fn read_pid(paths: &DaemonPaths) -> Option<i32> {
    fs::read_to_string(&paths.pid_file)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Check if the daemon is currently running (PID file exists and process alive).
pub fn is_daemon_running(paths: &DaemonPaths) -> bool {
    read_pid(paths).is_some_and(is_process_alive)
}

/// Send a stop signal to the running daemon via the socket.
///
/// Returns `Ok(())` if the stop request was sent successfully, or an error
/// if the daemon is not reachable.
pub async fn send_stop(paths: &DaemonPaths) -> Result<()> {
    if !paths.socket_file.exists() {
        bail!("daemon socket not found: {}", paths.socket_file.display());
    }

    // Connect to the Unix socket and send a raw HTTP POST /shutdown
    let mut stream = tokio::net::UnixStream::connect(&paths.socket_file)
        .await
        .context("failed to connect to daemon socket — is the daemon running?")?;

    let request = b"POST /api/v1/shutdown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n";
    stream
        .write_all(request)
        .await
        .context("failed to send shutdown request")?;

    info!("shutdown request sent");
    Ok(())
}

/// CLI smart routing: determine how to route a command.
///
/// Returns `Ok(CliRoute)` indicating whether to use the socket or error out.
pub fn detect_route(paths: &DaemonPaths) -> CliRoute {
    let pid = read_pid(paths);

    match pid {
        Some(pid) if is_process_alive(pid) => {
            if paths.socket_file.exists() {
                CliRoute::Socket
            } else {
                // PID alive but no socket — daemon is starting up or broken
                CliRoute::Error(format!(
                    "daemon PID {} is alive but socket not found. \
                     The daemon may be starting up or in a bad state.",
                    pid
                ))
            }
        }
        Some(pid) => {
            // PID exists but process dead — stale
            CliRoute::Error(format!(
                "stale PID file found (PID {} is dead). \
                 Run `flowctl daemon start` to start a new daemon.",
                pid
            ))
        }
        None => CliRoute::NoDaemon,
    }
}

/// Result of CLI route detection.
#[derive(Debug)]
pub enum CliRoute {
    /// Daemon is running and reachable via socket.
    Socket,
    /// No daemon is running (no PID file).
    NoDaemon,
    /// Error state: PID exists but daemon unreachable (no fallback).
    Error(String),
}

/// Get resident memory of the current process (macOS/Linux).
fn get_resident_memory() -> u64 {
    #[cfg(target_os = "macos")]
    {
        // macOS: use mach task_info
        // Simplified — return 0 if unavailable
        0
    }
    #[cfg(target_os = "linux")]
    {
        // Linux: read /proc/self/statm
        fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
            .map(|pages| pages * 4096)
            .unwrap_or(0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_paths() -> (TempDir, DaemonPaths) {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path().join(".flow");
        let paths = DaemonPaths::new(&flow_dir);
        (tmp, paths)
    }

    #[test]
    fn acquire_pid_lock_creates_file() {
        let (_tmp, paths) = test_paths();
        acquire_pid_lock(&paths).unwrap();
        assert!(paths.pid_file.exists());
        let contents = fs::read_to_string(&paths.pid_file).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn stale_pid_detected_and_cleaned() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        // Write a PID that definitely doesn't exist (PID 1 is init, use a very high PID)
        fs::write(&paths.pid_file, "999999999").unwrap();
        // Should succeed because the PID is dead
        acquire_pid_lock(&paths).unwrap();
        let contents = fs::read_to_string(&paths.pid_file).unwrap();
        let pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn live_pid_blocks_acquisition() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        // Write our own PID — it's alive
        fs::write(&paths.pid_file, std::process::id().to_string()).unwrap();
        let result = acquire_pid_lock(&paths);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already running"));
    }

    #[test]
    fn corrupt_pid_file_cleaned() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        fs::write(&paths.pid_file, "not-a-number").unwrap();
        acquire_pid_lock(&paths).unwrap();
        assert!(paths.pid_file.exists());
    }

    #[test]
    fn orphaned_socket_cleaned() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        fs::write(&paths.socket_file, "").unwrap();
        cleanup_orphaned_socket(&paths).unwrap();
        assert!(!paths.socket_file.exists());
    }

    #[test]
    fn socket_permissions_set() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        fs::write(&paths.socket_file, "").unwrap();
        set_socket_permissions(&paths.socket_file).unwrap();
        let meta = fs::metadata(&paths.socket_file).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn detect_route_no_daemon() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        assert!(matches!(detect_route(&paths), CliRoute::NoDaemon));
    }

    #[test]
    fn detect_route_stale_pid() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        fs::write(&paths.pid_file, "999999999").unwrap();
        assert!(matches!(detect_route(&paths), CliRoute::Error(_)));
    }

    #[test]
    fn runtime_health_metrics() {
        let (_tmp, paths) = test_paths();
        let runtime = DaemonRuntime::new(paths);
        let health = runtime.health();
        assert_eq!(health.pid, std::process::id());
        assert!(health.uptime_secs < 2);
    }

    #[test]
    fn runtime_cleanup_removes_files() {
        let (_tmp, paths) = test_paths();
        paths.ensure_state_dir().unwrap();
        fs::write(&paths.pid_file, "123").unwrap();
        fs::write(&paths.socket_file, "").unwrap();
        let runtime = DaemonRuntime::new(paths.clone());
        runtime.cleanup();
        assert!(!paths.pid_file.exists());
        assert!(!paths.socket_file.exists());
    }

    #[tokio::test]
    async fn runtime_drain_completes_with_no_tasks() {
        let (_tmp, paths) = test_paths();
        let runtime = DaemonRuntime::new(paths);
        runtime.initiate_shutdown();
        runtime.drain().await.unwrap();
    }

    #[tokio::test]
    async fn runtime_drain_waits_for_tasks() {
        let (_tmp, paths) = test_paths();
        let runtime = DaemonRuntime::new(paths);
        let cancel = runtime.cancel.clone();

        runtime.tracker.spawn(async move {
            // Simulate a short-lived task
            tokio::time::sleep(Duration::from_millis(50)).await;
        });

        runtime.initiate_shutdown();
        runtime.drain().await.unwrap();
        assert!(cancel.is_cancelled());
    }
}
