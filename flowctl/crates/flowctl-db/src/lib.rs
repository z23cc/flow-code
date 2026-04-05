//! flowctl-db: SQLite storage layer for flowctl.
//!
//! Provides connection management, repository abstractions, indexing,
//! and schema migrations for the `.flow/.state/flowctl.db` database.
//!
//! # Architecture
//!
//! - **SQLite is the single source of truth.** All reads and writes go through
//!   the repository layer. Markdown files are an export format (`flowctl export`).
//!   `flowctl import` (reindex) rebuilds the DB from Markdown for migration.
//!
//! - **PRAGMAs are per-connection**, not in migration files. WAL mode,
//!   busy_timeout, and foreign_keys are set on every connection open.
//!
//! - **State directory**: resolved via `git rev-parse --git-common-dir`
//!   so worktrees share a single database file.

pub mod error;
pub mod events;
pub mod indexer;
pub mod metrics;
pub mod migration;
pub mod pool;
pub mod pool_async;
pub mod repo;

pub use error::DbError;
pub use pool::{cleanup, open, open_memory, resolve_db_path, resolve_state_dir};
pub use pool_async::{open_async, open_memory_async, resolve_libsql_path};

// Re-export libsql types for callers that need them during migration.
pub use libsql::{Connection as AsyncConnection, Database as AsyncDatabase};
pub use indexer::{reindex, ReindexResult};
pub use migration::{migrate_runtime_state, needs_reindex, has_legacy_state, MigrationResult};
pub use repo::{EpicRepo, EvidenceRepo, EventRepo, EventRow, FileLockRepo, PhaseProgressRepo, RuntimeRepo, TaskRepo};
pub use events::{EventLog, TaskTokenSummary, TokenRecord, TokenUsageRow};
pub use metrics::StatsQuery;

pub use flowctl_core;
