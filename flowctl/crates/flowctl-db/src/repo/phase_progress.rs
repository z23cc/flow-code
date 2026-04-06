//! Async repository for worker-phase progress tracking.

use chrono::Utc;
use libsql::{params, Connection};

use crate::error::DbError;

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
