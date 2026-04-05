//! Async repository abstractions over libSQL.
//!
//! Ported from `flowctl-db::repo` (sync rusqlite). Each repo owns a
//! `libsql::Connection` (cheap Clone) and exposes async methods that
//! return `DbError`. Mirrors the sync API surface where it makes sense.

use chrono::{DateTime, Utc};
use libsql::{params, Connection};

use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, Epic, EpicStatus, Evidence, ReviewStatus, RuntimeState, Task};

use crate::error::DbError;

// ── Parsing helpers ─────────────────────────────────────────────────

fn parse_status(s: &str) -> Status {
    Status::parse(s).unwrap_or_default()
}

fn parse_epic_status(s: &str) -> EpicStatus {
    match s {
        "done" => EpicStatus::Done,
        _ => EpicStatus::Open,
    }
}

fn parse_review_status(s: &str) -> ReviewStatus {
    match s {
        "passed" => ReviewStatus::Passed,
        "failed" => ReviewStatus::Failed,
        _ => ReviewStatus::Unknown,
    }
}

fn parse_domain(s: &str) -> Domain {
    match s {
        "frontend" => Domain::Frontend,
        "backend" => Domain::Backend,
        "architecture" => Domain::Architecture,
        "testing" => Domain::Testing,
        "docs" => Domain::Docs,
        "ops" => Domain::Ops,
        _ => Domain::General,
    }
}

fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

// ── Epic repository ─────────────────────────────────────────────────

/// Async repository for epic CRUD operations.
pub struct EpicRepo {
    conn: Connection,
}

impl EpicRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace an epic (empty body preserves existing body).
    pub async fn upsert(&self, epic: &Epic) -> Result<(), DbError> {
        self.upsert_with_body(epic, "").await
    }

    /// Insert or replace an epic with its markdown body.
    pub async fn upsert_with_body(&self, epic: &Epic, body: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO epics (id, title, status, branch_name, plan_review, file_path, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                     title = excluded.title,
                     status = excluded.status,
                     branch_name = excluded.branch_name,
                     plan_review = excluded.plan_review,
                     file_path = excluded.file_path,
                     body = CASE WHEN excluded.body = '' THEN epics.body ELSE excluded.body END,
                     updated_at = excluded.updated_at",
                params![
                    epic.id.clone(),
                    epic.title.clone(),
                    epic.status.to_string(),
                    epic.branch_name.clone(),
                    epic.plan_review.to_string(),
                    epic.file_path.clone().unwrap_or_default(),
                    body.to_string(),
                    epic.created_at.to_rfc3339(),
                    epic.updated_at.to_rfc3339(),
                ],
            )
            .await?;

        // Upsert epic dependencies.
        self.conn
            .execute(
                "DELETE FROM epic_deps WHERE epic_id = ?1",
                params![epic.id.clone()],
            )
            .await?;
        for dep in &epic.depends_on_epics {
            self.conn
                .execute(
                    "INSERT INTO epic_deps (epic_id, depends_on) VALUES (?1, ?2)",
                    params![epic.id.clone(), dep.clone()],
                )
                .await?;
        }

        Ok(())
    }

    /// Get an epic by ID.
    pub async fn get(&self, id: &str) -> Result<Epic, DbError> {
        self.get_with_body(id).await.map(|(epic, _)| epic)
    }

    /// Get an epic by ID, returning (Epic, body).
    pub async fn get_with_body(&self, id: &str) -> Result<(Epic, String), DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, title, status, branch_name, plan_review, file_path, created_at, updated_at, COALESCE(body, '')
                 FROM epics WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| DbError::NotFound(format!("epic: {id}")))?;

        let status_s: String = row.get(2)?;
        let plan_s: String = row.get(4)?;
        let created_s: String = row.get(6)?;
        let updated_s: String = row.get(7)?;

        let epic = Epic {
            schema_version: 1,
            id: row.get::<String>(0)?,
            title: row.get::<String>(1)?,
            status: parse_epic_status(&status_s),
            branch_name: row.get::<Option<String>>(3)?,
            plan_review: parse_review_status(&plan_s),
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: Vec::new(),
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: row.get::<Option<String>>(5)?,
            created_at: parse_datetime(&created_s),
            updated_at: parse_datetime(&updated_s),
        };
        let body: String = row.get::<String>(8)?;

        let deps = self.get_deps(&epic.id).await?;
        Ok((
            Epic {
                depends_on_epics: deps,
                ..epic
            },
            body,
        ))
    }

    /// List all epics, optionally filtered by status.
    pub async fn list(&self, status: Option<&str>) -> Result<Vec<Epic>, DbError> {
        let mut rows = match status {
            Some(s) => {
                self.conn
                    .query(
                        "SELECT id FROM epics WHERE status = ?1 ORDER BY created_at",
                        params![s.to_string()],
                    )
                    .await?
            }
            None => {
                self.conn
                    .query("SELECT id FROM epics ORDER BY created_at", ())
                    .await?
            }
        };

        let mut ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in &ids {
            out.push(self.get(id).await?);
        }
        Ok(out)
    }

    /// Update epic status.
    pub async fn update_status(&self, id: &str, status: EpicStatus) -> Result<(), DbError> {
        let rows = self
            .conn
            .execute(
                "UPDATE epics SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.to_string(), Utc::now().to_rfc3339(), id.to_string()],
            )
            .await?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("epic: {id}")));
        }
        Ok(())
    }

    /// Delete an epic and its dep rows.
    pub async fn delete(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "DELETE FROM epic_deps WHERE epic_id = ?1",
                params![id.to_string()],
            )
            .await?;
        self.conn
            .execute("DELETE FROM epics WHERE id = ?1", params![id.to_string()])
            .await?;
        Ok(())
    }

    async fn get_deps(&self, epic_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT depends_on FROM epic_deps WHERE epic_id = ?1",
                params![epic_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }
}

