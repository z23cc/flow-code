//! Async repository for the event-sourced event store.
//!
//! Distinct from [`EventRepo`](super::EventRepo) (the audit log). This repo
//! implements append-only, version-ordered streams with optimistic concurrency
//! via a unique `(stream_id, version)` constraint.

use libsql::{params, Connection};

use crate::error::DbError;
use flowctl_core::events::{EventMetadata, FlowEvent};

/// A persisted event read back from the event store.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredEvent {
    pub event_id: i64,
    pub stream_id: String,
    pub version: i64,
    pub event_type: String,
    pub payload: FlowEvent,
    pub metadata: Option<EventMetadata>,
    pub created_at: String,
}

/// Async repository for event-sourced streams.
pub struct EventStoreRepo {
    conn: Connection,
}

impl EventStoreRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Append an event to a stream. Auto-increments the version via
    /// `SELECT MAX(version)+1`. Returns the assigned version number.
    ///
    /// Uses `INSERT OR FAIL` so a concurrent append that races on the same
    /// version will fail with a constraint error rather than silently
    /// overwriting.
    pub async fn append(
        &self,
        stream_id: &str,
        event: &FlowEvent,
        metadata: &EventMetadata,
    ) -> Result<u64, DbError> {
        // Determine the next version for this stream.
        let mut rows = self
            .conn
            .query(
                "SELECT COALESCE(MAX(version), 0) FROM event_store WHERE stream_id = ?1",
                params![stream_id.to_string()],
            )
            .await?;
        let next_version: i64 = match rows.next().await? {
            Some(row) => row.get::<i64>(0)? + 1,
            None => 1,
        };

        let event_type = event_type_label(event);
        let payload_json = serde_json::to_string(event)?;
        let metadata_json = serde_json::to_string(metadata)?;

        let result = self
            .conn
            .execute(
                "INSERT OR FAIL INTO event_store (stream_id, version, event_type, payload, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    stream_id.to_string(),
                    next_version,
                    event_type,
                    payload_json,
                    metadata_json
                ],
            )
            .await;

        match result {
            Ok(_) => Ok(next_version as u64),
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed") || msg.contains("constraint") {
                    Err(DbError::Constraint(format!(
                        "concurrency conflict: version {next_version} already exists for stream {stream_id}"
                    )))
                } else {
                    Err(DbError::LibSql(e))
                }
            }
        }
    }

    /// Query all events for a stream, in version order.
    pub async fn query_stream(&self, stream_id: &str) -> Result<Vec<StoredEvent>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT event_id, stream_id, version, event_type, payload, metadata, created_at
                 FROM event_store WHERE stream_id = ?1 ORDER BY version ASC",
                params![stream_id.to_string()],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(parse_stored_event(&row)?);
        }
        Ok(out)
    }

    /// Query events globally by event type, in creation order.
    pub async fn query_by_type(&self, event_type: &str) -> Result<Vec<StoredEvent>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT event_id, stream_id, version, event_type, payload, metadata, created_at
                 FROM event_store WHERE event_type = ?1 ORDER BY event_id ASC",
                params![event_type.to_string()],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(parse_stored_event(&row)?);
        }
        Ok(out)
    }

    /// Replay all events for a stream (same as `query_stream`, named for intent).
    pub async fn rebuild_stream(&self, stream_id: &str) -> Result<Vec<StoredEvent>, DbError> {
        self.query_stream(stream_id).await
    }

    /// Query all events whose stream_id matches any of the given prefixes.
    /// Useful for fetching all events related to an epic (epic stream + task streams).
    pub async fn query_by_stream_prefixes(&self, prefixes: &[String]) -> Result<Vec<StoredEvent>, DbError> {
        if prefixes.is_empty() {
            return Ok(Vec::new());
        }
        // Build WHERE clause: stream_id LIKE 'prefix1%' OR stream_id LIKE 'prefix2%' ...
        let conditions: Vec<String> = prefixes.iter().enumerate()
            .map(|(i, _)| format!("stream_id LIKE ?{}", i + 1))
            .collect();
        let sql = format!(
            "SELECT event_id, stream_id, version, event_type, payload, metadata, created_at
             FROM event_store WHERE {} ORDER BY event_id ASC",
            conditions.join(" OR ")
        );

        let like_params: Vec<String> = prefixes.iter().map(|p| format!("{p}%")).collect();
        // Use positional params via libsql::params_from_iter
        let values: Vec<libsql::Value> = like_params.into_iter().map(libsql::Value::from).collect();

        let mut rows = self.conn.query(&sql, values).await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(parse_stored_event(&row)?);
        }
        Ok(out)
    }
}

/// Extract a human-readable event type label from a `FlowEvent`.
fn event_type_label(event: &FlowEvent) -> String {
    match event {
        FlowEvent::Epic(e) => format!("epic:{}", serde_json::to_value(e).unwrap_or_default().as_str().unwrap_or("unknown")),
        FlowEvent::Task(t) => format!("task:{}", serde_json::to_value(t).unwrap_or_default().as_str().unwrap_or("unknown")),
    }
}

