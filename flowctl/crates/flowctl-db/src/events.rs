//! Extended event logging: query events by type/timerange, record token usage.
//!
//! Ported from `flowctl-db::events` to async libSQL. All methods take
//! an owned `libsql::Connection` (cheap Clone) and are async.

use libsql::{params, Connection};

use crate::error::DbError;
use crate::repo::EventRow;

/// Token usage record for a task/phase.
pub struct TokenRecord<'a> {
    pub epic_id: &'a str,
    pub task_id: Option<&'a str>,
    pub phase: Option<&'a str>,
    pub model: Option<&'a str>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: Option<f64>,
}

/// A row from the token_usage table.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenUsageRow {
    pub id: i64,
    pub timestamp: String,
    pub epic_id: String,
    pub task_id: Option<String>,
    pub phase: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: Option<f64>,
}

/// Aggregated token usage for a single task.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskTokenSummary {
    pub task_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: f64,
}

/// Extended async event queries beyond the basic EventRepo.
pub struct EventLog {
    conn: Connection,
}

impl EventLog {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Query events by type, optionally filtered by epic and time range.
    pub async fn query(
        &self,
        event_type: Option<&str>,
        epic_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        let mut conditions = Vec::new();
        let mut param_values: Vec<String> = Vec::new();

        if let Some(et) = event_type {
            param_values.push(et.to_string());
            conditions.push(format!("event_type = ?{}", param_values.len()));
        }
        if let Some(eid) = epic_id {
            param_values.push(eid.to_string());
            conditions.push(format!("epic_id = ?{}", param_values.len()));
        }
        if let Some(s) = since {
            param_values.push(s.to_string());
            conditions.push(format!("timestamp >= ?{}", param_values.len()));
        }
        if let Some(u) = until {
            param_values.push(u.to_string());
            conditions.push(format!("timestamp <= ?{}", param_values.len()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, timestamp, epic_id, task_id, event_type, actor, payload, session_id
             FROM events {where_clause} ORDER BY id DESC LIMIT ?{}",
            param_values.len() + 1
        );

        // Build libsql Params: Vec<libsql::Value>
        let mut values: Vec<libsql::Value> = param_values
            .into_iter()
            .map(libsql::Value::Text)
            .collect();
        values.push(libsql::Value::Integer(limit as i64));

        let mut rows = self
            .conn
            .query(&sql, libsql::params::Params::Positional(values))
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

    /// Shortcut: query events by type.
    pub async fn query_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<EventRow>, DbError> {
        self.query(Some(event_type), None, None, None, limit).await
    }

    /// Record token usage for a task/phase. Returns the inserted row id.
    pub async fn record_token_usage(&self, rec: &TokenRecord<'_>) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO token_usage (epic_id, task_id, phase, model, input_tokens, output_tokens, cache_read, cache_write, estimated_cost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rec.epic_id.to_string(),
                rec.task_id.map(|s| s.to_string()),
                rec.phase.map(|s| s.to_string()),
                rec.model.map(|s| s.to_string()),
                rec.input_tokens,
                rec.output_tokens,
                rec.cache_read,
                rec.cache_write,
                rec.estimated_cost,
            ],
        ).await?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Get all token records for a specific task.
    pub async fn tokens_by_task(&self, task_id: &str) -> Result<Vec<TokenUsageRow>, DbError> {
        let mut rows = self.conn.query(
            "SELECT id, timestamp, epic_id, task_id, phase, model, input_tokens, output_tokens, cache_read, cache_write, estimated_cost
             FROM token_usage WHERE task_id = ?1 ORDER BY id ASC",
            params![task_id.to_string()],
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(TokenUsageRow {
                id: row.get::<i64>(0)?,
                timestamp: row.get::<String>(1)?,
                epic_id: row.get::<String>(2)?,
                task_id: row.get::<Option<String>>(3)?,
                phase: row.get::<Option<String>>(4)?,
                model: row.get::<Option<String>>(5)?,
                input_tokens: row.get::<i64>(6)?,
                output_tokens: row.get::<i64>(7)?,
                cache_read: row.get::<i64>(8)?,
                cache_write: row.get::<i64>(9)?,
                estimated_cost: row.get::<Option<f64>>(10)?,
            });
        }
        Ok(out)
    }

    /// Get aggregated token usage per task for an epic.
    pub async fn tokens_by_epic(&self, epic_id: &str) -> Result<Vec<TaskTokenSummary>, DbError> {
        let mut rows = self.conn.query(
            "SELECT task_id, COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0),
                    COALESCE(SUM(cache_read), 0), COALESCE(SUM(cache_write), 0),
                    COALESCE(SUM(estimated_cost), 0.0)
             FROM token_usage WHERE epic_id = ?1 AND task_id IS NOT NULL
             GROUP BY task_id ORDER BY task_id",
            params![epic_id.to_string()],
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(TaskTokenSummary {
                task_id: row.get::<String>(0)?,
                input_tokens: row.get::<i64>(1)?,
                output_tokens: row.get::<i64>(2)?,
                cache_read: row.get::<i64>(3)?,
                cache_write: row.get::<i64>(4)?,
                estimated_cost: row.get::<f64>(5)?,
            });
        }
        Ok(out)
    }

    /// Count events by type for an epic.
    pub async fn count_by_type(&self, epic_id: &str) -> Result<Vec<(String, i64)>, DbError> {
        let mut rows = self.conn.query(
            "SELECT event_type, COUNT(*) FROM events WHERE epic_id = ?1 GROUP BY event_type ORDER BY COUNT(*) DESC",
            params![epic_id.to_string()],
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push((row.get::<String>(0)?, row.get::<i64>(1)?));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;
    use crate::repo::EventRepo;
    use libsql::Database;

    async fn setup() -> (Database, Connection) {
        let (db, conn) = open_memory_async().await.expect("in-memory db");
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        (db, conn)
    }

    #[tokio::test]
    async fn test_query_by_type() {
        let (_db, conn) = setup().await;
        let repo = EventRepo::new(conn.clone());
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_started", Some("w"), None, None).await.unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", Some("w"), None, None).await.unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.2"), "task_started", Some("w"), None, None).await.unwrap();

        let log = EventLog::new(conn.clone());
        let started = log.query(Some("task_started"), None, None, None, 100).await.unwrap();
        assert_eq!(started.len(), 2);

        let completed = log.query(Some("task_completed"), Some("fn-1-test"), None, None, 100).await.unwrap();
        assert_eq!(completed.len(), 1);

        let all = log.query_by_type("task_started", 10).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_record_token_usage() {
        let (_db, conn) = setup().await;
        let log = EventLog::new(conn.clone());
        let id = log.record_token_usage(&TokenRecord {
            epic_id: "fn-1-test",
            task_id: Some("fn-1-test.1"),
            phase: Some("impl"),
            model: Some("claude-sonnet-4-20250514"),
            input_tokens: 1000,
            output_tokens: 500,
            cache_read: 200,
            cache_write: 100,
            estimated_cost: Some(0.015),
        }).await.unwrap();
        assert!(id > 0);

        let mut rows = conn.query(
            "SELECT SUM(input_tokens + output_tokens) FROM token_usage WHERE epic_id = 'fn-1-test'",
            (),
        ).await.unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let total: i64 = row.get(0).unwrap();
        assert_eq!(total, 1500);
    }

    #[tokio::test]
    async fn test_count_by_type() {
        let (_db, conn) = setup().await;
        let repo = EventRepo::new(conn.clone());
        repo.insert("fn-1-test", None, "task_started", None, None, None).await.unwrap();
        repo.insert("fn-1-test", None, "task_started", None, None, None).await.unwrap();
        repo.insert("fn-1-test", None, "task_completed", None, None, None).await.unwrap();

        let log = EventLog::new(conn);
        let counts = log.count_by_type("fn-1-test").await.unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0], ("task_started".to_string(), 2));
        assert_eq!(counts[1], ("task_completed".to_string(), 1));
    }

    #[tokio::test]
    async fn test_tokens_by_task_and_epic() {
        let (_db, conn) = setup().await;
        let log = EventLog::new(conn);
        log.record_token_usage(&TokenRecord {
            epic_id: "fn-1-test",
            task_id: Some("fn-1-test.1"),
            phase: Some("impl"),
            model: None,
            input_tokens: 1000,
            output_tokens: 500,
            cache_read: 100,
            cache_write: 50,
            estimated_cost: Some(0.015),
        }).await.unwrap();
        log.record_token_usage(&TokenRecord {
            epic_id: "fn-1-test",
            task_id: Some("fn-1-test.1"),
            phase: Some("review"),
            model: None,
            input_tokens: 800,
            output_tokens: 300,
            cache_read: 0,
            cache_write: 0,
            estimated_cost: Some(0.010),
        }).await.unwrap();
        log.record_token_usage(&TokenRecord {
            epic_id: "fn-1-test",
            task_id: Some("fn-1-test.2"),
            phase: Some("impl"),
            model: None,
            input_tokens: 500,
            output_tokens: 200,
            cache_read: 0,
            cache_write: 0,
            estimated_cost: Some(0.005),
        }).await.unwrap();

        let t1_rows = log.tokens_by_task("fn-1-test.1").await.unwrap();
        assert_eq!(t1_rows.len(), 2);
        assert_eq!(t1_rows[0].input_tokens, 1000);

        let summaries = log.tokens_by_epic("fn-1-test").await.unwrap();
        assert_eq!(summaries.len(), 2);
        let t1 = summaries.iter().find(|s| s.task_id == "fn-1-test.1").unwrap();
        assert_eq!(t1.input_tokens, 1800);
        assert_eq!(t1.output_tokens, 800);
        assert!((t1.estimated_cost - 0.025).abs() < 0.001);
    }
}
