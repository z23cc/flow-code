//! Stats queries: summary, per-epic, weekly trends, token/cost analysis,
//! bottleneck analysis, DORA metrics, domain duration stats, monthly rollup.
//!
//! Ported from `flowctl-db::metrics` to async libSQL. All methods take an
//! owned `libsql::Connection` (cheap Clone) and are async.

use libsql::{params, Connection};
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
    pub lead_time_hours: Option<f64>,
    pub throughput_per_week: f64,
    pub change_failure_rate: f64,
    pub time_to_restore_hours: Option<f64>,
}

/// Per-domain historical duration statistics.
#[derive(Debug, Clone, Serialize)]
pub struct DomainDurationStats {
    pub domain: String,
    pub completed_count: i64,
    pub avg_duration_secs: f64,
    pub stddev_duration_secs: f64,
}

/// Async stats query engine.
pub struct StatsQuery {
    conn: Connection,
}

impl StatsQuery {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    async fn scalar_i64(&self, sql: &str) -> Result<i64, DbError> {
        let mut rows = self.conn.query(sql, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| DbError::NotFound("scalar query".into()))?;
        Ok(row.get::<i64>(0)?)
    }

    async fn scalar_f64(&self, sql: &str) -> Result<f64, DbError> {
        let mut rows = self.conn.query(sql, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| DbError::NotFound("scalar query".into()))?;
        Ok(row.get::<f64>(0)?)
    }

    /// Overall summary across all epics.
    pub async fn summary(&self) -> Result<Summary, DbError> {
        Ok(Summary {
            total_epics: self.scalar_i64("SELECT COUNT(*) FROM epics").await?,
            open_epics: self
                .scalar_i64("SELECT COUNT(*) FROM epics WHERE status = 'open'")
                .await?,
            total_tasks: self.scalar_i64("SELECT COUNT(*) FROM tasks").await?,
            done_tasks: self
                .scalar_i64("SELECT COUNT(*) FROM tasks WHERE status = 'done'")
                .await?,
            in_progress_tasks: self
                .scalar_i64("SELECT COUNT(*) FROM tasks WHERE status = 'in_progress'")
                .await?,
            blocked_tasks: self
                .scalar_i64("SELECT COUNT(*) FROM tasks WHERE status = 'blocked'")
                .await?,
            total_events: self.scalar_i64("SELECT COUNT(*) FROM events").await?,
            total_tokens: self
                .scalar_i64(
                    "SELECT COALESCE(SUM(input_tokens + output_tokens), 0) FROM token_usage",
                )
                .await?,
            total_cost_usd: self
                .scalar_f64("SELECT COALESCE(SUM(estimated_cost), 0.0) FROM token_usage")
                .await?,
        })
    }

    /// Per-epic stats.
    pub async fn epic_stats(&self, epic_id: Option<&str>) -> Result<Vec<EpicStats>, DbError> {
        let mut rows = match epic_id {
            Some(id) => {
                self.conn.query(
                    "SELECT e.id, e.title, e.status,
                            (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id),
                            (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id AND t.status = 'done'),
                            (SELECT AVG(rs.duration_secs) FROM runtime_state rs
                             JOIN tasks t ON t.id = rs.task_id WHERE t.epic_id = e.id AND rs.duration_secs IS NOT NULL),
                            COALESCE((SELECT SUM(tu.input_tokens + tu.output_tokens) FROM token_usage tu WHERE tu.epic_id = e.id), 0),
                            COALESCE((SELECT SUM(tu.estimated_cost) FROM token_usage tu WHERE tu.epic_id = e.id), 0.0)
                     FROM epics e WHERE e.id = ?1",
                    params![id.to_string()],
                ).await?
            }
            None => {
                self.conn.query(
                    "SELECT e.id, e.title, e.status,
                            (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id),
                            (SELECT COUNT(*) FROM tasks t WHERE t.epic_id = e.id AND t.status = 'done'),
                            (SELECT AVG(rs.duration_secs) FROM runtime_state rs
                             JOIN tasks t ON t.id = rs.task_id WHERE t.epic_id = e.id AND rs.duration_secs IS NOT NULL),
                            COALESCE((SELECT SUM(tu.input_tokens + tu.output_tokens) FROM token_usage tu WHERE tu.epic_id = e.id), 0),
                            COALESCE((SELECT SUM(tu.estimated_cost) FROM token_usage tu WHERE tu.epic_id = e.id), 0.0)
                     FROM epics e ORDER BY e.created_at",
                    (),
                ).await?
            }
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(EpicStats {
                epic_id: row.get::<String>(0)?,
                title: row.get::<String>(1)?,
                status: row.get::<String>(2)?,
                task_count: row.get::<i64>(3)?,
                done_count: row.get::<i64>(4)?,
                avg_duration_secs: row.get::<Option<f64>>(5)?,
                total_tokens: row.get::<Option<i64>>(6)?.unwrap_or(0),
                total_cost: row.get::<Option<f64>>(7)?.unwrap_or(0.0),
            });
        }
        Ok(out)
    }