/// Parse a row from the event_store table into a `StoredEvent`.
fn parse_stored_event(row: &libsql::Row) -> Result<StoredEvent, DbError> {
    let payload_str: String = row.get::<String>(4)?;
    let metadata_str: Option<String> = row.get::<Option<String>>(5)?;

    Ok(StoredEvent {
        event_id: row.get::<i64>(0)?,
        stream_id: row.get::<String>(1)?,
        version: row.get::<i64>(2)?,
        event_type: row.get::<String>(3)?,
        payload: serde_json::from_str(&payload_str)?,
        metadata: match metadata_str {
            Some(s) if !s.is_empty() => Some(serde_json::from_str(&s)?),
            _ => None,
        },
        created_at: row.get::<String>(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;
    use flowctl_core::events::{EpicEvent, TaskEvent};

    fn test_metadata() -> EventMetadata {
        EventMetadata {
            actor: "test".into(),
            source_cmd: "test".into(),
            session_id: "sess-1".into(),
            timestamp: None,
        }
    }

    #[tokio::test]
    async fn append_auto_increments_version() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        let v1 = repo
            .append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Created), &test_metadata())
            .await
            .unwrap();
        assert_eq!(v1, 1);

        let v2 = repo
            .append("epic:fn-1", &FlowEvent::Epic(EpicEvent::PlanWritten), &test_metadata())
            .await
            .unwrap();
        assert_eq!(v2, 2);

        // Different stream starts at 1.
        let v1b = repo
            .append("task:fn-1.1", &FlowEvent::Task(TaskEvent::Created), &test_metadata())
            .await
            .unwrap();
        assert_eq!(v1b, 1);
    }

    #[tokio::test]
    async fn query_stream_returns_version_order() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Created), &test_metadata()).await.unwrap();
        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::PlanWritten), &test_metadata()).await.unwrap();
        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Closed), &test_metadata()).await.unwrap();

        let events = repo.query_stream("epic:fn-1").await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].version, 1);
        assert_eq!(events[1].version, 2);
        assert_eq!(events[2].version, 3);
        assert_eq!(events[0].payload, FlowEvent::Epic(EpicEvent::Created));
        assert_eq!(events[2].payload, FlowEvent::Epic(EpicEvent::Closed));
    }

    #[tokio::test]
    async fn query_by_type_across_streams() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Created), &test_metadata()).await.unwrap();
        repo.append("epic:fn-2", &FlowEvent::Epic(EpicEvent::Created), &test_metadata()).await.unwrap();
        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Closed), &test_metadata()).await.unwrap();

        let created = repo.query_by_type("epic:created").await.unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].stream_id, "epic:fn-1");
        assert_eq!(created[1].stream_id, "epic:fn-2");
    }

    #[tokio::test]
    async fn rebuild_stream_replays_all_events() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        repo.append("task:fn-1.1", &FlowEvent::Task(TaskEvent::Created), &test_metadata()).await.unwrap();
        repo.append("task:fn-1.1", &FlowEvent::Task(TaskEvent::Started), &test_metadata()).await.unwrap();
        repo.append("task:fn-1.1", &FlowEvent::Task(TaskEvent::Completed), &test_metadata()).await.unwrap();

        let events = repo.rebuild_stream("task:fn-1.1").await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].payload, FlowEvent::Task(TaskEvent::Created));
        assert_eq!(events[1].payload, FlowEvent::Task(TaskEvent::Started));
        assert_eq!(events[2].payload, FlowEvent::Task(TaskEvent::Completed));
    }

    #[tokio::test]
    async fn optimistic_concurrency_conflict() {
        let (_db, conn) = open_memory_async().await.unwrap();

        // Directly insert two rows with the same (stream_id, version) to verify
        // the unique constraint fires correctly.
        conn.execute(
            "INSERT INTO event_store (stream_id, version, event_type, payload, metadata)
             VALUES ('epic:fn-1', 1, 'epic:created', '{}', '{}')",
            (),
        )
        .await
        .unwrap();

        // Second insert with the same stream_id + version should fail.
        let result = conn
            .execute(
                "INSERT OR FAIL INTO event_store (stream_id, version, event_type, payload, metadata)
                 VALUES ('epic:fn-1', 1, 'epic:plan_written', '{}', '{}')",
                (),
            )
            .await;

        assert!(result.is_err(), "expected UNIQUE constraint failure");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("UNIQUE") || err_msg.contains("constraint"),
            "expected constraint error, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn metadata_round_trips() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        let meta = EventMetadata {
            actor: "worker-1".into(),
            source_cmd: "flowctl done".into(),
            session_id: "sess-xyz".into(),
            timestamp: Some("2026-04-07T12:00:00Z".into()),
        };

        repo.append("epic:fn-1", &FlowEvent::Epic(EpicEvent::Created), &meta).await.unwrap();
        let events = repo.query_stream("epic:fn-1").await.unwrap();
        assert_eq!(events.len(), 1);

        let got_meta = events[0].metadata.as_ref().expect("metadata should exist");
        assert_eq!(got_meta.actor, "worker-1");
        assert_eq!(got_meta.source_cmd, "flowctl done");
        assert_eq!(got_meta.session_id, "sess-xyz");
    }

    #[tokio::test]
    async fn empty_stream_returns_empty() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = EventStoreRepo::new(conn);

        let events = repo.query_stream("nonexistent").await.unwrap();
        assert!(events.is_empty());

        let rebuilt = repo.rebuild_stream("nonexistent").await.unwrap();
        assert!(rebuilt.is_empty());
    }
}