// ── Task repository ─────────────────────────────────────────────────

/// Async repository for task CRUD operations.
pub struct TaskRepo {
    conn: Connection,
}

impl TaskRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace a task (empty body preserves existing body).
    pub async fn upsert(&self, task: &Task) -> Result<(), DbError> {
        self.upsert_with_body(task, "").await
    }

    /// Insert or replace a task with its markdown body.
    pub async fn upsert_with_body(&self, task: &Task, body: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO tasks (id, epic_id, title, status, priority, domain, file_path, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                     title = excluded.title,
                     status = excluded.status,
                     priority = excluded.priority,
                     domain = excluded.domain,
                     file_path = excluded.file_path,
                     body = CASE WHEN excluded.body = '' THEN tasks.body ELSE excluded.body END,
                     updated_at = excluded.updated_at",
                params![
                    task.id.clone(),
                    task.epic.clone(),
                    task.title.clone(),
                    task.status.to_string(),
                    task.sort_priority() as i64,
                    task.domain.to_string(),
                    task.file_path.clone().unwrap_or_default(),
                    body.to_string(),
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                ],
            )
            .await?;

        // Upsert dependencies.
        self.conn
            .execute(
                "DELETE FROM task_deps WHERE task_id = ?1",
                params![task.id.clone()],
            )
            .await?;
        for dep in &task.depends_on {
            self.conn
                .execute(
                    "INSERT INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                    params![task.id.clone(), dep.clone()],
                )
                .await?;
        }

        // Upsert file ownership.
        self.conn
            .execute(
                "DELETE FROM file_ownership WHERE task_id = ?1",
                params![task.id.clone()],
            )
            .await?;
        for file in &task.files {
            self.conn
                .execute(
                    "INSERT INTO file_ownership (file_path, task_id) VALUES (?1, ?2)",
                    params![file.clone(), task.id.clone()],
                )
                .await?;
        }

        Ok(())
    }

    /// Get a task by ID.
    pub async fn get(&self, id: &str) -> Result<Task, DbError> {
        self.get_with_body(id).await.map(|(task, _)| task)
    }

    /// Get a task by ID, returning (Task, body).
    pub async fn get_with_body(&self, id: &str) -> Result<(Task, String), DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, epic_id, title, status, priority, domain, file_path, created_at, updated_at, COALESCE(body, '')
                 FROM tasks WHERE id = ?1",
                params![id.to_string()],
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| DbError::NotFound(format!("task: {id}")))?;

        let status_s: String = row.get(3)?;
        let domain_s: String = row.get(5)?;
        let created_s: String = row.get(7)?;
        let updated_s: String = row.get(8)?;
        let priority_val: i64 = row.get(4)?;
        let priority = if priority_val == 999 {
            None
        } else {
            Some(priority_val as u32)
        };

        let task = Task {
            schema_version: 1,
            id: row.get::<String>(0)?,
            epic: row.get::<String>(1)?,
            title: row.get::<String>(2)?,
            status: parse_status(&status_s),
            priority,
            domain: parse_domain(&domain_s),
            depends_on: Vec::new(),
            files: Vec::new(),
            r#impl: None,
            review: None,
            sync: None,
            file_path: row.get::<Option<String>>(6)?,
            created_at: parse_datetime(&created_s),
            updated_at: parse_datetime(&updated_s),
        };
        let body: String = row.get::<String>(9)?;

        let deps = self.get_deps(&task.id).await?;
        let files = self.get_files(&task.id).await?;
        Ok((
            Task {
                depends_on: deps,
                files,
                ..task
            },
            body,
        ))
    }

    /// List tasks for an epic.
    pub async fn list_by_epic(&self, epic_id: &str) -> Result<Vec<Task>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id FROM tasks WHERE epic_id = ?1 ORDER BY priority, id",
                params![epic_id.to_string()],
            )
            .await?;

        let mut ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in &ids {
            out.push(self.get(id).await?);
        }
        Ok(out)
    }

    /// List all tasks, optionally filtered by status and/or domain.
    pub async fn list_all(
        &self,
        status: Option<&str>,
        domain: Option<&str>,
    ) -> Result<Vec<Task>, DbError> {
        let mut rows = match (status, domain) {
            (Some(s), Some(d)) => {
                self.conn
                    .query(
                        "SELECT id FROM tasks WHERE status = ?1 AND domain = ?2 ORDER BY epic_id, priority, id",
                        params![s.to_string(), d.to_string()],
                    )
                    .await?
            }
            (Some(s), None) => {
                self.conn
                    .query(
                        "SELECT id FROM tasks WHERE status = ?1 ORDER BY epic_id, priority, id",
                        params![s.to_string()],
                    )
                    .await?
            }
            (None, Some(d)) => {
                self.conn
                    .query(
                        "SELECT id FROM tasks WHERE domain = ?1 ORDER BY epic_id, priority, id",
                        params![d.to_string()],
                    )
                    .await?
            }
            (None, None) => {
                self.conn
                    .query("SELECT id FROM tasks ORDER BY epic_id, priority, id", ())
                    .await?
            }
        };

        let mut ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in &ids {
            out.push(self.get(id).await?);
        }
        Ok(out)
    }

    /// List tasks filtered by status.
    pub async fn list_by_status(&self, status: Status) -> Result<Vec<Task>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id FROM tasks WHERE status = ?1 ORDER BY priority, id",
                params![status.to_string()],
            )
            .await?;
        let mut ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get::<String>(0)?);
        }

        let mut out = Vec::with_capacity(ids.len());
        for id in &ids {
            out.push(self.get(id).await?);
        }
        Ok(out)
    }

    /// Update task status.
    pub async fn update_status(&self, id: &str, status: Status) -> Result<(), DbError> {
        let rows = self
            .conn
            .execute(
                "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.to_string(), Utc::now().to_rfc3339(), id.to_string()],
            )
            .await?;
        if rows == 0 {
            return Err(DbError::NotFound(format!("task: {id}")));
        }
        Ok(())
    }

    /// Delete a task and all related data.
    pub async fn delete(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "DELETE FROM task_deps WHERE task_id = ?1",
                params![id.to_string()],
            )
            .await?;
        self.conn
            .execute(
                "DELETE FROM file_ownership WHERE task_id = ?1",
                params![id.to_string()],
            )
            .await?;
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])
            .await?;
        Ok(())
    }

    async fn get_deps(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT depends_on FROM task_deps WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }

    async fn get_files(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT file_path FROM file_ownership WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }
}