    /// Weekly trends from daily_rollup (last N weeks).
    pub async fn weekly_trends(&self, weeks: u32) -> Result<Vec<WeeklyTrend>, DbError> {
        let offset = format!("-{} days", weeks * 7);
        let mut rows = self.conn.query(
            "SELECT strftime('%Y-W%W', day) AS week,
                    SUM(tasks_started), SUM(tasks_completed), SUM(tasks_failed)
             FROM daily_rollup
             WHERE day >= strftime('%Y-%m-%d', 'now', ?1)
             GROUP BY week ORDER BY week",
            params![offset],
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(WeeklyTrend {
                week: row.get::<String>(0)?,
                tasks_started: row.get::<Option<i64>>(1)?.unwrap_or(0),
                tasks_completed: row.get::<Option<i64>>(2)?.unwrap_or(0),
                tasks_failed: row.get::<Option<i64>>(3)?.unwrap_or(0),
            });
        }
        Ok(out)
    }

    /// Token/cost breakdown by epic and model.
    pub async fn token_breakdown(&self, epic_id: Option<&str>) -> Result<Vec<TokenBreakdown>, DbError> {
        let mut rows = match epic_id {
            Some(id) => {
                self.conn.query(
                    "SELECT epic_id, COALESCE(model, 'unknown'), SUM(input_tokens), SUM(output_tokens),
                            SUM(cache_read), SUM(cache_write), SUM(estimated_cost)
                     FROM token_usage WHERE epic_id = ?1
                     GROUP BY epic_id, model ORDER BY SUM(estimated_cost) DESC",
                    params![id.to_string()],
                ).await?
            }
            None => {
                self.conn.query(
                    "SELECT epic_id, COALESCE(model, 'unknown'), SUM(input_tokens), SUM(output_tokens),
                            SUM(cache_read), SUM(cache_write), SUM(estimated_cost)
                     FROM token_usage
                     GROUP BY epic_id, model ORDER BY SUM(estimated_cost) DESC",
                    (),
                ).await?
            }
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(TokenBreakdown {
                epic_id: row.get::<String>(0)?,
                model: row.get::<String>(1)?,
                input_tokens: row.get::<Option<i64>>(2)?.unwrap_or(0),
                output_tokens: row.get::<Option<i64>>(3)?.unwrap_or(0),
                cache_read: row.get::<Option<i64>>(4)?.unwrap_or(0),
                cache_write: row.get::<Option<i64>>(5)?.unwrap_or(0),
                estimated_cost: row.get::<Option<f64>>(6)?.unwrap_or(0.0),
            });
        }
        Ok(out)
    }

