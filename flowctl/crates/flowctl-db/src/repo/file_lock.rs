//! Async repository for runtime file locks (Teams mode concurrency).
//!
//! Uses PID-based crash detection + TTL fallback for hung processes.
//! Stale locks (dead PID or expired TTL) are auto-cleaned on `acquire()`.

use chrono::{Duration, Utc};
use libsql::{params, Connection};
use nix::sys::signal;
use nix::unistd::Pid;

use crate::error::DbError;

/// Default lock TTL: 45 minutes.
const LOCK_TTL_MINUTES: i64 = 45;

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

    /// Acquire a lock on a file for a task.
    ///
    /// Calls `cleanup_stale()` first, then uses `INSERT OR IGNORE` with
    /// `holder_pid` and `expires_at`. If the row already exists (rows_affected=0),
    /// checks whether it belongs to this task (idempotent Ok) or another
    /// (returns `DbError::Constraint`).
    pub async fn acquire(&self, file_path: &str, task_id: &str) -> Result<(), DbError> {
        self.cleanup_stale().await?;

        let pid = std::process::id();
        let expires_at = (Utc::now() + Duration::minutes(LOCK_TTL_MINUTES)).to_rfc3339();

        let rows_affected = self
            .conn
            .execute(
                "INSERT OR IGNORE INTO file_locks (file_path, task_id, locked_at, holder_pid, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    file_path.to_string(),
                    task_id.to_string(),
                    Utc::now().to_rfc3339(),
                    pid as i64,
                    expires_at,
                ],
            )
            .await?;

        if rows_affected > 0 {
            return Ok(());
        }

        // Row already exists — check if it's our own lock (idempotent).
        let existing_task = self.check(file_path).await?;
        if existing_task.as_deref() == Some(task_id) {
            return Ok(());
        }

        Err(DbError::Constraint(format!(
            "file already locked: {file_path}"
        )))
    }

    /// Remove stale locks: dead PIDs (via `kill(pid, 0)`) and expired TTLs.
    pub async fn cleanup_stale(&self) -> Result<u64, DbError> {
        let mut total_cleaned = 0u64;

        // 1. Delete expired locks (expires_at < now).
        let now = Utc::now().to_rfc3339();
        let expired = self
            .conn
            .execute(
                "DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?1",
                params![now],
            )
            .await?;
        total_cleaned += expired;

        // 2. Check PIDs of remaining locks — delete dead ones.
        let mut rows = self
            .conn
            .query(
                "SELECT file_path, holder_pid FROM file_locks WHERE holder_pid IS NOT NULL",
                (),
            )
            .await?;

        let mut dead_files = Vec::new();
        while let Some(row) = rows.next().await? {
            let fp: String = row.get(0)?;
            let pid: i64 = row.get(1)?;

            if !is_process_alive(pid as i32) {
                dead_files.push(fp);
            }
        }

        for fp in &dead_files {
            let n = self
                .conn
                .execute(
                    "DELETE FROM file_locks WHERE file_path = ?1",
                    params![fp.clone()],
                )
                .await?;
            total_cleaned += n;
        }

        Ok(total_cleaned)
    }

    /// Extend `expires_at` for all locks held by a task (heartbeat).
    pub async fn heartbeat(&self, task_id: &str) -> Result<u64, DbError> {
        let new_expires = (Utc::now() + Duration::minutes(LOCK_TTL_MINUTES)).to_rfc3339();
        let n = self
            .conn
            .execute(
                "UPDATE file_locks SET expires_at = ?1 WHERE task_id = ?2",
                params![new_expires, task_id.to_string()],
            )
            .await?;
        Ok(n)
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

/// Check if a process is alive using `kill(pid, 0)`.
fn is_process_alive(pid: i32) -> bool {
    // kill(pid, 0) returns Ok if process exists, Err(ESRCH) if not.
    signal::kill(Pid::from_raw(pid), None).is_ok()
}
