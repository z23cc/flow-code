//! Async repository for task and epic dependency edges.

use libsql::{params, Connection};

use crate::error::DbError;

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