    /// Bottleneck analysis: longest-running and blocked tasks.
    pub async fn bottlenecks(&self, limit: usize) -> Result<Vec<Bottleneck>, DbError> {
        let mut rows = self.conn.query(
            "SELECT t.id, t.epic_id, t.title, rs.duration_secs, t.status, rs.blocked_reason
             FROM tasks t
             LEFT JOIN runtime_state rs ON rs.task_id = t.id
             WHERE t.status IN ('done', 'blocked', 'in_progress')
             ORDER BY
                 CASE WHEN t.status = 'blocked' THEN 0 ELSE 1 END,
                 rs.duration_secs DESC NULLS LAST
             LIMIT ?1",
            params![limit as i64],
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(Bottleneck {
                task_id: row.get::<String>(0)?,
                epic_id: row.get::<String>(1)?,
                title: row.get::<String>(2)?,
                duration_secs: row.get::<Option<i64>>(3)?,
                status: row.get::<String>(4)?,
                blocked_reason: row.get::<Option<String>>(5)?,
            });
        }
        Ok(out)
    }

    /// DORA-style metrics.
    pub async fn dora_metrics(&self) -> Result<DoraMetrics, DbError> {
        // Lead time
        let mut rows = self.conn.query(
            "SELECT AVG(rs.duration_secs)
             FROM runtime_state rs
             WHERE rs.completed_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days')
               AND rs.duration_secs IS NOT NULL",
            (),
        ).await?;
        let lead_time_secs: Option<f64> = match rows.next().await? {
            Some(row) => row.get::<Option<f64>>(0)?,
            None => None,
        };

        // Throughput
        let mut rows = self.conn.query(
            "SELECT CAST(COUNT(*) AS REAL) FROM runtime_state
             WHERE completed_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-28 days')
               AND completed_at IS NOT NULL",
            (),
        ).await?;
        let completed_28d: f64 = match rows.next().await? {
            Some(row) => row.get::<f64>(0).unwrap_or(0.0),
            None => 0.0,
        };

        // Change failure rate
        let mut rows = self.conn.query(
            "SELECT
                COALESCE(SUM(CASE WHEN event_type = 'task_completed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN event_type = 'task_failed' THEN 1 ELSE 0 END), 0)
             FROM events
             WHERE timestamp >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days')
               AND event_type IN ('task_completed', 'task_failed')",
            (),
        ).await?;
        let (completed_30d, failed_30d): (f64, f64) = match rows.next().await? {
            Some(row) => (
                row.get::<i64>(0).unwrap_or(0) as f64,
                row.get::<i64>(1).unwrap_or(0) as f64,
            ),
            None => (0.0, 0.0),
        };

        let change_failure_rate = if (completed_30d + failed_30d) > 0.0 {
            failed_30d / (completed_30d + failed_30d)
        } else {
            0.0
        };

        // TTR
        let mut rows = self.conn.query(
            "SELECT AVG(CAST(
                (julianday(rs.completed_at) - julianday(rs.claimed_at)) * 86400 AS REAL
             ))
             FROM runtime_state rs
             WHERE rs.blocked_reason IS NOT NULL
               AND rs.completed_at IS NOT NULL
               AND rs.claimed_at IS NOT NULL",
            (),
        ).await?;
        let ttr_secs: Option<f64> = match rows.next().await? {
            Some(row) => row.get::<Option<f64>>(0)?,
            None => None,
        };

        Ok(DoraMetrics {
            lead_time_hours: lead_time_secs.map(|s| s / 3600.0),
            throughput_per_week: completed_28d / 4.0,
            change_failure_rate,
            time_to_restore_hours: ttr_secs.map(|s| s / 3600.0),
        })
    }

