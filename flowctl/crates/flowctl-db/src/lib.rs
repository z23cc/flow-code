//! flowctl-db: Sync file-based storage layer for flowctl.
//!
//! All I/O is synchronous, delegating to `flowctl_core::json_store`.
//! No async runtime required — pure synchronous file I/O.
//!
//! # Architecture
//!
//! - `FlowStore` is the main entry point, wrapping a `.flow/` directory path.
//! - Sub-stores (`EventStore`, `PipelineStore`, etc.) are accessed via methods.
//! - All data lives as JSON files in the `.flow/` directory tree.

pub mod approvals;
pub mod error;
pub mod events;
pub mod gaps;
pub mod locks;
pub mod memory;
pub mod phases;
pub mod pipeline;
pub mod store;

pub use error::DbError;
pub use store::FlowStore;

// Re-export sub-store types for convenience.
pub use approvals::ApprovalStore;
pub use events::EventStore;
pub use gaps::{GapEntry, GapStore};
pub use locks::{LockEntry, LockStore};
pub use memory::MemoryStore;
pub use phases::PhaseStore;
pub use pipeline::PipelineStore;

// Re-export json_store types that callers may need.
pub use flowctl_core::json_store::TaskState;
