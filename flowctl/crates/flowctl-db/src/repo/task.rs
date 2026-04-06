//! Async repository for task CRUD operations.

use chrono::Utc;
use libsql::{params, Connection};

use flowctl_core::state_machine::Status;
use flowctl_core::types::Task;

use crate::error::DbError;

use super::helpers::{parse_datetime, parse_domain, parse_status};

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
