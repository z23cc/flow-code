//! Async repository for epic CRUD operations.

use chrono::Utc;
use libsql::{params, Connection};

use flowctl_core::types::{Epic, EpicStatus};

use crate::error::DbError;

use super::helpers::{parse_datetime, parse_epic_status, parse_review_status};
use flowctl_core::types::ReviewStatus;

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
                "INSERT INTO epics (id, title, status, branch_name, plan_review, auto_execute_pending, auto_execute_set_at, archived, file_path, body, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET
                     title = excluded.title,
                     status = excluded.status,
                     branch_name = excluded.branch_name,
                     plan_review = excluded.plan_review,
                     auto_execute_pending = excluded.auto_execute_pending,
                     auto_execute_set_at = excluded.auto_execute_set_at,
                     archived = excluded.archived,
                     file_path = excluded.file_path,
                     body = CASE WHEN excluded.body = '' THEN epics.body ELSE excluded.body END,
                     updated_at = excluded.updated_at",
                params![
                    epic.id.clone(),
                    epic.title.clone(),
                    epic.status.to_string(),
                    epic.branch_name.clone(),
                    epic.plan_review.to_string(),
                    epic.auto_execute_pending.unwrap_or(false) as i64,
                    epic.auto_execute_set_at.clone(),
                    epic.archived as i64,
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
                "SELECT id, title, status, branch_name, plan_review, file_path, created_at, updated_at, COALESCE(body, ''), auto_execute_pending, auto_execute_set_at, archived
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
        let auto_exec_pending: i64 = row.get::<i64>(9).unwrap_or(0);
        let auto_exec_set_at: Option<String> = row.get::<Option<String>>(10).unwrap_or(None);
        let archived_val: i64 = row.get::<i64>(11).unwrap_or(0);

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
            auto_execute_pending: if auto_exec_pending != 0 { Some(true) } else { None },
            auto_execute_set_at: auto_exec_set_at,
            archived: archived_val != 0,
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
