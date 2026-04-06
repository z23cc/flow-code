//! Async repository for runtime file locks (Teams mode concurrency).

use chrono::Utc;
use libsql::{params, Connection};

use crate::error::DbError;

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
