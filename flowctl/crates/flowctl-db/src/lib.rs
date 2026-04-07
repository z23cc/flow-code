//! flowctl-db: Async libSQL storage layer for flowctl.
//!
//! All DB access is async, Tokio-native. Memory table uses libSQL's native
//! vector column (`F32_BLOB(384)`) for semantic search via `vector_top_k`.
//!
//! # Architecture
//!
//! - **libSQL is the single source of truth.** All reads and writes go
//!   through async repository methods. Markdown files are an export format.
//! - **Schema is applied on open** via a single embedded SQL blob, then
//!   migrations run to upgrade existing databases (see `migration.rs`).
//! - **Connections are cheap clones.** `libsql::Connection` is `Send + Sync`,
//!   pass by value. Do not wrap in `Arc<Mutex<_>>`.
//!
//! # History
//!
//! This crate was rewritten from rusqlite to libsql in fn-19 (April 2026).
//! The old rusqlite implementation is no longer available.

pub mod error;
pub mod events;
pub mod indexer;
pub mod memory;
pub mod metrics;
pub mod migration;
pub mod pool;
pub mod repo;
pub mod skill;

pub use error::DbError;
pub use indexer::{reindex, ReindexResult};
pub use events::{EventLog, TaskTokenSummary, TokenRecord, TokenUsageRow};
pub use memory::{MemoryEntry, MemoryFilter, MemoryRepo};
pub use metrics::StatsQuery;
pub use skill::{SkillEntry, SkillMatch, SkillRepo};
pub use pool::{cleanup, open_async, open_memory_async, resolve_db_path, resolve_libsql_path, resolve_state_dir};
pub use repo::{
    DepRepo, EpicRepo, EventRepo, EventRow, EvidenceRepo, FileLockRepo, FileOwnershipRepo,
    GapRepo, GapRow, PhaseProgressRepo, RuntimeRepo, TaskRepo,
    max_epic_num, max_task_num,
};

// Re-export libsql types for callers.
pub use libsql::{Connection, Database};
