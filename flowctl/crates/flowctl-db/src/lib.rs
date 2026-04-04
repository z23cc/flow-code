//! flowctl-db: SQLite storage layer for flowctl.
//!
//! Provides connection management, repository abstractions, indexing,
//! and schema migrations for the `.flow/.state/flowctl.db` database.
//!
//! # Architecture
//!
//! - **Markdown is canonical, SQLite is cache.** The `flowctl reindex`
//!   command can fully rebuild the indexed tables from Markdown frontmatter.
//!   Runtime-only data (locks, heartbeats, events, metrics) is not recoverable.
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
pub mod repo;
pub mod sync;

pub use error::DbError;
pub use pool::{cleanup, open, open_memory, resolve_db_path, resolve_state_dir};
pub use indexer::{reindex, ReindexResult};
pub use migration::{migrate_runtime_state, needs_reindex, has_legacy_state, MigrationResult};
pub use repo::{EpicRepo, EvidenceRepo, EventRepo, EventRow, FileLockRepo, PhaseProgressRepo, RuntimeRepo, TaskRepo};
pub use events::EventLog;
pub use metrics::StatsQuery;
pub use sync::{write_epic, write_task, write_task_with_legacy, check_staleness, refresh_if_stale, retry_pending, SyncStatus};

pub use flowctl_core;