// ── Dependency repository ───────────────────────────────────────────

/// Async repository for task and epic dependency edges.
pub struct DepRepo {
    conn: Connection,
}

impl DepRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn add_task_dep(&self, task_id: &str, depends_on: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                params![task_id.to_string(), depends_on.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn remove_task_dep(&self, task_id: &str, depends_on: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "DELETE FROM task_deps WHERE task_id = ?1 AND depends_on = ?2",
                params![task_id.to_string(), depends_on.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn list_task_deps(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT depends_on FROM task_deps WHERE task_id = ?1 ORDER BY depends_on",
                params![task_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }

    pub async fn add_epic_dep(&self, epic_id: &str, depends_on: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO epic_deps (epic_id, depends_on) VALUES (?1, ?2)",
                params![epic_id.to_string(), depends_on.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn remove_epic_dep(&self, epic_id: &str, depends_on: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "DELETE FROM epic_deps WHERE epic_id = ?1 AND depends_on = ?2",
                params![epic_id.to_string(), depends_on.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn list_epic_deps(&self, epic_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT depends_on FROM epic_deps WHERE epic_id = ?1 ORDER BY depends_on",
                params![epic_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }
}

// ── File ownership repository ───────────────────────────────────────

/// Async repository for file ownership edges.
pub struct FileOwnershipRepo {
    conn: Connection,
}

impl FileOwnershipRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub async fn add(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO file_ownership (file_path, task_id) VALUES (?1, ?2)",
                params![file_path.to_string(), task_id.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn remove(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "DELETE FROM file_ownership WHERE file_path = ?1 AND task_id = ?2",
                params![file_path.to_string(), task_id.to_string()],
            )
            .await?;
        Ok(())
    }

    pub async fn list_for_task(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT file_path FROM file_ownership WHERE task_id = ?1 ORDER BY file_path",
                params![task_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }

    pub async fn list_for_file(&self, file_path: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id FROM file_ownership WHERE file_path = ?1 ORDER BY task_id",
                params![file_path.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }
}

// ── Runtime-state repository ────────────────────────────────────────

/// Async repository for per-task runtime state (Teams mode assignment, timing).
pub struct RuntimeRepo {
    conn: Connection,
}

impl RuntimeRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Upsert runtime state for a task.
    pub async fn upsert(&self, state: &RuntimeState) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO runtime_state (task_id, assignee, claimed_at, completed_at, duration_secs, blocked_reason, baseline_rev, final_rev, retry_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(task_id) DO UPDATE SET
                     assignee = excluded.assignee,
                     claimed_at = excluded.claimed_at,
                     completed_at = excluded.completed_at,
                     duration_secs = excluded.duration_secs,
                     blocked_reason = excluded.blocked_reason,
                     baseline_rev = excluded.baseline_rev,
                     final_rev = excluded.final_rev,
                     retry_count = excluded.retry_count",
                params![
                    state.task_id.clone(),
                    state.assignee.clone(),
                    state.claimed_at.map(|dt| dt.to_rfc3339()),
                    state.completed_at.map(|dt| dt.to_rfc3339()),
                    state.duration_secs.map(|d| d as i64),
                    state.blocked_reason.clone(),
                    state.baseline_rev.clone(),
                    state.final_rev.clone(),
                    state.retry_count as i64,
                ],
            )
            .await?;
        Ok(())
    }

    /// Get runtime state for a task.
    pub async fn get(&self, task_id: &str) -> Result<Option<RuntimeState>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id, assignee, claimed_at, completed_at, duration_secs, blocked_reason, baseline_rev, final_rev, retry_count
                 FROM runtime_state WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let claimed_s: Option<String> = row.get::<Option<String>>(2)?;
        let completed_s: Option<String> = row.get::<Option<String>>(3)?;
        let duration: Option<i64> = row.get::<Option<i64>>(4)?;
        let retry: i64 = row.get::<i64>(8)?;

        Ok(Some(RuntimeState {
            task_id: row.get::<String>(0)?,
            assignee: row.get::<Option<String>>(1)?,
            claimed_at: claimed_s.as_deref().map(parse_datetime),
            completed_at: completed_s.as_deref().map(parse_datetime),
            duration_secs: duration.map(|d| d as u64),
            blocked_reason: row.get::<Option<String>>(5)?,
            baseline_rev: row.get::<Option<String>>(6)?,
            final_rev: row.get::<Option<String>>(7)?,
            retry_count: retry as u32,
        }))
    }
}

// ── Evidence repository ─────────────────────────────────────────────

/// Async repository for task completion evidence.
pub struct EvidenceRepo {
    conn: Connection,
}

impl EvidenceRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Upsert evidence for a task. Commits and tests are stored as JSON arrays.
    pub async fn upsert(&self, task_id: &str, evidence: &Evidence) -> Result<(), DbError> {
        let commits_json = if evidence.commits.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&evidence.commits)?)
        };
        let tests_json = if evidence.tests.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&evidence.tests)?)
        };

        self.conn
            .execute(
                "INSERT INTO evidence (task_id, commits, tests, files_changed, insertions, deletions, review_iters)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(task_id) DO UPDATE SET
                     commits = excluded.commits,
                     tests = excluded.tests,
                     files_changed = excluded.files_changed,
                     insertions = excluded.insertions,
                     deletions = excluded.deletions,
                     review_iters = excluded.review_iters",
                params![
                    task_id.to_string(),
                    commits_json,
                    tests_json,
                    evidence.files_changed.map(|v| v as i64),
                    evidence.insertions.map(|v| v as i64),
                    evidence.deletions.map(|v| v as i64),
                    evidence.review_iterations.map(|v| v as i64),
                ],
            )
            .await?;
        Ok(())
    }

    /// Get evidence for a task.
    pub async fn get(&self, task_id: &str) -> Result<Option<Evidence>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT commits, tests, files_changed, insertions, deletions, review_iters
                 FROM evidence WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let commits_json: Option<String> = row.get::<Option<String>>(0)?;
        let tests_json: Option<String> = row.get::<Option<String>>(1)?;
        let files_changed: Option<i64> = row.get::<Option<i64>>(2)?;
        let insertions: Option<i64> = row.get::<Option<i64>>(3)?;
        let deletions: Option<i64> = row.get::<Option<i64>>(4)?;
        let review_iters: Option<i64> = row.get::<Option<i64>>(5)?;

        let commits: Vec<String> = commits_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        let tests: Vec<String> = tests_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();

        Ok(Some(Evidence {
            commits,
            tests,
            prs: Vec::new(),
            files_changed: files_changed.map(|v| v as u32),
            insertions: insertions.map(|v| v as u32),
            deletions: deletions.map(|v| v as u32),
            review_iterations: review_iters.map(|v| v as u32),
            workspace_changes: None,
        }))
    }
}

