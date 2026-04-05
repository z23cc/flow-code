//! Async repository abstractions over libSQL.
//!
//! Ported from `flowctl-db::repo` (sync rusqlite). Each repo owns a
//! `libsql::Connection` (cheap Clone) and exposes async methods that
//! return `DbError`. Mirrors the sync API surface where it makes sense.

use chrono::{DateTime, Utc};
use libsql::{params, Connection};

use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, Epic, EpicStatus, ReviewStatus, Task};

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
}
