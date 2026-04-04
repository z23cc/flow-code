//! flowctl-daemon: Background daemon process for flowctl.
//!
//! Provides the DAG scheduler, file watcher, heartbeat watchdog,
//! circuit breaker, and HTTP API over Unix socket.

pub use flowctl_core;
