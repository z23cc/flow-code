//! Async repository for per-task runtime state (Teams mode assignment, timing).

use libsql::{params, Connection};

use flowctl_core::types::RuntimeState;

use crate::error::DbError;

use super::helpers::parse_datetime;

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
