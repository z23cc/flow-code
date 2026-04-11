//! flowctl MCP server — goal-driven adaptive engine over Model Context Protocol.
//!
//! This crate provides an MCP server (stdio transport) that exposes 16 tools
//! for goal-driven development. It wraps the synchronous `flowctl-core` engine
//! using `tokio::task::spawn_blocking`.
//!
//! See docs/v3-final-architecture.md for the full design.

#![forbid(unsafe_code)]

pub mod server;

pub use server::{FlowctlServer, run_server};