// ── File lock repository (Teams mode) ───────────────────────────────

/// Async repository for runtime file locks. Load-bearing for Teams-mode
/// concurrency: `acquire` on an already-locked file returns
/// `DbError::Constraint`.
pub struct FileLockRepo {
    conn: Connection,
}

impl FileLockRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Acquire a lock on a file for a task. Returns `DbError::Constraint`
    /// if the file is already locked by another task.
    pub async fn acquire(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        let res = self
            .conn
            .execute(
                "INSERT INTO file_locks (file_path, task_id, locked_at) VALUES (?1, ?2, ?3)",
                params![
                    file_path.to_string(),
                    task_id.to_string(),
                    Utc::now().to_rfc3339(),
                ],
            )
            .await;

        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string();
                let low = msg.to_lowercase();
                if low.contains("unique constraint")
                    || low.contains("constraint failed")
                    || low.contains("primary key")
                {
                    Err(DbError::Constraint(format!(
                        "file already locked: {file_path}"
                    )))
                } else {
                    Err(DbError::LibSql(e))
                }
            }
        }
    }

    /// Release locks held by a task. Returns number of rows deleted.
    pub async fn release_for_task(&self, task_id: &str) -> Result<u64, DbError> {
        let n = self
            .conn
            .execute(
                "DELETE FROM file_locks WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;
        Ok(n)
    }

    /// Release all locks (between waves). Returns number of rows deleted.
    pub async fn release_all(&self) -> Result<u64, DbError> {
        let n = self.conn.execute("DELETE FROM file_locks", ()).await?;
        Ok(n)
    }

    /// Check if a file is locked. Returns the locking task_id if so.
    pub async fn check(&self, file_path: &str) -> Result<Option<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id FROM file_locks WHERE file_path = ?1",
                params![file_path.to_string()],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(row.get::<String>(0)?))
        } else {
            Ok(None)
        }
    }
}

