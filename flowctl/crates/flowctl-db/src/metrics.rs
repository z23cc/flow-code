//! Stats queries: summary, per-epic, weekly trends, token/cost analysis,
//! bottleneck analysis, DORA metrics, and monthly rollup generation.

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::DbError;

/// Overall summary stats.
#[derive(Debug, Serialize)]
pub struct Summary {
    pub total_epics: i64,
    pub open_epics: i64,
    pub total_tasks: i64,
    pub done_tasks: i64,
    pub in_progress_tasks: i64,
    pub blocked_tasks: i64,
    pub total_events: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
}

/// Per-epic stats row.
#[derive(Debug, Serialize)]
pub struct EpicStats {
    pub epic_id: String,
    pub title: String,
    pub status: String,
    pub task_count: i64,
    pub done_count: i64,
    pub avg_duration_secs: Option<f64>,
    pub total_tokens: i64,
    pub total_cost: f64,
}

/// Weekly trend data point.
#[derive(Debug, Serialize)]
pub struct WeeklyTrend {
    pub week: String,
    pub tasks_started: i64,
    pub tasks_completed: i64,
    pub tasks_failed: i64,
}

/// Token usage breakdown.
#[derive(Debug, Serialize)]
pub struct TokenBreakdown {
    pub epic_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: f64,
}

/// Bottleneck: tasks that took longest or were blocked.
#[derive(Debug, Serialize)]
pub struct Bottleneck {
    pub task_id: String,
    pub epic_id: String,
    pub title: String,
    pub duration_secs: Option<i64>,
    pub status: String,
    pub blocked_reason: Option<String>,
}

/// DORA metrics.
#[derive(Debug, Serialize)]
pub struct DoraMetrics {
    /// Average hours from task creation to completion (last 30 days).
    pub lead_time_hours: Option<f64>,
    /// Tasks completed per week (last 4 weeks average).
    pub throughput_per_week: f64,
    /// Ratio of failed tasks to total completed (last 30 days).
    pub change_failure_rate: f64,
    /// Average hours from block to unblock (last 30 days).
    pub time_to_restore_hours: Option<f64>,
}

/// Stats query engine.
pub struct StatsQuery<'a> {
    conn: &'a Connection,
}

