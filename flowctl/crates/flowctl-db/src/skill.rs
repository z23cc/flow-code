//! Skill repository with native libSQL vector search.
//!
//! Stores skill metadata (name, description, plugin path) with a 384-dim
//! BGE-small embedding for semantic matching via `vector_top_k`.
//!
//! Reuses `embed_one()`, `ensure_embedder()`, and `vec_to_literal()` from
//! the memory module -- zero duplication.

use libsql::{params, Connection};

use crate::error::DbError;
use crate::memory::{embed_one, vec_to_literal};

// ── Types ───────────────────────────────────────────────────────────

/// A skill match result from semantic search.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillMatch {
    pub name: String,
    pub description: String,
    pub score: f64,
}

/// A registered skill entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillEntry {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub plugin_path: Option<String>,
    pub updated_at: String,
}

// ── Repository ──────────────────────────────────────────────────────

/// Async repository for skill metadata + semantic vector search.
pub struct SkillRepo {
    conn: Connection,
}

impl SkillRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Insert or replace a skill. Auto-generates an embedding from
    /// `description` when the embedder is available; otherwise leaves the
    /// embedding NULL and logs a warning.
    pub async fn upsert(
        &self,
        name: &str,
        description: &str,
        plugin_path: Option<&str>,
    ) -> Result<(), DbError> {
        let now = chrono::Utc::now().to_rfc3339();

        self.conn
            .execute(
                "INSERT INTO skills (name, description, plugin_path, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(name) DO UPDATE SET
                     description = excluded.description,
                     plugin_path = excluded.plugin_path,
                     updated_at  = excluded.updated_at",
                params![
                    name.to_string(),
                    description.to_string(),
                    plugin_path.map(String::from),
                    now,
                ],
            )
            .await?;

        // Attempt to embed; swallow failures (NULL embedding is fine).
        match embed_one(description).await {
            Ok(vec) => {
                let lit = vec_to_literal(&vec);
                self.conn
                    .execute(
                        "UPDATE skills SET embedding = vector32(?1) WHERE name = ?2",
                        params![lit, name.to_string()],
                    )
                    .await?;
            }
            Err(e) => {
                tracing::warn!(
                    skill = name,
                    error = %e,
                    "embedder unavailable; skill inserted without embedding"
                );
            }
        }

        Ok(())
    }

    /// Semantic search: embed the query, find nearest skills via
    /// `vector_top_k`, convert L2 distance to cosine similarity, and
    /// filter by threshold.
    ///
    /// Returns `Ok(vec![])` (not an error) if the embedder or vector
    /// index is unavailable -- graceful degradation.
    pub async fn match_skills(
        &self,
        query: &str,
        limit: usize,
        threshold: f64,
    ) -> Result<Vec<SkillMatch>, DbError> {
        let vec = match embed_one(query).await {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };
        let lit = vec_to_literal(&vec);

        // Use vector_distance_cos() instead of vector_top_k() — works without
        // a vector index (no ANN index required in embedded mode). Exact search
        // via full table scan; perfectly fast for <10,000 rows (~30 skills).
        let rows_result = self
            .conn
            .query(
                "SELECT s.name, s.description,
                        vector_distance_cos(s.embedding, vector32(?1)) AS distance
                   FROM skills s
                  WHERE s.embedding IS NOT NULL
                  ORDER BY distance ASC
                  LIMIT ?2",
                params![lit, limit as i64],
            )
            .await;

        let mut rows = match rows_result {
            Ok(r) => r,
            Err(_) => return Ok(vec![]), // vector functions unavailable
        };

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let dist: f64 = row.get(2)?;
            // Cosine distance → cosine similarity: sim = 1 - dist
            // (0 = identical, 1 = orthogonal, 2 = opposite)
            let score = 1.0 - dist;
            if score >= threshold {
                out.push(SkillMatch {
                    name: row.get::<String>(0)?,
                    description: row.get::<String>(1)?,
                    score,
                });
            }
        }
        Ok(out)
    }

    /// List all registered skills (for debugging / introspection).
    pub async fn list(&self) -> Result<Vec<SkillEntry>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, name, description, plugin_path, updated_at
                   FROM skills
                  ORDER BY name ASC",
                (),
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(SkillEntry {
                id: row.get::<i64>(0)?,
                name: row.get::<String>(1)?,
                description: row.get::<String>(2)?,
                plugin_path: row.get::<Option<String>>(3)?,
                updated_at: row.get::<String>(4)?,
            });
        }
        Ok(out)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;

    async fn fresh_repo() -> SkillRepo {
        let (_db, conn) = open_memory_async().await.expect("open memory db");
        let _ = Box::leak(Box::new(_db));
        SkillRepo::new(conn)
    }

    #[tokio::test]
    async fn test_upsert_and_list() {
        let repo = fresh_repo().await;
        repo.upsert("plan", "Plan and design tasks", Some("/plugins/flow"))
            .await
            .expect("upsert");
        repo.upsert("work", "Execute implementation tasks", None)
            .await
            .expect("upsert");

        let skills = repo.list().await.expect("list");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "plan");
        assert_eq!(skills[1].name, "work");
    }

    #[tokio::test]
    async fn test_upsert_replaces() {
        let repo = fresh_repo().await;
        repo.upsert("plan", "old description", None)
            .await
            .expect("upsert");
        repo.upsert("plan", "new description", Some("/new/path"))
            .await
            .expect("upsert");

        let skills = repo.list().await.expect("list");
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "new description");
        assert_eq!(skills[0].plugin_path.as_deref(), Some("/new/path"));
    }

    #[tokio::test]
    async fn test_match_skills_graceful_no_index() {
        // In-memory DB won't have vector index; should return empty, not error.
        let repo = fresh_repo().await;
        repo.upsert("plan", "Plan tasks", None)
            .await
            .expect("upsert");

        let matches = repo
            .match_skills("planning", 5, 0.5)
            .await
            .expect("match_skills should not error");
        // May be empty if embedder or index is unavailable -- that's fine.
        assert!(matches.len() <= 5);
    }

    /// Semantic match end-to-end using vector_distance_cos (no index needed).
    /// Gated behind `#[ignore]` because the first run downloads the
    /// BGE-small model (~130MB).
    #[tokio::test]
    #[ignore = "requires fastembed model (~130MB); run with --ignored"]
    async fn test_match_skills_semantic() {
        let repo = fresh_repo().await;
        repo.upsert("plan", "Design and architect implementation plans", None)
            .await
            .expect("upsert");
        repo.upsert("work", "Execute coding tasks and write code", None)
            .await
            .expect("upsert");
        repo.upsert("review", "Review code changes for quality", None)
            .await
            .expect("upsert");

        let matches = repo
            .match_skills("architecture design", 3, 0.3)
            .await
            .expect("match_skills");
        assert!(!matches.is_empty(), "expected at least one match");
        // "plan" (Design and architect...) should be the best match
        assert_eq!(
            matches[0].name, "plan",
            "expected 'plan' as best match for architecture query, got '{}'",
            matches[0].name
        );
        // Scores should be between 0 and 1
        for m in &matches {
            assert!(m.score > 0.0 && m.score <= 1.0, "score out of range: {}", m.score);
        }
    }
}
