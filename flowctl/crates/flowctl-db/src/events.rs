//! Extended event logging: query events by type/timerange, record token usage.

use rusqlite::{params, Connection};

use crate::error::DbError;
use crate::repo::EventRow;

/// Extended event queries beyond the basic EventRepo.
pub struct EventLog<'a> {
    conn: &'a Connection,
}

impl<'a> EventLog<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Query events by type, optionally filtered by epic and time range.
    pub fn query(
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

        let mut stmt = self.conn.prepare(&sql)?;

        // Build params dynamically
        let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = param_values
            .iter()
            .map(|v| Box::new(v.clone()) as Box<dyn rusqlite::types::ToSql>)
            .collect();
        all_params.push(Box::new(limit as i64));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = all_params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(EventRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    epic_id: row.get(2)?,
                    task_id: row.get(3)?,
                    event_type: row.get(4)?,
                    actor: row.get(5)?,
                    payload: row.get(6)?,
                    session_id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Record token usage for a task/phase.
    pub fn record_tokens(
        &self,
        epic_id: &str,
        task_id: Option<&str>,
        phase: Option<&str>,
        model: Option<&str>,
        input_tokens: i64,
        output_tokens: i64,
        cache_read: i64,
        cache_write: i64,
        estimated_cost: Option<f64>,
    ) -> Result<i64, DbError> {
        self.conn.execute(
            "INSERT INTO token_usage (epic_id, task_id, phase, model, input_tokens, output_tokens, cache_read, cache_write, estimated_cost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![epic_id, task_id, phase, model, input_tokens, output_tokens, cache_read, cache_write, estimated_cost],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Count events by type for an epic.
    pub fn count_by_type(&self, epic_id: &str) -> Result<Vec<(String, i64)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT event_type, COUNT(*) FROM events WHERE epic_id = ?1 GROUP BY event_type ORDER BY COUNT(*) DESC",
        )?;
        let rows = stmt
            .query_map(params![epic_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;
    use crate::repo::EventRepo;

    fn setup() -> Connection {
        let conn = open_memory().expect("in-memory db");
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn test_query_by_type() {
        let conn = setup();
        let repo = EventRepo::new(&conn);
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_started", Some("w"), None, None).unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", Some("w"), None, None).unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.2"), "task_started", Some("w"), None, None).unwrap();

        let log = EventLog::new(&conn);
        let started = log.query(Some("task_started"), None, None, None, 100).unwrap();
        assert_eq!(started.len(), 2);

        let completed = log.query(Some("task_completed"), Some("fn-1-test"), None, None, 100).unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_record_tokens() {
        let conn = setup();
        let log = EventLog::new(&conn);
        let id = log.record_tokens("fn-1-test", Some("fn-1-test.1"), Some("impl"), Some("claude-sonnet-4-20250514"), 1000, 500, 200, 100, Some(0.015)).unwrap();
        assert!(id > 0);

        let total: i64 = conn.query_row(
            "SELECT SUM(input_tokens + output_tokens) FROM token_usage WHERE epic_id = 'fn-1-test'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(total, 1500);
    }

    #[test]
    fn test_count_by_type() {
        let conn = setup();
        let repo = EventRepo::new(&conn);
        repo.insert("fn-1-test", None, "task_started", None, None, None).unwrap();
        repo.insert("fn-1-test", None, "task_started", None, None, None).unwrap();
        repo.insert("fn-1-test", None, "task_completed", None, None, None).unwrap();

        let log = EventLog::new(&conn);
        let counts = log.count_by_type("fn-1-test").unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0], ("task_started".to_string(), 2));
        assert_eq!(counts[1], ("task_completed".to_string(), 1));
    }
}
