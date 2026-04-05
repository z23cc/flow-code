//! flowctl-service: Business logic service layer for flowctl.
//!
//! This crate provides the canonical business logic that is shared across
//! all three execution paths (CLI, daemon, MCP). It sits between the
//! transport layer (HTTP handlers, CLI commands, MCP protocol) and the
//! storage layer (flowctl-db).
//!
//! # Architecture
//!
//! ```text
//!   CLI commands ─┐
//!   HTTP handlers ─┼─► flowctl-service ──► flowctl-db ──► SQLite
//!   MCP server ───┘         │
//!                    flowctl-core (types, DAG, state machine)
//! ```
//!
//! # Connection management
//!
//! `libsql::Connection` is `Send + Sync` and cheap to `Clone`. All service
//! functions are async and accept the connection by reference.

pub mod connection;
pub mod error;
pub mod lifecycle;

// Re-export key types at crate root.
pub use connection::{open_async, FileConnectionProvider};
pub use error::{ServiceError, ServiceResult};