impl<'a> StatsQuery<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Overall summary across all epics.
    pub fn summary(&self) -> Result<Summary, DbError> {
        let total_epics: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM epics", [], |row| row.get(0),
        )?;
        let open_epics: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM epics WHERE status = 'open'", [], |row| row.get(0),
        )?;
        let total_tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks", [], |row| row.get(0),
        )?;
        let done_tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'done'", [], |row| row.get(0),
        )?;
        let in_progress_tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'in_progress'", [], |row| row.get(0),
        )?;
        let blocked_tasks: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'blocked'", [], |row| row.get(0),
        )?;
        let total_events: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events", [], |row| row.get(0),
        )?;
        let total_tokens: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM token_usage", [], |row| row.get(0),
        )?;
        let total_cost_usd: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(estimated_cost), 0.0) FROM token_usage", [], |row| row.get(0),
        )?;

        Ok(Summary {
            total_epics,
            open_epics,
            total_tasks,
            done_tasks,
            in_progress_tasks,
            blocked_tasks,
            total_events,
            total_tokens,
            total_cost_usd,
        })
    }

    /// Per-epic stats.
    pub fn per_epic(&self, epic_id: Option<&str>) -> Result<Vec<EpicStats>, DbError> {
        let (sql, filter) = match epic_id {
            Some(id) => (
                "SELECT e.id, e.title, e.status,
                        (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id),
                        (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id AND t.status = 'done'),
                        (SELECT AVG(rs.duration_secs) FROM runtime_state rs
                         JOIN tasks t ON t.id = rs.task_id WHERE t.epic_id = e.id AND rs.duration_secs IS NOT NULL),
                        COALESCE((SELECT SUM(tu.input_tokens + tu.output_tokens) FROM token_usage tu WHERE tu.epic_id = e.id), 0),
                        COALESCE((SELECT SUM(tu.estimated_cost) FROM token_usage tu WHERE tu.epic_id = e.id), 0.0)
                 FROM epics e WHERE e.id = ?1",
                Some(id.to_string()),
            ),
            None => (
                "SELECT e.id, e.title, e.status,
                        (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id),
                        (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id AND t.status = 'done'),
                        (SELECT AVG(rs.duration_secs) FROM runtime_state rs
                         JOIN tasks t ON t.id = rs.task_id WHERE t.epic_id = e.id AND rs.duration_secs IS NOT NULL),
                        COALESCE((SELECT SUM(tu.input_tokens + tu.output_tokens) FROM token_usage tu WHERE tu.epic_id = e.id), 0),
                        COALESCE((SELECT SUM(tu.estimated_cost) FROM token_usage tu WHERE tu.epic_id = e.id), 0.0)
                 FROM epics e ORDER BY e.created_at",
                None,
            ),
        };

        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(ref id) = filter {
            stmt.query_map(params![id], map_epic_stats)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map_epic_stats)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    /// Weekly trends from daily_rollup (last N weeks).
    pub fn weekly_trends(&self, weeks: u32) -> Result<Vec<WeeklyTrend>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%Y-W%W', day) AS week,
                    SUM(tasks_started), SUM(tasks_completed), SUM(tasks_failed)
             FROM daily_rollup
             WHERE day >= strftime('%Y-%m-%d', 'now', ?1)
             GROUP BY week ORDER BY week",
        )?;

        let offset = format!("-{} days", weeks * 7);
        let rows = stmt
            .query_map(params![offset], |row| {
                Ok(WeeklyTrend {
                    week: row.get(0)?,
                    tasks_started: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    tasks_completed: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    tasks_failed: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Token/cost breakdown by epic and model.
    pub fn token_breakdown(&self, epic_id: Option<&str>) -> Result<Vec<TokenBreakdown>, DbError> {
        let (sql, filter) = match epic_id {
            Some(id) => (
                "SELECT epic_id, COALESCE(model, 'unknown'), SUM(input_tokens), SUM(output_tokens),
                        SUM(cache_read), SUM(cache_write), SUM(estimated_cost)
                 FROM token_usage WHERE epic_id = ?1
                 GROUP BY epic_id, model ORDER BY SUM(estimated_cost) DESC",
                Some(id.to_string()),
            ),
            None => (
                "SELECT epic_id, COALESCE(model, 'unknown'), SUM(input_tokens), SUM(output_tokens),
                        SUM(cache_read), SUM(cache_write), SUM(estimated_cost)
                 FROM token_usage
                 GROUP BY epic_id, model ORDER BY SUM(estimated_cost) DESC",
                None,
            ),
        };

        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(ref id) = filter {
            stmt.query_map(params![id], map_token_breakdown)?
                .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map_token_breakdown)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    /// Bottleneck analysis: longest-running and blocked tasks.
    pub fn bottlenecks(&self, limit: usize) -> Result<Vec<Bottleneck>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.epic_id, t.title, rs.duration_secs, t.status, rs.blocked_reason
             FROM tasks t
             LEFT JOIN runtime_state rs ON rs.task_id = t.id
             WHERE t.status IN ('done', 'blocked', 'in_progress')
             ORDER BY
                 CASE WHEN t.status = 'blocked' THEN 0 ELSE 1 END,
                 rs.duration_secs DESC NULLS LAST
             LIMIT ?1",
        )?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(Bottleneck {
                    task_id: row.get(0)?,
                    epic_id: row.get(1)?,
                    title: row.get(2)?,
                    duration_secs: row.get(3)?,
                    status: row.get(4)?,
                    blocked_reason: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// DORA-style metrics computed from events and runtime state.
    pub fn dora_metrics(&self) -> Result<DoraMetrics, DbError> {
        // Lead time: avg seconds from task creation to completion (last 30 days)
        let lead_time_secs: Option<f64> = self.conn.query_row(
            "SELECT AVG(rs.duration_secs)
             FROM runtime_state rs
             WHERE rs.completed_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days')
               AND rs.duration_secs IS NOT NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(None);

        // Throughput: tasks completed in last 28 days / 4
        let completed_28d: f64 = self.conn.query_row(
            "SELECT CAST(COUNT(*) AS REAL) FROM runtime_state
             WHERE completed_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-28 days')
               AND completed_at IS NOT NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Change failure rate: task_failed / (task_completed + task_failed) in last 30 days
        let (completed_30d, failed_30d): (f64, f64) = self.conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN event_type = 'task_completed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN event_type = 'task_failed' THEN 1 ELSE 0 END), 0)
             FROM events
             WHERE timestamp >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days')
               AND event_type IN ('task_completed', 'task_failed')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0.0, 0.0));

        let change_failure_rate = if (completed_30d + failed_30d) > 0.0 {
            failed_30d / (completed_30d + failed_30d)
        } else {
            0.0
        };

        // Time to restore: avg hours blocked tasks spent blocked
        // Approximated from events: time between task_blocked and task_started (resume)
        // Simplified: count blocked tasks with duration in runtime_state
        let ttr_secs: Option<f64> = self.conn.query_row(
            "SELECT AVG(CAST(
                (julianday(rs.completed_at) - julianday(rs.claimed_at)) * 86400 AS REAL
             ))
             FROM runtime_state rs
             WHERE rs.blocked_reason IS NOT NULL
               AND rs.completed_at IS NOT NULL
               AND rs.claimed_at IS NOT NULL",
            [],
            |row| row.get(0),
        ).unwrap_or(None);

        Ok(DoraMetrics {
            lead_time_hours: lead_time_secs.map(|s| s / 3600.0),
            throughput_per_week: completed_28d / 4.0,
            change_failure_rate,
            time_to_restore_hours: ttr_secs.map(|s| s / 3600.0),
        })
    }

    /// Generate monthly rollup for any months that have daily_rollup data but no monthly entry.
    pub fn generate_monthly_rollups(&self) -> Result<usize, DbError> {
        let rows = self.conn.execute(
            "INSERT OR REPLACE INTO monthly_rollup (month, epics_completed, tasks_completed, avg_lead_time_h, total_tokens, total_cost_usd)
             SELECT
                 strftime('%Y-%m', day) AS month,
                 COALESCE((SELECT COUNT(*) FROM epics e WHERE e.status = 'done'
                           AND strftime('%Y-%m', e.updated_at) = strftime('%Y-%m', dr.day)), 0),
                 SUM(dr.tasks_completed),
                 COALESCE((SELECT AVG(rs.duration_secs) / 3600.0 FROM runtime_state rs
                           WHERE rs.completed_at IS NOT NULL
                           AND strftime('%Y-%m', rs.completed_at) = strftime('%Y-%m', dr.day)), 0),
                 COALESCE((SELECT SUM(tu.input_tokens + tu.output_tokens) FROM token_usage tu
                           WHERE strftime('%Y-%m', tu.timestamp) = strftime('%Y-%m', dr.day)), 0),
                 COALESCE((SELECT SUM(tu.estimated_cost) FROM token_usage tu
                           WHERE strftime('%Y-%m', tu.timestamp) = strftime('%Y-%m', dr.day)), 0.0)
             FROM daily_rollup dr
             GROUP BY strftime('%Y-%m', day)",
            [],
        )?;
        Ok(rows)
    }
}

fn map_epic_stats(row: &rusqlite::Row) -> rusqlite::Result<EpicStats> {
    Ok(EpicStats {
        epic_id: row.get(0)?,
        title: row.get(1)?,
        status: row.get(2)?,
        task_count: row.get(3)?,
        done_count: row.get(4)?,
        avg_duration_secs: row.get(5)?,
        total_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
        total_cost: row.get::<_, Option<f64>>(7)?.unwrap_or(0.0),
    })
}

fn map_token_breakdown(row: &rusqlite::Row) -> rusqlite::Result<TokenBreakdown> {
    Ok(TokenBreakdown {
        epic_id: row.get(0)?,
        model: row.get(1)?,
        input_tokens: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
        output_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
        cache_read: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
        cache_write: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        estimated_cost: row.get::<_, Option<f64>>(6)?.unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory;
    use crate::repo::EventRepo;

    fn setup() -> Connection {
        let conn = open_memory().expect("in-memory db");
        // Insert test epic
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test Epic', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        // Insert test tasks
        conn.execute(
            "INSERT INTO tasks (id, epic_id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test.1', 'fn-1-test', 'Task 1', 'done', 't1.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO tasks (id, epic_id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test.2', 'fn-1-test', 'Task 2', 'in_progress', 't2.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        ).unwrap();
        conn
    }

    #[test]
    fn test_summary() {
        let conn = setup();
        let stats = StatsQuery::new(&conn);
        let s = stats.summary().unwrap();
        assert_eq!(s.total_epics, 1);
        assert_eq!(s.total_tasks, 2);
        assert_eq!(s.done_tasks, 1);
        assert_eq!(s.in_progress_tasks, 1);
    }

    #[test]
    fn test_per_epic() {
        let conn = setup();
        let stats = StatsQuery::new(&conn);
        let epics = stats.per_epic(None).unwrap();
        assert_eq!(epics.len(), 1);
        assert_eq!(epics[0].task_count, 2);
        assert_eq!(epics[0].done_count, 1);
    }

    #[test]
    fn test_per_epic_filtered() {
        let conn = setup();
        let stats = StatsQuery::new(&conn);
        let epics = stats.per_epic(Some("fn-1-test")).unwrap();
        assert_eq!(epics.len(), 1);
        assert_eq!(epics[0].epic_id, "fn-1-test");
    }

    #[test]
    fn test_weekly_trends() {
        let conn = setup();
        // Insert events to trigger daily_rollup
        let repo = EventRepo::new(&conn);
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_started", None, None, None).unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", None, None, None).unwrap();

        let stats = StatsQuery::new(&conn);
        let trends = stats.weekly_trends(4).unwrap();
        // Should have at least one week with data
        assert!(!trends.is_empty());
        assert!(trends[0].tasks_started > 0);
    }

    #[test]
    fn test_token_breakdown() {
        let conn = setup();
        conn.execute(
            "INSERT INTO token_usage (epic_id, task_id, model, input_tokens, output_tokens, estimated_cost)
             VALUES ('fn-1-test', 'fn-1-test.1', 'claude-sonnet-4-20250514', 1000, 500, 0.01)",
            [],
        ).unwrap();

        let stats = StatsQuery::new(&conn);
        let tokens = stats.token_breakdown(None).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].input_tokens, 1000);
        assert_eq!(tokens[0].output_tokens, 500);
    }

    #[test]
    fn test_bottlenecks() {
        let conn = setup();
        conn.execute(
            "INSERT INTO runtime_state (task_id, duration_secs) VALUES ('fn-1-test.1', 3600)",
            [],
        ).unwrap();

        let stats = StatsQuery::new(&conn);
        let bottlenecks = stats.bottlenecks(10).unwrap();
        assert!(!bottlenecks.is_empty());
        assert_eq!(bottlenecks[0].task_id, "fn-1-test.1");
    }

    #[test]
    fn test_dora_metrics() {
        let conn = setup();
        let stats = StatsQuery::new(&conn);
        let dora = stats.dora_metrics().unwrap();
        // Fresh DB, no completions in last 30 days
        assert_eq!(dora.throughput_per_week, 0.0);
        assert_eq!(dora.change_failure_rate, 0.0);
    }

    #[test]
    fn test_generate_monthly_rollups() {
        let conn = setup();
        let repo = EventRepo::new(&conn);
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", None, None, None).unwrap();

        let stats = StatsQuery::new(&conn);
        let count = stats.generate_monthly_rollups().unwrap();
        assert!(count > 0);

        // Verify monthly_rollup has data
        let tasks_completed: i64 = conn.query_row(
            "SELECT COALESCE(SUM(tasks_completed), 0) FROM monthly_rollup", [], |row| row.get(0),
        ).unwrap();
        assert!(tasks_completed > 0);
    }
}
