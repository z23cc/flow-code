//! flowctl-service: Business logic service layer for flowctl.
//!
//! This crate provides the canonical business logic that is shared across
//! CLI and MCP execution paths. It sits between the transport layer
//! (CLI commands, MCP protocol) and the storage layer (flowctl-db).
//!
//! # Architecture
//!
//! ```text
//!   CLI commands ─┐
//!   MCP server ───┴─► flowctl-service ──► flowctl-db ──► JSON files
//!                            │
//!                     flowctl-core (types, DAG, state machine)
//! ```
//!
//! All operations are synchronous, using file-based storage.

pub mod approvals;
pub mod changes;
pub mod error;
pub mod lifecycle;
pub mod outputs;

// Re-export key types at crate root.
pub use error::{ServiceError, ServiceResult};
