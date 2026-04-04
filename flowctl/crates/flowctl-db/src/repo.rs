//! Repository abstractions for database CRUD operations.
//!
//! Thin wrappers over rusqlite that map between flowctl-core types and
//! SQLite rows. Each repository struct borrows a `&Connection` and
//! provides typed query methods.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use flowctl_core::types::{Domain, Epic, EpicStatus, Evidence, ReviewStatus, RuntimeState, Task};
use flowctl_core::state_machine::Status;

use crate::error::DbError;

// ── Epic repository ─────────────────────────────────────────────────

/// Repository for epic CRUD operations.
pub struct EpicRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EpicRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace an epic (used by reindex and create).
    pub fn upsert(&self, epic: &Epic) -> Result<(), DbError> {
        self.upsert_with_body(epic, "")
    }

    /// Insert or replace an epic with its markdown body.
    pub fn upsert_with_body(&self, epic: &Epic, body: &str) -> Result<(), DbError> {
        self.conn.execute(
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
                epic.id,
                epic.title,
                epic.status.to_string(),
                epic.branch_name,
                epic.plan_review.to_string(),
                epic.file_path.as_deref().unwrap_or(""),
                body,
                epic.created_at.to_rfc3339(),
                epic.updated_at.to_rfc3339(),
            ],
        )?;

        // Upsert epic dependencies.
        self.conn.execute(
            "DELETE FROM epic_deps WHERE epic_id = ?1",
            params![epic.id],
        )?;
        for dep in &epic.depends_on_epics {
            self.conn.execute(
                "INSERT INTO epic_deps (epic_id, depends_on) VALUES (?1, ?2)",
                params![epic.id, dep],
            )?;
        }

        Ok(())
    }

    /// Get an epic by ID.
    pub fn get(&self, id: &str) -> Result<Epic, DbError> {
        self.get_with_body(id).map(|(epic, _body)| epic)
    }

    /// Get an epic by ID, returning (Epic, body).
    pub fn get_with_body(&self, id: &str) -> Result<(Epic, String), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, status, branch_name, plan_review, file_path, created_at, updated_at, COALESCE(body, '')
             FROM epics WHERE id = ?1",
        )?;

        let (epic, body) = stmt
            .query_row(params![id], |row| {
                Ok((Epic {
                    schema_version: 1,
                    id: row.get(0)?,
                    title: row.get(1)?,
                    status: parse_epic_status(&row.get::<_, String>(2)?),
                    branch_name: row.get(3)?,
                    plan_review: parse_review_status(&row.get::<_, String>(4)?),
                    completion_review: ReviewStatus::Unknown,
                    depends_on_epics: Vec::new(), // loaded below
                    default_impl: None,
                    default_review: None,
                    default_sync: None,
                    file_path: row.get::<_, Option<String>>(5)?,
                    created_at: parse_datetime(&row.get::<_, String>(6)?),
                    updated_at: parse_datetime(&row.get::<_, String>(7)?),
                }, row.get::<_, String>(8)?))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound {
                    entity: "epic",
                    id: id.to_string(),
                },
                other => DbError::Sqlite(other),
            })?;

        // Load dependencies.
        let deps = self.get_deps(&epic.id)?;
        Ok((Epic {
            depends_on_epics: deps,
            ..epic
        }, body))
    }

    /// List all epics, optionally filtered by status.
    pub fn list(&self, status: Option<&str>) -> Result<Vec<Epic>, DbError> {
        let sql = match status {
            Some(_) => "SELECT id FROM epics WHERE status = ?1 ORDER BY created_at",
            None => "SELECT id FROM epics ORDER BY created_at",
        };

        let mut stmt = self.conn.prepare(sql)?;
        let ids: Vec<String> = match status {
            Some(s) => stmt
                .query_map(params![s], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?,
            None => stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?,
        };

        ids.iter().map(|id| self.get(id)).collect()
    }

    /// Update epic status.
    pub fn update_status(&self, id: &str, status: EpicStatus) -> Result<(), DbError> {
        let rows = self.conn.execute(
            "UPDATE epics SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.to_string(), Utc::now().to_rfc3339(), id],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound {
                entity: "epic",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    /// Delete an epic and all related data (for reindex).
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM epic_deps WHERE epic_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM epics WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn get_deps(&self, epic_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT depends_on FROM epic_deps WHERE epic_id = ?1")?;
        let deps = stmt
            .query_map(params![epic_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(deps)
    }
}

// ── Task repository ─────────────────────────────────────────────────

/// Repository for task CRUD operations.
pub struct TaskRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TaskRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace a task (used by reindex and create).
    pub fn upsert(&self, task: &Task) -> Result<(), DbError> {
        self.upsert_with_body(task, "")
    }

    /// Insert or replace a task with its markdown body.
    pub fn upsert_with_body(&self, task: &Task, body: &str) -> Result<(), DbError> {
        self.conn.execute(
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
                task.id,
                task.epic,
                task.title,
                task.status.to_string(),
                task.sort_priority() as i64,
                task.domain.to_string(),
                task.file_path.as_deref().unwrap_or(""),
                body,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ],
        )?;

        // Upsert dependencies.
        self.conn.execute(
            "DELETE FROM task_deps WHERE task_id = ?1",
            params![task.id],
        )?;
        for dep in &task.depends_on {
            self.conn.execute(
                "INSERT INTO task_deps (task_id, depends_on) VALUES (?1, ?2)",
                params![task.id, dep],
            )?;
        }

        // Upsert file ownership.
        self.conn.execute(
            "DELETE FROM file_ownership WHERE task_id = ?1",
            params![task.id],
        )?;
        for file in &task.files {
            self.conn.execute(
                "INSERT INTO file_ownership (file_path, task_id) VALUES (?1, ?2)",
                params![file, task.id],
            )?;
        }

        Ok(())
    }

    /// Get a task by ID.
    pub fn get(&self, id: &str) -> Result<Task, DbError> {
        self.get_with_body(id).map(|(task, _body)| task)
    }

    /// Get a task by ID, returning (Task, body).
    pub fn get_with_body(&self, id: &str) -> Result<(Task, String), DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, epic_id, title, status, priority, domain, file_path, created_at, updated_at, COALESCE(body, '')
             FROM tasks WHERE id = ?1",
        )?;

        let (task, body) = stmt
            .query_row(params![id], |row| {
                let priority_val: i64 = row.get(4)?;
                let priority = if priority_val == 999 {
                    None
                } else {
                    Some(priority_val as u32)
                };

                Ok((Task {
                    schema_version: 1,
                    id: row.get(0)?,
                    epic: row.get(1)?,
                    title: row.get(2)?,
                    status: parse_status(&row.get::<_, String>(3)?),
                    priority,
                    domain: parse_domain(&row.get::<_, String>(5)?),
                    depends_on: Vec::new(), // loaded below
                    files: Vec::new(),      // loaded below
                    r#impl: None,
                    review: None,
                    sync: None,
                    file_path: row.get::<_, Option<String>>(6)?,
                    created_at: parse_datetime(&row.get::<_, String>(7)?),
                    updated_at: parse_datetime(&row.get::<_, String>(8)?),
                }, row.get::<_, String>(9)?))
            })
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound {
                    entity: "task",
                    id: id.to_string(),
                },
                other => DbError::Sqlite(other),
            })?;

        let deps = self.get_deps(&task.id)?;
        let files = self.get_files(&task.id)?;
        Ok((Task {
            depends_on: deps,
            files,
            ..task
        }, body))
    }

    /// List tasks for an epic.
    pub fn list_by_epic(&self, epic_id: &str) -> Result<Vec<Task>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM tasks WHERE epic_id = ?1 ORDER BY priority, id")?;
        let ids: Vec<String> = stmt
            .query_map(params![epic_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        ids.iter().map(|id| self.get(id)).collect()
    }

    /// List all tasks, optionally filtered by status and/or domain.
    pub fn list_all(
        &self,
        status: Option<&str>,
        domain: Option<&str>,
    ) -> Result<Vec<Task>, DbError> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        if let Some(s) = status {
            conditions.push(format!("status = ?{}", param_values.len() + 1));
            param_values.push(s.to_string());
        }
        if let Some(d) = domain {
            conditions.push(format!("domain = ?{}", param_values.len() + 1));
            param_values.push(d.to_string());
        }

        let sql = if conditions.is_empty() {
            "SELECT id FROM tasks ORDER BY epic_id, priority, id".to_string()
        } else {
            format!(
                "SELECT id FROM tasks WHERE {} ORDER BY epic_id, priority, id",
                conditions.join(" AND ")
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let ids: Vec<String> = match param_values.len() {
            0 => stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?,
            1 => stmt
                .query_map(params![param_values[0]], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?,
            2 => stmt
                .query_map(params![param_values[0], param_values[1]], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?,
            _ => unreachable!(),
        };

        ids.iter().map(|id| self.get(id)).collect()
    }

    /// List tasks filtered by status.
    pub fn list_by_status(&self, status: Status) -> Result<Vec<Task>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM tasks WHERE status = ?1 ORDER BY priority, id")?;
        let ids: Vec<String> = stmt
            .query_map(params![status.to_string()], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        ids.iter().map(|id| self.get(id)).collect()
    }

    /// Update task status.
    pub fn update_status(&self, id: &str, status: Status) -> Result<(), DbError> {
        let rows = self.conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.to_string(), Utc::now().to_rfc3339(), id],
        )?;
        if rows == 0 {
            return Err(DbError::NotFound {
                entity: "task",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    /// Delete a task and all related data (for reindex).
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM task_deps WHERE task_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM file_ownership WHERE task_id = ?1", params![id])?;
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn get_deps(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT depends_on FROM task_deps WHERE task_id = ?1")?;
        let deps = stmt
            .query_map(params![task_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(deps)
    }

    fn get_files(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_path FROM file_ownership WHERE task_id = ?1")?;
        let files = stmt
            .query_map(params![task_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }
}

// ── Runtime state repository ────────────────────────────────────────

/// Repository for runtime state (not in Markdown, SQLite-only).
pub struct RuntimeRepo<'a> {
    conn: &'a Connection,
}

impl<'a> RuntimeRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Upsert runtime state for a task.
    pub fn upsert(&self, state: &RuntimeState) -> Result<(), DbError> {
        self.conn.execute(
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
                state.task_id,
                state.assignee,
                state.claimed_at.map(|dt| dt.to_rfc3339()),
                state.completed_at.map(|dt| dt.to_rfc3339()),
                state.duration_secs.map(|d| d as i64),
                state.blocked_reason,
                state.baseline_rev,
                state.final_rev,
                state.retry_count,
            ],
        )?;
        Ok(())
    }

    /// Get runtime state for a task.
    pub fn get(&self, task_id: &str) -> Result<Option<RuntimeState>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, assignee, claimed_at, completed_at, duration_secs, blocked_reason, baseline_rev, final_rev, retry_count
             FROM runtime_state WHERE task_id = ?1",
        )?;

        let result = stmt.query_row(params![task_id], |row| {
            Ok(RuntimeState {
                task_id: row.get(0)?,
                assignee: row.get(1)?,
                claimed_at: row
                    .get::<_, Option<String>>(2)?
                    .map(|s| parse_datetime(&s)),
                completed_at: row
                    .get::<_, Option<String>>(3)?
                    .map(|s| parse_datetime(&s)),
                duration_secs: row.get::<_, Option<i64>>(4)?.map(|d| d as u64),
                blocked_reason: row.get(5)?,
                baseline_rev: row.get(6)?,
                final_rev: row.get(7)?,
                retry_count: row.get::<_, i32>(8).unwrap_or(0) as u32,
            })
        });

        match result {
            Ok(state) => Ok(Some(state)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }
}

// ── Evidence repository ─────────────────────────────────────────────

/// Repository for task completion evidence.
pub struct EvidenceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EvidenceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Upsert evidence for a task. Commits and tests are stored as JSON arrays.
    pub fn upsert(&self, task_id: &str, evidence: &Evidence) -> Result<(), DbError> {
        let commits_json =
            if evidence.commits.is_empty() { None } else { Some(serde_json::to_string(&evidence.commits)?) };
        let tests_json =
            if evidence.tests.is_empty() { None } else { Some(serde_json::to_string(&evidence.tests)?) };

        self.conn.execute(
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
                task_id,
                commits_json,
                tests_json,
                evidence.files_changed.map(|v| v as i64),
                evidence.insertions.map(|v| v as i64),
                evidence.deletions.map(|v| v as i64),
                evidence.review_iterations.map(|v| v as i64),
            ],
        )?;
        Ok(())
    }

    /// Get evidence for a task.
    pub fn get(&self, task_id: &str) -> Result<Option<Evidence>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT commits, tests, files_changed, insertions, deletions, review_iters
             FROM evidence WHERE task_id = ?1",
        )?;

        let result = stmt.query_row(params![task_id], |row| {
            let commits_json: Option<String> = row.get(0)?;
            let tests_json: Option<String> = row.get(1)?;

            Ok((commits_json, tests_json, row.get::<_, Option<i64>>(2)?, row.get::<_, Option<i64>>(3)?, row.get::<_, Option<i64>>(4)?, row.get::<_, Option<i64>>(5)?))
        });

        match result {
            Ok((commits_json, tests_json, files_changed, insertions, deletions, review_iters)) => {
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
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }
}

// ── File lock repository ────────────────────────────────────────────

/// Repository for runtime file locks (Teams mode).
pub struct FileLockRepo<'a> {
    conn: &'a Connection,
}

impl<'a> FileLockRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Acquire a lock on a file for a task. Returns error if already locked.
    pub fn acquire(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "INSERT INTO file_locks (file_path, task_id, locked_at) VALUES (?1, ?2, ?3)",
                params![file_path, task_id, Utc::now().to_rfc3339()],
            )
            .map_err(|e| match e {
                rusqlite::Error::SqliteFailure(err, _)
                    if err.code == rusqlite::ffi::ErrorCode::ConstraintViolation =>
                {
                    DbError::Constraint(format!("file already locked: {file_path}"))
                }
                other => DbError::Sqlite(other),
            })?;
        Ok(())
    }

    /// Release locks held by a task.
    pub fn release_for_task(&self, task_id: &str) -> Result<usize, DbError> {
        let count = self
            .conn
            .execute(
                "DELETE FROM file_locks WHERE task_id = ?1",
                params![task_id],
            )?;
        Ok(count)
    }

    /// Release all locks (between waves).
    pub fn release_all(&self) -> Result<usize, DbError> {
        let count = self.conn.execute("DELETE FROM file_locks", [])?;
        Ok(count)
    }

    /// Check if a file is locked. Returns the locking task_id if so.
    pub fn check(&self, file_path: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT task_id FROM file_locks WHERE file_path = ?1")?;

        match stmt.query_row(params![file_path], |row| row.get(0)) {
            Ok(task_id) => Ok(Some(task_id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }
}

// ── Event repository ────────────────────────────────────────────────

/// Repository for the append-only event log.
pub struct EventRepo<'a> {
    conn: &'a Connection,
}

impl<'a> EventRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Record an event.
    pub fn insert(
        &self,
        epic_id: &str,
        task_id: Option<&str>,
        event_type: &str,
        actor: Option<&str>,
        payload: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO events (epic_id, task_id, event_type, actor, payload, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![epic_id, task_id, event_type, actor, payload, session_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Query recent events for an epic.
    pub fn list_by_epic(&self, epic_id: &str, limit: usize) -> Result<Vec<EventRow>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
             FROM events WHERE epic_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![epic_id, limit as i64], |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    epic_id: row.get(2)?,
                    task_id: row.get(3)?,
                    event_type: row.get(4)?,
                    actor: row.get(5)?,
                    payload: row.get(6)?,
                    session_id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

/// A row from the events table.
#[derive(Debug, Clone)]
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

// ── Phase progress repository ──────────────────────────────────────

/// Repository for worker-phase progress tracking.
pub struct PhaseProgressRepo<'a> {
    conn: &'a Connection,
}

impl<'a> PhaseProgressRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Get all completed phases for a task.
    pub fn get_completed(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT phase FROM phase_progress WHERE task_id = ?1 AND status = 'done' ORDER BY rowid",
        )?;
        let phases = stmt
            .query_map(params![task_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(phases)
    }

    /// Mark a phase as done.
    pub fn mark_done(&self, task_id: &str, phase: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO phase_progress (task_id, phase, status, completed_at)
             VALUES (?1, ?2, 'done', ?3)
             ON CONFLICT(task_id, phase) DO UPDATE SET
                 status = 'done',
                 completed_at = excluded.completed_at",
            params![task_id, phase, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Reset all phase progress for a task.
    pub fn reset(&self, task_id: &str) -> Result<usize, DbError> {
        let count = self
            .conn
            .execute("DELETE FROM phase_progress WHERE task_id = ?1", params![task_id])?;
        Ok(count)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;

    fn test_conn() -> Connection {
        open_memory().expect("in-memory db")
    }

    // ── Epic tests ──────────────────────────────────────────────────

    #[test]
    fn test_epic_upsert_and_get() {
        let conn = test_conn();
        let repo = EpicRepo::new(&conn);

        let epic = Epic {
            schema_version: 1,
            id: "fn-1-test".to_string(),
            title: "Test Epic".to_string(),
            status: EpicStatus::Open,
            branch_name: Some("feat/test".to_string()),
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some("epics/fn-1-test.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        repo.upsert(&epic).unwrap();
        let loaded = repo.get("fn-1-test").unwrap();
        assert_eq!(loaded.id, "fn-1-test");
        assert_eq!(loaded.title, "Test Epic");
        assert_eq!(loaded.status, EpicStatus::Open);
        assert_eq!(loaded.branch_name, Some("feat/test".to_string()));
    }

    #[test]
    fn test_epic_with_deps() {
        let conn = test_conn();
        let repo = EpicRepo::new(&conn);

        // Create dependency epic first.
        let dep_epic = Epic {
            schema_version: 1,
            id: "fn-1-dep".to_string(),
            title: "Dependency".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some("epics/fn-1-dep.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        repo.upsert(&dep_epic).unwrap();

        let epic = Epic {
            depends_on_epics: vec!["fn-1-dep".to_string()],
            id: "fn-2-test".to_string(),
            title: "With Deps".to_string(),
            file_path: Some("epics/fn-2-test.md".to_string()),
            ..dep_epic.clone()
        };
        repo.upsert(&epic).unwrap();

        let loaded = repo.get("fn-2-test").unwrap();
        assert_eq!(loaded.depends_on_epics, vec!["fn-1-dep"]);
    }

    #[test]
    fn test_epic_not_found() {
        let conn = test_conn();
        let repo = EpicRepo::new(&conn);
        let result = repo.get("nonexistent");
        assert!(matches!(result, Err(DbError::NotFound { .. })));
    }

    #[test]
    fn test_epic_list() {
        let conn = test_conn();
        let repo = EpicRepo::new(&conn);

        for i in 1..=3 {
            let epic = Epic {
                schema_version: 1,
                id: format!("fn-{i}-test"),
                title: format!("Epic {i}"),
                status: if i == 3 { EpicStatus::Done } else { EpicStatus::Open },
                branch_name: None,
                plan_review: ReviewStatus::Unknown,
                completion_review: ReviewStatus::Unknown,
                depends_on_epics: vec![],
                default_impl: None,
                default_review: None,
                default_sync: None,
                file_path: Some(format!("epics/fn-{i}-test.md")),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            repo.upsert(&epic).unwrap();
        }

        assert_eq!(repo.list(None).unwrap().len(), 3);
        assert_eq!(repo.list(Some("open")).unwrap().len(), 2);
        assert_eq!(repo.list(Some("done")).unwrap().len(), 1);
    }

    // ── Task tests ──────────────────────────────────────────────────

    #[test]
    fn test_task_upsert_and_get() {
        let conn = test_conn();
        let epic_repo = EpicRepo::new(&conn);
        let task_repo = TaskRepo::new(&conn);

        // Create epic first (FK).
        let epic = Epic {
            schema_version: 1,
            id: "fn-1-test".to_string(),
            title: "Test".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some("epics/fn-1-test.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        epic_repo.upsert(&epic).unwrap();

        let task = Task {
            schema_version: 1,
            id: "fn-1-test.1".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Task 1".to_string(),
            status: Status::Todo,
            priority: Some(1),
            domain: Domain::Backend,
            depends_on: vec![],
            files: vec!["src/main.rs".to_string()],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some("tasks/fn-1-test.1.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        task_repo.upsert(&task).unwrap();
        let loaded = task_repo.get("fn-1-test.1").unwrap();
        assert_eq!(loaded.id, "fn-1-test.1");
        assert_eq!(loaded.title, "Task 1");
        assert_eq!(loaded.status, Status::Todo);
        assert_eq!(loaded.priority, Some(1));
        assert_eq!(loaded.domain, Domain::Backend);
        assert_eq!(loaded.files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_task_with_deps() {
        let conn = test_conn();
        let epic_repo = EpicRepo::new(&conn);
        let task_repo = TaskRepo::new(&conn);

        let epic = Epic {
            schema_version: 1,
            id: "fn-1-test".to_string(),
            title: "Test".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some("epics/fn-1-test.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        epic_repo.upsert(&epic).unwrap();

        // Task 1 (no deps).
        let t1 = Task {
            schema_version: 1,
            id: "fn-1-test.1".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Task 1".to_string(),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some("tasks/fn-1-test.1.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        task_repo.upsert(&t1).unwrap();

        // Task 2 depends on Task 1.
        let t2 = Task {
            id: "fn-1-test.2".to_string(),
            title: "Task 2".to_string(),
            depends_on: vec!["fn-1-test.1".to_string()],
            file_path: Some("tasks/fn-1-test.2.md".to_string()),
            ..t1.clone()
        };
        task_repo.upsert(&t2).unwrap();

        let loaded = task_repo.get("fn-1-test.2").unwrap();
        assert_eq!(loaded.depends_on, vec!["fn-1-test.1"]);
    }

    #[test]
    fn test_task_status_update() {
        let conn = test_conn();
        let epic_repo = EpicRepo::new(&conn);
        let task_repo = TaskRepo::new(&conn);

        let epic = Epic {
            schema_version: 1,
            id: "fn-1-test".to_string(),
            title: "Test".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            file_path: Some("epics/fn-1-test.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        epic_repo.upsert(&epic).unwrap();

        let task = Task {
            schema_version: 1,
            id: "fn-1-test.1".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Task 1".to_string(),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: Some("tasks/fn-1-test.1.md".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        task_repo.upsert(&task).unwrap();

        task_repo
            .update_status("fn-1-test.1", Status::InProgress)
            .unwrap();
        let loaded = task_repo.get("fn-1-test.1").unwrap();
        assert_eq!(loaded.status, Status::InProgress);
    }

    // ── File lock tests ─────────────────────────────────────────────

    #[test]
    fn test_file_lock_acquire_release() {
        let conn = test_conn();
        let repo = FileLockRepo::new(&conn);

        repo.acquire("src/main.rs", "fn-1.1").unwrap();

        let locker = repo.check("src/main.rs").unwrap();
        assert_eq!(locker, Some("fn-1.1".to_string()));

        repo.release_for_task("fn-1.1").unwrap();
        let locker = repo.check("src/main.rs").unwrap();
        assert_eq!(locker, None);
    }

    #[test]
    fn test_file_lock_conflict() {
        let conn = test_conn();
        let repo = FileLockRepo::new(&conn);

        repo.acquire("src/main.rs", "fn-1.1").unwrap();
        let result = repo.acquire("src/main.rs", "fn-1.2");
        assert!(matches!(result, Err(DbError::Constraint(_))));
    }

    #[test]
    fn test_file_lock_release_all() {
        let conn = test_conn();
        let repo = FileLockRepo::new(&conn);

        repo.acquire("src/a.rs", "fn-1.1").unwrap();
        repo.acquire("src/b.rs", "fn-1.2").unwrap();

        let released = repo.release_all().unwrap();
        assert_eq!(released, 2);

        assert_eq!(repo.check("src/a.rs").unwrap(), None);
        assert_eq!(repo.check("src/b.rs").unwrap(), None);
    }

    // ── Event tests ─────────────────────────────────────────────────

    #[test]
    fn test_event_insert_and_list() {
        let conn = test_conn();

        // Need an epic for the trigger's FK.
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();

        let repo = EventRepo::new(&conn);

        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_started", Some("worker"), None, None)
            .unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", Some("worker"), None, None)
            .unwrap();

        let events = repo.list_by_epic("fn-1-test", 10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "task_completed"); // DESC order
        assert_eq!(events[1].event_type, "task_started");
    }

    // ── Runtime state tests ─────────────────────────────────────────

    #[test]
    fn test_runtime_state_upsert_and_get() {
        let conn = test_conn();
        let repo = RuntimeRepo::new(&conn);

        let state = RuntimeState {
            task_id: "fn-1-test.1".to_string(),
            assignee: Some("worker-1".to_string()),
            claimed_at: Some(Utc::now()),
            completed_at: None,
            duration_secs: None,
            blocked_reason: None,
            baseline_rev: Some("abc123".to_string()),
            final_rev: None,
            retry_count: 0,
        };

        repo.upsert(&state).unwrap();
        let loaded = repo.get("fn-1-test.1").unwrap().unwrap();
        assert_eq!(loaded.assignee, Some("worker-1".to_string()));
        assert_eq!(loaded.baseline_rev, Some("abc123".to_string()));
    }

    #[test]
    fn test_runtime_state_not_found() {
        let conn = test_conn();
        let repo = RuntimeRepo::new(&conn);
        let result = repo.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    // ── Evidence tests ──────────────────────────────────────────────

    #[test]
    fn test_evidence_upsert_and_get() {
        let conn = test_conn();
        let repo = EvidenceRepo::new(&conn);

        let evidence = Evidence {
            commits: vec!["abc123".to_string(), "def456".to_string()],
            tests: vec!["cargo test".to_string()],
            prs: vec![],
            files_changed: Some(5),
            insertions: Some(100),
            deletions: Some(20),
            review_iterations: Some(2),
            workspace_changes: None,
        };

        repo.upsert("fn-1-test.1", &evidence).unwrap();
        let loaded = repo.get("fn-1-test.1").unwrap().unwrap();
        assert_eq!(loaded.commits, vec!["abc123", "def456"]);
        assert_eq!(loaded.tests, vec!["cargo test"]);
        assert_eq!(loaded.files_changed, Some(5));
        assert_eq!(loaded.insertions, Some(100));
        assert_eq!(loaded.deletions, Some(20));
        assert_eq!(loaded.review_iterations, Some(2));
    }
}
