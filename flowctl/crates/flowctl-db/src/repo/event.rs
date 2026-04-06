//! Async repository for the append-only event log.

use libsql::{params, Connection};

use crate::error::DbError;

/// A row from the events table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EventRow {
    pub id: i64,
    pub timestamp: String,
    pub epic_id: String,
    pub task_id: Option<String>,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload: Option<String>,
    pub session_id: Option<String>,
}

/// Async repository for the append-only event log.
pub struct EventRepo {
    conn: Connection,
}

impl EventRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Record an event. Returns the inserted rowid.
    pub async fn insert(
        &self,
        epic_id: &str,
        task_id: Option<&str>,
        event_type: &str,
        actor: Option<&str>,
        payload: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<i64, DbError> {
        self.conn
            .execute(
                "INSERT INTO events (epic_id, task_id, event_type, actor, payload, session_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    epic_id.to_string(),
                    task_id.map(std::string::ToString::to_string),
                    event_type.to_string(),
                    actor.map(std::string::ToString::to_string),
                    payload.map(std::string::ToString::to_string),
                    session_id.map(std::string::ToString::to_string),
                ],
            )
            .await?;
        Ok(self.conn.last_insert_rowid())
    }

    /// List recent events for an epic (most recent first).
    pub async fn list_by_epic(
        &self,
        epic_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
                 FROM events WHERE epic_id = ?1 ORDER BY id DESC LIMIT ?2",
                params![epic_id.to_string(), limit as i64],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(EventRow {
                id: row.get::<i64>(0)?,
                timestamp: row.get::<String>(1)?,
                epic_id: row.get::<String>(2)?,
                task_id: row.get::<Option<String>>(3)?,
                event_type: row.get::<String>(4)?,
                actor: row.get::<Option<String>>(5)?,
                payload: row.get::<Option<String>>(6)?,
                session_id: row.get::<Option<String>>(7)?,
            });
        }
        Ok(out)
    }

    /// List recent events of a given type across all epics.
    pub async fn list_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
                 FROM events WHERE event_type = ?1 ORDER BY id DESC LIMIT ?2",
                params![event_type.to_string(), limit as i64],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(EventRow {
                id: row.get::<i64>(0)?,
                timestamp: row.get::<String>(1)?,
                epic_id: row.get::<String>(2)?,
                task_id: row.get::<Option<String>>(3)?,
                event_type: row.get::<String>(4)?,
                actor: row.get::<Option<String>>(5)?,
                payload: row.get::<Option<String>>(6)?,
                session_id: row.get::<Option<String>>(7)?,
            });
        }
        Ok(out)
    }
}
