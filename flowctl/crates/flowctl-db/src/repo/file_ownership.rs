//! Async repository for file ownership edges.

use libsql::{params, Connection};

use crate::error::DbError;

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
