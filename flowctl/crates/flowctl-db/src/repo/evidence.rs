//! Async repository for task completion evidence.

use libsql::{params, Connection};

use flowctl_core::types::Evidence;

use crate::error::DbError;

/// Async repository for task completion evidence.
pub struct EvidenceRepo {
    conn: Connection,
}

impl EvidenceRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Upsert evidence for a task. Commits and tests are stored as JSON arrays.
    pub async fn upsert(&self, task_id: &str, evidence: &Evidence) -> Result<(), DbError> {
        let commits_json = if evidence.commits.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&evidence.commits)?)
        };
        let tests_json = if evidence.tests.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&evidence.tests)?)
        };

        self.conn
            .execute(
                "INSERT INTO evidence (task_id, commits, tests, files_changed, insertions, deletions, review_iters)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(task_id) DO UPDATE SET
                     commits = excluded.commits,
                     tests = excluded.tests,
                     files_changed = excluded.files_changed,
                     insertions = excluded.insertions,
                     deletions = excluded.deletions,
                     review_iters = excluded.review_iters",
                params![
                    task_id.to_string(),
                    commits_json,
                    tests_json,
                    evidence.files_changed.map(|v| v as i64),
                    evidence.insertions.map(|v| v as i64),
                    evidence.deletions.map(|v| v as i64),
                    evidence.review_iterations.map(|v| v as i64),
                ],
            )
            .await?;
        Ok(())
    }

    /// Get evidence for a task.
    pub async fn get(&self, task_id: &str) -> Result<Option<Evidence>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT commits, tests, files_changed, insertions, deletions, review_iters
                 FROM evidence WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;

        let Some(row) = rows.next().await? else {
            return Ok(None);
        };

        let commits_json: Option<String> = row.get::<Option<String>>(0)?;
        let tests_json: Option<String> = row.get::<Option<String>>(1)?;
        let files_changed: Option<i64> = row.get::<Option<i64>>(2)?;
        let insertions: Option<i64> = row.get::<Option<i64>>(3)?;
        let deletions: Option<i64> = row.get::<Option<i64>>(4)?;
        let review_iters: Option<i64> = row.get::<Option<i64>>(5)?;

        let commits: Vec<String> = commits_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();
        let tests: Vec<String> = tests_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or_default();

        Ok(Some(Evidence {
            commits,
            tests,
            prs: Vec::new(),
            files_changed: files_changed.map(|v| v as u32),
            insertions: insertions.map(|v| v as u32),
            deletions: deletions.map(|v| v as u32),
            review_iterations: review_iters.map(|v| v as u32),
            workspace_changes: None,
        }))
    }
}
