//! Sync shim over `flowctl-db` (async libSQL) providing the same API
//! surface as the deprecated `flowctl-db` (rusqlite) crate.
//!
//! Every sync method spins up a per-call `tokio::runtime::Builder::
//! new_current_thread` runtime, which is cheap for CLI command invocation.
//! The shim exists so the many sync CLI call sites can stay as-is while
//! the underlying storage is async libSQL.
//!
//! This module is the canonical CLI entry point: `crate::commands::db_shim
//! as flowctl_db` (glob-style) is the migration pattern. Do not add
//! long-lived futures or background tasks here.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub use flowctl_db::{DbError, GapRow, ReindexResult};
pub use flowctl_db::metrics::{
    Bottleneck, DoraMetrics, EpicStats, Summary, TokenBreakdown, WeeklyTrend,
};

/// Wrapped libSQL connection. Produced by [`open`]; passed by reference to
/// the repos mirroring the old rusqlite API.
#[derive(Clone)]
pub struct Connection {
    conn: libsql::Connection,
}

impl Connection {
    fn inner(&self) -> libsql::Connection {
        self.conn.clone()
    }

    /// Public accessor for modules that need the raw libsql connection
    /// (e.g. skill commands that call async repos directly).
    pub fn inner_conn(&self) -> libsql::Connection {
        self.conn.clone()
    }
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime")
        .block_on(fut)
}

// ── Pool functions ──────────────────────────────────────────────────

pub fn resolve_state_dir(working_dir: &Path) -> Result<PathBuf, DbError> {
    flowctl_db::resolve_state_dir(working_dir)
}

pub fn resolve_db_path(working_dir: &Path) -> Result<PathBuf, DbError> {
    flowctl_db::resolve_db_path(working_dir)
}

pub fn open(working_dir: &Path) -> Result<Connection, DbError> {
    block_on(async {
        let db = flowctl_db::open_async(working_dir).await?;
        let conn = db.connect()?;
        // Leak the Database handle to keep it alive for the process lifetime.
        // (libsql Database drop closes the file.)
        std::mem::forget(db);
        Ok(Connection { conn })
    })
}

/// Open DB connection with hard error on failure (DB must be available).
/// This is the preferred entry point — all CLI code should use this
/// (DB is the sole source of truth, no fallback path).
pub fn require_db() -> Result<Connection, DbError> {
    let cwd = std::env::current_dir()
        .map_err(|e| DbError::StateDir(format!("cannot get current dir: {e}")))?;
    open(&cwd)
}

pub fn cleanup(conn: &Connection) -> Result<u64, DbError> {
    block_on(flowctl_db::cleanup(&conn.inner()))
}

/// Get the maximum epic number from DB.
pub fn max_epic_num(conn: &Connection) -> Result<i64, DbError> {
    block_on(flowctl_db::max_epic_num(&conn.inner()))
}

/// Get the maximum task number for an epic from DB.
pub fn max_task_num(conn: &Connection, epic_id: &str) -> Result<i64, DbError> {
    block_on(flowctl_db::max_task_num(&conn.inner(), epic_id))
}

pub fn reindex(
    conn: &Connection,
    flow_dir: &Path,
    state_dir: Option<&Path>,
) -> Result<ReindexResult, DbError> {
    block_on(flowctl_db::reindex(&conn.inner(), flow_dir, state_dir))
}

// ── Epic repository ────────────────────────────────────────────────

pub struct EpicRepo(libsql::Connection);

impl EpicRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn get(&self, id: &str) -> Result<flowctl_core::types::Epic, DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).get(id))
    }

    pub fn get_with_body(
        &self,
        id: &str,
    ) -> Result<(flowctl_core::types::Epic, String), DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).get_with_body(id))
    }

    pub fn list(
        &self,
        status: Option<&str>,
    ) -> Result<Vec<flowctl_core::types::Epic>, DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).list(status))
    }

    pub fn upsert(&self, epic: &flowctl_core::types::Epic) -> Result<(), DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).upsert(epic))
    }

    pub fn upsert_with_body(
        &self,
        epic: &flowctl_core::types::Epic,
        body: &str,
    ) -> Result<(), DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).upsert_with_body(epic, body))
    }

    pub fn update_status(
        &self,
        id: &str,
        status: flowctl_core::types::EpicStatus,
    ) -> Result<(), DbError> {
        block_on(flowctl_db::EpicRepo::new(self.0.clone()).update_status(id, status))
    }
}

