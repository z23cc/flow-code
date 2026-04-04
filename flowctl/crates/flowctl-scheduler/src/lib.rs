//! flowctl-scheduler: DAG scheduler and event bus for flowctl.
//!
//! Implements Kahn's algorithm with bounded parallelism, heartbeat
//! watchdog, circuit breaker, and broadcast event bus.

pub mod circuit_breaker;
pub mod event_bus;
pub mod scheduler;
pub mod watcher;
pub mod watchdog;

pub use flowctl_core;

// Re-export key types at crate root.
pub use circuit_breaker::CircuitBreaker;
pub use event_bus::{EventBus, FlowEvent, TimestampedEvent};
pub use scheduler::{Scheduler, SchedulerConfig, TaskResult};
pub use watcher::FlowChange;
pub use watchdog::{HeartbeatTable, ZombieAction};
