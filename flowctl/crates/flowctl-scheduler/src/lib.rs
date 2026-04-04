//! flowctl-scheduler: DAG scheduler and event bus for flowctl.
//!
//! Implements Kahn's algorithm with bounded parallelism, heartbeat
//! watchdog, circuit breaker, and broadcast event bus.

pub use flowctl_core;