// ── Task repository ────────────────────────────────────────────────

pub struct TaskRepo(libsql::Connection);

impl TaskRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn get(&self, id: &str) -> Result<flowctl_core::types::Task, DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).get(id))
    }

    pub fn get_with_body(
        &self,
        id: &str,
    ) -> Result<(flowctl_core::types::Task, String), DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).get_with_body(id))
    }

    pub fn list_by_epic(
        &self,
        epic_id: &str,
    ) -> Result<Vec<flowctl_core::types::Task>, DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).list_by_epic(epic_id))
    }

    pub fn list_all(
        &self,
        status: Option<&str>,
        domain: Option<&str>,
    ) -> Result<Vec<flowctl_core::types::Task>, DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).list_all(status, domain))
    }

    pub fn upsert(&self, task: &flowctl_core::types::Task) -> Result<(), DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).upsert(task))
    }

    pub fn upsert_with_body(
        &self,
        task: &flowctl_core::types::Task,
        body: &str,
    ) -> Result<(), DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).upsert_with_body(task, body))
    }

    pub fn update_status(
        &self,
        id: &str,
        status: flowctl_core::state_machine::Status,
    ) -> Result<(), DbError> {
        block_on(flowctl_db::TaskRepo::new(self.0.clone()).update_status(id, status))
    }
}

// ── Dep repository ─────────────────────────────────────────────────

pub struct DepRepo(libsql::Connection);

impl DepRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn add_task_dep(&self, task_id: &str, depends_on: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::DepRepo::new(self.0.clone()).add_task_dep(task_id, depends_on),
        )
    }

    pub fn remove_task_dep(&self, task_id: &str, depends_on: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::DepRepo::new(self.0.clone()).remove_task_dep(task_id, depends_on),
        )
    }

    pub fn list_task_deps(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        block_on(flowctl_db::DepRepo::new(self.0.clone()).list_task_deps(task_id))
    }

    pub fn add_epic_dep(&self, epic_id: &str, depends_on: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::DepRepo::new(self.0.clone()).add_epic_dep(epic_id, depends_on),
        )
    }

    pub fn remove_epic_dep(&self, epic_id: &str, depends_on: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::DepRepo::new(self.0.clone()).remove_epic_dep(epic_id, depends_on),
        )
    }

    pub fn list_epic_deps(&self, epic_id: &str) -> Result<Vec<String>, DbError> {
        block_on(flowctl_db::DepRepo::new(self.0.clone()).list_epic_deps(epic_id))
    }

    /// Replace all deps for a task (delete-all + insert each).
    pub fn replace_task_deps(&self, task_id: &str, deps: &[String]) -> Result<(), DbError> {
        let inner = self.0.clone();
        block_on(async move {
            inner
                .execute(
                    "DELETE FROM task_deps WHERE task_id = ?1",
                    libsql::params![task_id.to_string()],
                )
                .await?;
            for d in deps {
                inner
                    .execute(
                        "INSERT INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                        libsql::params![task_id.to_string(), d.to_string()],
                    )
                    .await?;
            }
            Ok::<(), DbError>(())
        })
    }
}

// ── Runtime repository ─────────────────────────────────────────────

pub struct RuntimeRepo(libsql::Connection);

impl RuntimeRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn get(
        &self,
        task_id: &str,
    ) -> Result<Option<flowctl_core::types::RuntimeState>, DbError> {
        block_on(flowctl_db::RuntimeRepo::new(self.0.clone()).get(task_id))
    }

    pub fn upsert(
        &self,
        state: &flowctl_core::types::RuntimeState,
    ) -> Result<(), DbError> {
        block_on(flowctl_db::RuntimeRepo::new(self.0.clone()).upsert(state))
    }
}

// ── File lock repository ───────────────────────────────────────────

pub struct FileLockRepo(libsql::Connection);