    /// Per-domain duration statistics for completed tasks.
    pub async fn domain_duration_stats(&self) -> Result<Vec<DomainDurationStats>, DbError> {
        let mut rows = self.conn.query(
            "SELECT t.domain,
                    COUNT(*) AS cnt,
                    AVG(rs.duration_secs) AS avg_dur,
                    AVG(rs.duration_secs * rs.duration_secs) AS avg_sq
             FROM tasks t
             JOIN runtime_state rs ON rs.task_id = t.id
             WHERE t.status = 'done'
               AND rs.duration_secs IS NOT NULL
             GROUP BY t.domain",
            (),
        ).await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let avg: f64 = row.get::<f64>(2)?;
            let avg_sq: f64 = row.get::<f64>(3)?;
            let variance = (avg_sq - avg * avg).max(0.0);
            out.push(DomainDurationStats {
                domain: row.get::<String>(0)?,
                completed_count: row.get::<i64>(1)?,
                avg_duration_secs: avg,
                stddev_duration_secs: variance.sqrt(),
            });
        }
        Ok(out)
    }

    /// Generate monthly rollup.
    pub async fn generate_monthly_rollups(&self) -> Result<u64, DbError> {
        let n = self.conn.execute(
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
            (),
        ).await?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;
    use crate::repo::EventRepo;

    async fn setup() -> (libsql::Database, Connection) {
        let (db, conn) = open_memory_async().await.expect("in-memory db");
        conn.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test', 'Test Epic', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        conn.execute(
            "INSERT INTO tasks (id, epic_id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test.1', 'fn-1-test', 'Task 1', 'done', 't1.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        conn.execute(
            "INSERT INTO tasks (id, epic_id, title, status, file_path, created_at, updated_at)
             VALUES ('fn-1-test.2', 'fn-1-test', 'Task 2', 'in_progress', 't2.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        (db, conn)
    }

    #[tokio::test]
    async fn test_summary() {
        let (_db, conn) = setup().await;
        let stats = StatsQuery::new(conn);
        let s = stats.summary().await.unwrap();
        assert_eq!(s.total_epics, 1);
        assert_eq!(s.total_tasks, 2);
        assert_eq!(s.done_tasks, 1);
        assert_eq!(s.in_progress_tasks, 1);
    }

    #[tokio::test]
    async fn test_epic_stats() {
        let (_db, conn) = setup().await;
        let stats = StatsQuery::new(conn);
        let epics = stats.epic_stats(None).await.unwrap();
        assert_eq!(epics.len(), 1);
        assert_eq!(epics[0].task_count, 2);
        assert_eq!(epics[0].done_count, 1);

        let one = stats.epic_stats(Some("fn-1-test")).await.unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].epic_id, "fn-1-test");
    }

    #[tokio::test]
    async fn test_weekly_trends() {
        let (_db, conn) = setup().await;
        let repo = EventRepo::new(conn.clone());
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_started", None, None, None).await.unwrap();
        repo.insert("fn-1-test", Some("fn-1-test.1"), "task_completed", None, None, None).await.unwrap();

        let stats = StatsQuery::new(conn);
        let trends = stats.weekly_trends(4).await.unwrap();
        assert!(!trends.is_empty());
        assert!(trends[0].tasks_started > 0);
    }

    #[tokio::test]
    async fn test_token_breakdown() {
        let (_db, conn) = setup().await;
        conn.execute(
            "INSERT INTO token_usage (epic_id, task_id, model, input_tokens, output_tokens, estimated_cost)
             VALUES ('fn-1-test', 'fn-1-test.1', 'claude-sonnet-4-20250514', 1000, 500, 0.01)",
            (),
        ).await.unwrap();

        let stats = StatsQuery::new(conn);
        let tokens = stats.token_breakdown(None).await.unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].input_tokens, 1000);
        assert_eq!(tokens[0].output_tokens, 500);
    }

    #[tokio::test]
    async fn test_bottlenecks() {
        let (_db, conn) = setup().await;
        conn.execute(
            "INSERT INTO runtime_state (task_id, duration_secs) VALUES ('fn-1-test.1', 3600)",
            (),
        ).await.unwrap();

        let stats = StatsQuery::new(conn);
        let bottlenecks = stats.bottlenecks(10).await.unwrap();
        assert!(!bottlenecks.is_empty());
        assert_eq!(bottlenecks[0].task_id, "fn-1-test.1");
    }

    #[tokio::test]
    async fn test_dora_metrics() {
        let (_db, conn) = setup().await;
        let stats = StatsQuery::new(conn);
        let dora = stats.dora_metrics().await.unwrap();
        assert_eq!(dora.throughput_per_week, 0.0);
        assert_eq!(dora.change_failure_rate, 0.0);
    }
}