// ── Phase progress repository ───────────────────────────────────────

/// Async repository for worker-phase progress tracking.
pub struct PhaseProgressRepo {
    conn: Connection,
}

impl PhaseProgressRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Get all completed phases for a task, in rowid (insertion) order.
    pub async fn get_completed(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT phase FROM phase_progress WHERE task_id = ?1 AND status = 'done' ORDER BY rowid",
                params![task_id.to_string()],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row.get::<String>(0)?);
        }
        Ok(out)
    }

    /// Mark a phase as done.
    pub async fn mark_done(&self, task_id: &str, phase: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO phase_progress (task_id, phase, status, completed_at)
                 VALUES (?1, ?2, 'done', ?3)
                 ON CONFLICT(task_id, phase) DO UPDATE SET
                     status = 'done',
                     completed_at = excluded.completed_at",
                params![
                    task_id.to_string(),
                    phase.to_string(),
                    Utc::now().to_rfc3339(),
                ],
            )
            .await?;
        Ok(())
    }

    /// Reset all phase progress for a task. Returns number of rows deleted.
    pub async fn reset(&self, task_id: &str) -> Result<u64, DbError> {
        let n = self
            .conn
            .execute(
                "DELETE FROM phase_progress WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;
        Ok(n)
    }
}

// ── Event repository ────────────────────────────────────────────────

/// A row from the events table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventRow {
    pub id: i64,
    pub timestamp: String,
    pub epic_id: String,
    pub task_id: Option<String>,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload: Option<String>,
    pub session_id: Option<String>,
}

/// Async repository for the append-only event log.
pub struct EventRepo {
    conn: Connection,
}