impl FileLockRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn acquire(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::FileLockRepo::new(self.0.clone()).acquire(file_path, task_id),
        )
    }

    pub fn release_for_task(&self, task_id: &str) -> Result<u64, DbError> {
        block_on(flowctl_db::FileLockRepo::new(self.0.clone()).release_for_task(task_id))
    }

    pub fn release_all(&self) -> Result<u64, DbError> {
        block_on(flowctl_db::FileLockRepo::new(self.0.clone()).release_all())
    }

    pub fn check(&self, file_path: &str) -> Result<Option<String>, DbError> {
        block_on(flowctl_db::FileLockRepo::new(self.0.clone()).check(file_path))
    }

    /// List all active locks: (file_path, task_id, locked_at).
    pub fn list_all(&self) -> Result<Vec<(String, String, String)>, DbError> {
        let inner = self.0.clone();
        block_on(async move {
            let mut rows = inner
                .query(
                    "SELECT file_path, task_id, locked_at FROM file_locks ORDER BY file_path",
                    (),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push((
                    row.get::<String>(0)?,
                    row.get::<String>(1)?,
                    row.get::<String>(2)?,
                ));
            }
            Ok(out)
        })
    }
}

// ── Event repository ──────────────────────────────────────────────

pub struct EventRepoSync(libsql::Connection);

impl EventRepoSync {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    /// Return the inner async EventRepo for use with ChangesApplier.
    pub fn as_async(&self) -> flowctl_db::EventRepo {
        flowctl_db::EventRepo::new(self.0.clone())
    }
}

// ── Phase progress repository ──────────────────────────────────────

pub struct PhaseProgressRepo(libsql::Connection);

impl PhaseProgressRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn get_completed(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        block_on(
            flowctl_db::PhaseProgressRepo::new(self.0.clone()).get_completed(task_id),
        )
    }

    pub fn mark_done(&self, task_id: &str, phase: &str) -> Result<(), DbError> {
        block_on(
            flowctl_db::PhaseProgressRepo::new(self.0.clone()).mark_done(task_id, phase),
        )
    }
}

// ── Gap repository ────────────────────────────────────────────────

pub struct GapRepo(libsql::Connection);

impl GapRepo {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn add(
        &self,
        epic_id: &str,
        capability: &str,
        priority: &str,
        source: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<i64, DbError> {
        block_on(
            flowctl_db::GapRepo::new(self.0.clone())
                .add(epic_id, capability, priority, source, task_id),
        )
    }

    pub fn list(
        &self,
        epic_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<GapRow>, DbError> {
        block_on(
            flowctl_db::GapRepo::new(self.0.clone())
                .list(epic_id, status),
        )
    }

    pub fn remove(&self, id: i64) -> Result<(), DbError> {
        block_on(flowctl_db::GapRepo::new(self.0.clone()).remove(id))
    }

    pub fn remove_all(&self, epic_id: &str) -> Result<u64, DbError> {
        block_on(flowctl_db::GapRepo::new(self.0.clone()).remove_all(epic_id))
    }

    pub fn resolve(&self, id: i64, evidence: &str) -> Result<(), DbError> {
        block_on(flowctl_db::GapRepo::new(self.0.clone()).resolve(id, evidence))
    }

    pub fn resolve_by_capability(
        &self,
        epic_id: &str,
        capability: &str,
        evidence: &str,
    ) -> Result<(), DbError> {
        block_on(
            flowctl_db::GapRepo::new(self.0.clone())
                .resolve_by_capability(epic_id, capability, evidence),
        )
    }
}

// ── Stats query ────────────────────────────────────────────────────

pub struct StatsQuery(libsql::Connection);

impl StatsQuery {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.inner())
    }

    pub fn summary(&self) -> Result<Summary, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).summary())
    }

    pub fn per_epic(&self, epic_id: Option<&str>) -> Result<Vec<EpicStats>, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).epic_stats(epic_id))
    }

    pub fn weekly_trends(&self, weeks: u32) -> Result<Vec<WeeklyTrend>, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).weekly_trends(weeks))
    }

    pub fn token_breakdown(
        &self,
        epic_id: Option<&str>,
    ) -> Result<Vec<TokenBreakdown>, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).token_breakdown(epic_id))
    }

    pub fn bottlenecks(&self, limit: usize) -> Result<Vec<Bottleneck>, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).bottlenecks(limit))
    }

    pub fn dora_metrics(&self) -> Result<DoraMetrics, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).dora_metrics())
    }

    pub fn generate_monthly_rollups(&self) -> Result<u64, DbError> {
        block_on(flowctl_db::StatsQuery::new(self.0.clone()).generate_monthly_rollups())
    }
}
