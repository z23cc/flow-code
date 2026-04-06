//! Async repository for the gaps registry.

use libsql::{params, Connection};

use crate::error::DbError;

/// A row from the gaps table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GapRow {
    pub id: i64,
    pub epic_id: String,
    pub capability: String,
    pub priority: String,
    pub source: Option<String>,
    pub status: String,
    pub resolved_at: Option<String>,
    pub evidence: Option<String>,
    pub task_id: Option<String>,
    pub created_at: String,
}

/// Async repository for the gaps registry.
pub struct GapRepo {
    conn: Connection,
}

impl GapRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Add a new gap.
    pub async fn add(
        &self,
        epic_id: &str,
        capability: &str,
        priority: &str,
        source: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<i64, DbError> {
        self.conn
            .execute(
                "INSERT INTO gaps (epic_id, capability, priority, source, task_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    epic_id.to_string(),
                    capability.to_string(),
                    priority.to_string(),
                    source.map(std::string::ToString::to_string),
                    task_id.map(std::string::ToString::to_string),
                ],
            )
            .await?;

        // Return the last inserted rowid.
        let mut rows = self
            .conn
            .query("SELECT last_insert_rowid()", ())
            .await?;
        let row = rows.next().await?.ok_or_else(|| {
            DbError::NotFound("last_insert_rowid".to_string())
        })?;
        Ok(row.get::<i64>(0)?)
    }

    /// List gaps for an epic, optionally filtered by status.
    pub async fn list(
        &self,
        epic_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<GapRow>, DbError> {
        let mut rows = match status {
            Some(s) => {
                self.conn
                    .query(
                        "SELECT id, epic_id, capability, priority, source, status, resolved_at, evidence, task_id, created_at
                         FROM gaps WHERE epic_id = ?1 AND status = ?2 ORDER BY id",
                        params![epic_id.to_string(), s.to_string()],
                    )
                    .await?
            }
            None => {
                self.conn
                    .query(
                        "SELECT id, epic_id, capability, priority, source, status, resolved_at, evidence, task_id, created_at
                         FROM gaps WHERE epic_id = ?1 ORDER BY id",
                        params![epic_id.to_string()],
                    )
                    .await?
            }
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(GapRow {
                id: row.get::<i64>(0)?,
                epic_id: row.get::<String>(1)?,
                capability: row.get::<String>(2)?,
                priority: row.get::<String>(3)?,
                source: row.get::<Option<String>>(4)?,
                status: row.get::<String>(5)?,
                resolved_at: row.get::<Option<String>>(6)?,
                evidence: row.get::<Option<String>>(7)?,
                task_id: row.get::<Option<String>>(8)?,
                created_at: row.get::<String>(9)?,
            });
        }
        Ok(out)
    }

    /// Remove a gap by ID.
    pub async fn remove(&self, id: i64) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM gaps WHERE id = ?1", params![id])
            .await?;
        Ok(())
    }

    /// Remove all gaps for an epic.
    pub async fn remove_all(&self, epic_id: &str) -> Result<u64, DbError> {
        let n = self
            .conn
            .execute(
                "DELETE FROM gaps WHERE epic_id = ?1",
                params![epic_id.to_string()],
            )
            .await?;
        Ok(n)
    }

    /// Resolve a gap by ID.
    pub async fn resolve(&self, id: i64, evidence: &str) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE gaps SET status = 'resolved', resolved_at = datetime('now'), evidence = ?1 WHERE id = ?2",
                params![evidence.to_string(), id],
            )
            .await?;
        Ok(())
    }

    /// Resolve a gap by capability name within an epic.
    pub async fn resolve_by_capability(
        &self,
        epic_id: &str,
        capability: &str,
        evidence: &str,
    ) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE gaps SET status = 'resolved', resolved_at = datetime('now'), evidence = ?1
                 WHERE epic_id = ?2 AND capability = ?3 AND status = 'open'",
                params![evidence.to_string(), epic_id.to_string(), capability.to_string()],
            )
            .await?;
        Ok(())
    }
}
