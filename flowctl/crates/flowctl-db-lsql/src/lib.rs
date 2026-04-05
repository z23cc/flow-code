//! flowctl-db-lsql: Async libSQL storage layer for flowctl.
//!
//! Successor to `flowctl-db` (rusqlite-based). All DB access is async,
//! Tokio-native. Memory table uses libSQL's native vector column
//! (`F32_BLOB(384)`) for semantic search via `vector_top_k`.
//!
//! # Architecture
//!
//! - **libSQL is the single source of truth.** All reads and writes go
//!   through async repository methods. Markdown files are an export format.
//! - **Schema is applied on open** via a single embedded SQL blob. No
//!   migrations — this crate assumes a fresh DB.
//! - **Connections are cheap clones.** `libsql::Connection` is `Send + Sync`,
//!   pass by value. Do not wrap in `Arc<Mutex<_>>`.
//!
//! # Why a separate crate?
//!
//! libsql 0.9 cannot coexist with `rusqlite(bundled)` in the same test
//! binary — their C-level static init collides. Keeping the new stack in
//! its own crate gives clean test isolation during migration.

pub mod error;
pub mod pool;
pub mod repo;

pub use error::DbError;
pub use pool::{open_async, open_memory_async, resolve_libsql_path};
pub use repo::{
    DepRepo, EpicRepo, EvidenceRepo, FileLockRepo, FileOwnershipRepo, PhaseProgressRepo,
    RuntimeRepo, TaskRepo,
};

// Re-export libsql types for callers.
pub use libsql::{Connection, Database};