impl EventRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Record an event. Returns the inserted rowid.
    pub async fn insert(
        &self,
        epic_id: &str,
        task_id: Option<&str>,
        event_type: &str,
        actor: Option<&str>,
        payload: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<i64, DbError> {
        self.conn
            .execute(
                "INSERT INTO events (epic_id, task_id, event_type, actor, payload, session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    epic_id.to_string(),
                    task_id.map(|s| s.to_string()),
                    event_type.to_string(),
                    actor.map(|s| s.to_string()),
                    payload.map(|s| s.to_string()),
                    session_id.map(|s| s.to_string()),
                ],
            )
            .await?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List recent events for an epic (most recent first).
    pub async fn list_by_epic(
        &self,
        epic_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
                 FROM events WHERE epic_id = ?1 ORDER BY id DESC LIMIT ?2",
                params![epic_id.to_string(), limit as i64],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(EventRow {
                id: row.get::<i64>(0)?,
                timestamp: row.get::<String>(1)?,
                epic_id: row.get::<String>(2)?,
                task_id: row.get::<Option<String>>(3)?,
                event_type: row.get::<String>(4)?,
                actor: row.get::<Option<String>>(5)?,
                payload: row.get::<Option<String>>(6)?,
                session_id: row.get::<Option<String>>(7)?,
            });
        }
        Ok(out)
    }

    /// List recent events of a given type across all epics.
    pub async fn list_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
                 FROM events WHERE event_type = ?1 ORDER BY id DESC LIMIT ?2",
                params![event_type.to_string(), limit as i64],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(EventRow {
                id: row.get::<i64>(0)?,
                timestamp: row.get::<String>(1)?,
                epic_id: row.get::<String>(2)?,
                task_id: row.get::<Option<String>>(3)?,
                event_type: row.get::<String>(4)?,
                actor: row.get::<Option<String>>(5)?,
                payload: row.get::<Option<String>>(6)?,
                session_id: row.get::<Option<String>>(7)?,
            });
        }
        Ok(out)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;
    use chrono::Utc;
    use flowctl_core::types::{Domain, Epic, EpicStatus, ReviewStatus, Task};
    use flowctl_core::state_machine::Status;

    fn sample_epic(id: &str) -> Epic {
        let now = Utc::now();
        Epic {
            schema_version: 1,
            id: id.to_string(),
            title: format!("Title of {id}"),
            status: EpicStatus::Open,
            branch_name: Some("feat/x".to_string()),
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: Vec::new(),
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some(format!("epics/{id}.md")),
            created_at: now,
            updated_at: now,
        }
    }

    fn sample_task(epic: &str, id: &str) -> Task {
        let now = Utc::now();
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: epic.to_string(),
            title: format!("Task {id}"),
            status: Status::Todo,
            priority: Some(1),
            domain: Domain::Backend,
            depends_on: Vec::new(),
            files: Vec::new(),
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some(format!("tasks/{id}.md")),
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn epic_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());

        let e = sample_epic("fn-1-test");
        repo.upsert(&e).await.unwrap();

        let got = repo.get("fn-1-test").await.unwrap();
        assert_eq!(got.id, "fn-1-test");
        assert_eq!(got.title, "Title of fn-1-test");
        assert_eq!(got.branch_name.as_deref(), Some("feat/x"));
        assert!(matches!(got.status, EpicStatus::Open));
    }

    #[tokio::test]
    async fn epic_upsert_with_body_preserves() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());
        let e = sample_epic("fn-2-body");

        repo.upsert_with_body(&e, "# Body v1").await.unwrap();
        let (_, body) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body, "# Body v1");

        // Empty body preserves existing.
        repo.upsert_with_body(&e, "").await.unwrap();
        let (_, body2) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body2, "# Body v1");

        // Non-empty overwrites.
        repo.upsert_with_body(&e, "# Body v2").await.unwrap();
        let (_, body3) = repo.get_with_body("fn-2-body").await.unwrap();
        assert_eq!(body3, "# Body v2");
    }

    #[tokio::test]
    async fn epic_list_and_update_status_and_delete() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());

        repo.upsert(&sample_epic("fn-a")).await.unwrap();
        repo.upsert(&sample_epic("fn-b")).await.unwrap();

        let all = repo.list(None).await.unwrap();
        assert_eq!(all.len(), 2);

        repo.update_status("fn-a", EpicStatus::Done).await.unwrap();
        let done = repo.list(Some("done")).await.unwrap();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "fn-a");

        repo.delete("fn-b").await.unwrap();
        let remaining = repo.list(None).await.unwrap();
        assert_eq!(remaining.len(), 1);

        let err = repo.get("nope").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn epic_get_missing_is_not_found() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EpicRepo::new(conn.clone());
        let err = repo.get("does-not-exist").await.unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn task_upsert_get_with_deps_and_files() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let erepo = EpicRepo::new(conn.clone());
        erepo.upsert(&sample_epic("fn-1")).await.unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t = sample_task("fn-1", "fn-1.1");
        t.depends_on = vec!["fn-1.0".to_string()];
        t.files = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
        trepo.upsert(&t).await.unwrap();

        let got = trepo.get("fn-1.1").await.unwrap();
        assert_eq!(got.epic, "fn-1");
        assert_eq!(got.priority, Some(1));
        assert!(matches!(got.domain, Domain::Backend));
        assert_eq!(got.depends_on, vec!["fn-1.0".to_string()]);
        assert_eq!(got.files.len(), 2);
        assert!(got.files.contains(&"src/a.rs".to_string()));
    }

    #[tokio::test]
    async fn task_list_by_epic_status_domain() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let erepo = EpicRepo::new(conn.clone());
        erepo.upsert(&sample_epic("fn-1")).await.unwrap();
        erepo.upsert(&sample_epic("fn-2")).await.unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t1 = sample_task("fn-1", "fn-1.1");
        let mut t2 = sample_task("fn-1", "fn-1.2");
        t2.domain = Domain::Frontend;
        let t3 = sample_task("fn-2", "fn-2.1");
        trepo.upsert(&t1).await.unwrap();
        trepo.upsert(&t2).await.unwrap();
        trepo.upsert(&t3).await.unwrap();

        let ep1 = trepo.list_by_epic("fn-1").await.unwrap();
        assert_eq!(ep1.len(), 2);

        let all = trepo.list_all(None, None).await.unwrap();
        assert_eq!(all.len(), 3);

        let fe = trepo.list_all(None, Some("frontend")).await.unwrap();
        assert_eq!(fe.len(), 1);
        assert_eq!(fe[0].id, "fn-1.2");

        t1.status = Status::Done;
        trepo.upsert(&t1).await.unwrap();
        let done = trepo.list_by_status(Status::Done).await.unwrap();
        assert_eq!(done.len(), 1);

        let todo_fe = trepo
            .list_all(Some("todo"), Some("frontend"))
            .await
            .unwrap();
        assert_eq!(todo_fe.len(), 1);
    }

    #[tokio::test]
    async fn task_update_status_and_delete() {
        let (_db, conn) = open_memory_async().await.unwrap();
        EpicRepo::new(conn.clone())
            .upsert(&sample_epic("fn-1"))
            .await
            .unwrap();

        let trepo = TaskRepo::new(conn.clone());
        let mut t = sample_task("fn-1", "fn-1.1");
        t.depends_on = vec!["fn-1.0".to_string()];
        t.files = vec!["src/a.rs".to_string()];
        trepo.upsert(&t).await.unwrap();

        trepo
            .update_status("fn-1.1", Status::InProgress)
            .await
            .unwrap();
        let got = trepo.get("fn-1.1").await.unwrap();
        assert!(matches!(got.status, Status::InProgress));

        trepo.delete("fn-1.1").await.unwrap();
        assert!(matches!(
            trepo.get("fn-1.1").await.unwrap_err(),
            DbError::NotFound(_)
        ));

        // Update missing -> NotFound.
        let err = trepo
            .update_status("missing", Status::Done)
            .await
            .unwrap_err();
        assert!(matches!(err, DbError::NotFound(_)));
    }

    #[tokio::test]
    async fn dep_repo_add_list_remove() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let deps = DepRepo::new(conn.clone());

        deps.add_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        deps.add_task_dep("fn-1.2", "fn-1.0").await.unwrap();
        // Idempotent.
        deps.add_task_dep("fn-1.2", "fn-1.1").await.unwrap();

        let mut got = deps.list_task_deps("fn-1.2").await.unwrap();
        got.sort();
        assert_eq!(got, vec!["fn-1.0".to_string(), "fn-1.1".to_string()]);

        deps.remove_task_dep("fn-1.2", "fn-1.1").await.unwrap();
        let after = deps.list_task_deps("fn-1.2").await.unwrap();
        assert_eq!(after, vec!["fn-1.0".to_string()]);

        deps.add_epic_dep("fn-2", "fn-1").await.unwrap();
        deps.add_epic_dep("fn-2", "fn-0").await.unwrap();
        let mut elist = deps.list_epic_deps("fn-2").await.unwrap();
        elist.sort();
        assert_eq!(elist, vec!["fn-0".to_string(), "fn-1".to_string()]);

        deps.remove_epic_dep("fn-2", "fn-0").await.unwrap();
        assert_eq!(
            deps.list_epic_deps("fn-2").await.unwrap(),
            vec!["fn-1".to_string()]
        );
    }

    #[tokio::test]
    async fn file_ownership_repo_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let f = FileOwnershipRepo::new(conn.clone());

        f.add("src/a.rs", "fn-1.1").await.unwrap();
        f.add("src/b.rs", "fn-1.1").await.unwrap();
        f.add("src/a.rs", "fn-1.2").await.unwrap();
        // Idempotent.
        f.add("src/a.rs", "fn-1.1").await.unwrap();

        let mut t1 = f.list_for_task("fn-1.1").await.unwrap();
        t1.sort();
        assert_eq!(t1, vec!["src/a.rs".to_string(), "src/b.rs".to_string()]);

        let mut owners = f.list_for_file("src/a.rs").await.unwrap();
        owners.sort();
        assert_eq!(owners, vec!["fn-1.1".to_string(), "fn-1.2".to_string()]);

        f.remove("src/a.rs", "fn-1.2").await.unwrap();
        let owners2 = f.list_for_file("src/a.rs").await.unwrap();
        assert_eq!(owners2, vec!["fn-1.1".to_string()]);
    }

    // ── RuntimeRepo ─────────────────────────────────────────────────

    #[tokio::test]
    async fn runtime_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = RuntimeRepo::new(conn.clone());
        let now = Utc::now();
        let state = RuntimeState {
            task_id: "fn-1.1".to_string(),
            assignee: Some("worker-1".to_string()),
            claimed_at: Some(now),
            completed_at: None,
            duration_secs: Some(42),
            blocked_reason: None,
            baseline_rev: Some("abc123".to_string()),
            final_rev: None,
            retry_count: 2,
        };
        repo.upsert(&state).await.unwrap();

        let got = repo.get("fn-1.1").await.unwrap().expect("should exist");
        assert_eq!(got.task_id, "fn-1.1");
        assert_eq!(got.assignee.as_deref(), Some("worker-1"));
        assert_eq!(got.duration_secs, Some(42));
        assert_eq!(got.baseline_rev.as_deref(), Some("abc123"));
        assert_eq!(got.retry_count, 2);
        assert!(got.claimed_at.is_some());
        assert!(got.completed_at.is_none());

        // Update (upsert) the same task.
        let updated = RuntimeState {
            retry_count: 3,
            final_rev: Some("def456".to_string()),
            ..state
        };
        repo.upsert(&updated).await.unwrap();
        let got2 = repo.get("fn-1.1").await.unwrap().unwrap();
        assert_eq!(got2.retry_count, 3);
        assert_eq!(got2.final_rev.as_deref(), Some("def456"));
    }

    #[tokio::test]
    async fn runtime_get_missing_returns_none() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = RuntimeRepo::new(conn.clone());
        assert!(repo.get("does-not-exist").await.unwrap().is_none());
    }

    // ── EvidenceRepo ────────────────────────────────────────────────

    #[tokio::test]
    async fn evidence_upsert_get_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        let ev = Evidence {
            commits: vec!["abc123".to_string(), "def456".to_string()],
            tests: vec!["cargo test".to_string(), "bash smoke.sh".to_string()],
            prs: Vec::new(),
            files_changed: Some(5),
            insertions: Some(120),
            deletions: Some(30),
            review_iterations: Some(1),
            workspace_changes: None,
        };
        repo.upsert("fn-1.1", &ev).await.unwrap();

        let got = repo.get("fn-1.1").await.unwrap().expect("should exist");
        assert_eq!(got.commits, vec!["abc123".to_string(), "def456".to_string()]);
        assert_eq!(
            got.tests,
            vec!["cargo test".to_string(), "bash smoke.sh".to_string()]
        );
        assert_eq!(got.files_changed, Some(5));
        assert_eq!(got.insertions, Some(120));
        assert_eq!(got.deletions, Some(30));
        assert_eq!(got.review_iterations, Some(1));
    }

    #[tokio::test]
    async fn evidence_get_missing_returns_none() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        assert!(repo.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn evidence_empty_vecs_roundtrip() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EvidenceRepo::new(conn.clone());
        let ev = Evidence {
            commits: Vec::new(),
            tests: Vec::new(),
            prs: Vec::new(),
            files_changed: None,
            insertions: None,
            deletions: None,
            review_iterations: None,
            workspace_changes: None,
        };
        repo.upsert("fn-2.1", &ev).await.unwrap();
        let got = repo.get("fn-2.1").await.unwrap().unwrap();
        assert!(got.commits.is_empty());
        assert!(got.tests.is_empty());
        assert_eq!(got.files_changed, None);
    }

    // ── FileLockRepo ────────────────────────────────────────────────

    #[tokio::test]
    async fn file_lock_acquire_twice_conflicts() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        repo.acquire("src/a.rs", "fn-1.1").await.unwrap();
        let err = repo.acquire("src/a.rs", "fn-1.2").await.unwrap_err();
        assert!(
            matches!(err, DbError::Constraint(_)),
            "expected Constraint, got {err:?}"
        );
    }

    #[tokio::test]
    async fn file_lock_release_for_task_and_check() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn.clone());

        repo.acquire("src/a.rs", "fn-1.1").await.unwrap();
        repo.acquire("src/b.rs", "fn-1.1").await.unwrap();
        repo.acquire("src/c.rs", "fn-1.2").await.unwrap();

        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.1")
        );
        assert!(repo.check("src/missing.rs").await.unwrap().is_none());

        let n = repo.release_for_task("fn-1.1").await.unwrap();
        assert_eq!(n, 2);
        assert!(repo.check("src/a.rs").await.unwrap().is_none());
        assert!(repo.check("src/b.rs").await.unwrap().is_none());
        // fn-1.2 still holds its lock.
        assert_eq!(
            repo.check("src/c.rs").await.unwrap().as_deref(),
            Some("fn-1.2")
        );

        // Re-acquiring a released file works.
        repo.acquire("src/a.rs", "fn-1.3").await.unwrap();
        assert_eq!(
            repo.check("src/a.rs").await.unwrap().as_deref(),
            Some("fn-1.3")
        );

        // release_all clears remaining locks.
        let n2 = repo.release_all().await.unwrap();
        assert_eq!(n2, 2);
        assert!(repo.check("src/a.rs").await.unwrap().is_none());
        assert!(repo.check("src/c.rs").await.unwrap().is_none());
    }

    // ── PhaseProgressRepo ───────────────────────────────────────────

    #[tokio::test]
    async fn event_repo_insert_list_by_epic_and_type() {
        let (_db, conn) = open_memory_async().await.unwrap();
        // Need an epic row since events.epic_id is TEXT NOT NULL (no FK but we'll be honest).
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-9-evt', 'Evt Test', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();

        let repo = EventRepo::new(conn.clone());
        let id1 = repo.insert("fn-9-evt", Some("fn-9-evt.1"), "task_started", Some("w1"), None, None).await.unwrap();
        let id2 = repo.insert("fn-9-evt", Some("fn-9-evt.1"), "task_completed", Some("w1"), Some("{}"), None).await.unwrap();
        let id3 = repo.insert("fn-9-evt", Some("fn-9-evt.2"), "task_started", Some("w1"), None, None).await.unwrap();
        assert!(id1 > 0 && id2 > id1 && id3 > id2);

        let by_epic = repo.list_by_epic("fn-9-evt", 10).await.unwrap();
        assert_eq!(by_epic.len(), 3);
        // Most recent first.
        assert_eq!(by_epic[0].id, id3);

        let started = repo.list_by_type("task_started", 10).await.unwrap();
        assert_eq!(started.len(), 2);
        let completed = repo.list_by_type("task_completed", 10).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].payload.as_deref(), Some("{}"));
    }

    #[tokio::test]
    async fn phase_progress_mark_done_and_get() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = PhaseProgressRepo::new(conn.clone());

        repo.mark_done("fn-1.1", "plan").await.unwrap();
        repo.mark_done("fn-1.1", "implement").await.unwrap();

        let phases = repo.get_completed("fn-1.1").await.unwrap();
        assert_eq!(phases, vec!["plan".to_string(), "implement".to_string()]);

        // Idempotent re-mark.
        repo.mark_done("fn-1.1", "plan").await.unwrap();
        let phases2 = repo.get_completed("fn-1.1").await.unwrap();
        assert_eq!(phases2.len(), 2);

        let n = repo.reset("fn-1.1").await.unwrap();
        assert_eq!(n, 2);
        assert!(repo.get_completed("fn-1.1").await.unwrap().is_empty());
    }
}
