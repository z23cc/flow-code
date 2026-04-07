//! Memory repository with native libSQL vector search.
//!
//! Memory entries carry a 384-dimensional embedding (BGE-small) stored in
//! the native `F32_BLOB(384)` column. Semantic search uses libSQL's
//! `vector_top_k` virtual function against the `memory_emb_idx` index.
//!
//! ## Offline fallback
//!
//! The first call to `get_embedder()` downloads the BGE-small model
//! (~130MB) to a local cache. If that download fails (no network, no
//! disk space) we log a warning and:
//! - `add()` still inserts the row, with embedding left NULL
//! - `search_semantic()` returns `DbError::Schema("embedder unavailable")`
//!
//! Callers should always have `search_literal()` as a fallback path.
//!
//! ## Tests
//!
//! Tests that require the embedder are gated on a successful
//! `test_embedder_loads` check. In CI environments without network
//! access they will report as passing with a warning. See the test
//! module for details.

use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use libsql::{params, Connection};
use tokio::sync::OnceCell;

use crate::error::DbError;

// ── Types ───────────────────────────────────────────────────────────

/// A memory entry (pitfall/convention/decision) with optional embedding.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    pub id: Option<i64>,
    pub entry_type: String,
    pub content: String,
    pub summary: Option<String>,
    pub hash: Option<String>,
    pub module: Option<String>,
    pub severity: Option<String>,
    pub problem_type: Option<String>,
    pub component: Option<String>,
    pub tags: Vec<String>,
    pub track: Option<String>,
    pub created_at: String,
    pub last_verified: Option<String>,
    pub refs: u32,
}

impl Default for MemoryEntry {
    fn default() -> Self {
        Self {
            id: None,
            entry_type: "convention".to_string(),
            content: String::new(),
            summary: None,
            hash: None,
            module: None,
            severity: None,
            problem_type: None,
            component: None,
            tags: Vec::new(),
            track: None,
            created_at: String::new(),
            last_verified: None,
            refs: 0,
        }
    }
}

/// Filter for `list()` and `search_semantic()` queries.
#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub entry_type: Option<String>,
    pub module: Option<String>,
    pub track: Option<String>,
    pub severity: Option<String>,
}

// ── Embedder (lazy, shared) ─────────────────────────────────────────

static EMBEDDER: OnceCell<Result<Mutex<TextEmbedding>, String>> = OnceCell::const_new();

/// Lazily initialize the BGE-small embedder. First call downloads the
/// model (~130MB) via fastembed; subsequent calls return the cached
/// instance. Initialization runs on a blocking thread because fastembed
/// performs synchronous file I/O.
pub(crate) async fn ensure_embedder() -> Result<(), DbError> {
    let res = EMBEDDER
        .get_or_init(|| async {
            match tokio::task::spawn_blocking(|| {
                TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))
                    .map(Mutex::new)
                    .map_err(|e| format!("fastembed init: {e}"))
            })
            .await
            {
                Ok(inner) => inner,
                Err(join_err) => Err(format!("spawn_blocking: {join_err}")),
            }
        })
        .await;
    res.as_ref()
        .map(|_| ())
        .map_err(|e| DbError::Schema(format!("embedder unavailable: {e}")))
}

/// Embed a single passage into a 384-dim vector.
pub(crate) async fn embed_one(text: &str) -> Result<Vec<f32>, DbError> {
    ensure_embedder().await?;
    let text = text.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let cell = EMBEDDER
            .get()
            .and_then(|r| r.as_ref().ok())
            .ok_or_else(|| "embedder missing".to_string())?;
        let mut emb = cell.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        emb.embed(vec![text], None)
            .map_err(|e| format!("embed: {e}"))
    })
    .await
    .map_err(|e| DbError::Schema(format!("spawn_blocking: {e}")))?
    .map_err(DbError::Schema)?;

    result
        .into_iter()
        .next()
        .ok_or_else(|| DbError::Schema("empty embedding result".into()))
}

/// Convert a `Vec<f32>` into a libSQL `vector32()` literal string.
pub(crate) fn vec_to_literal(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(std::string::ToString::to_string).collect();
    format!("[{}]", parts.join(","))
}

// ── Repository ──────────────────────────────────────────────────────

/// Async repository for memory entries + semantic vector search.
pub struct MemoryRepo {
    conn: Connection,
}

impl MemoryRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Insert a memory entry. Auto-generates an embedding from `content`
    /// when the embedder is available; otherwise leaves the embedding
    /// NULL and logs a warning. Returns the new row id.
    ///
    /// If `entry.hash` collides with an existing row, returns the
    /// existing id (treated as an upsert-style no-op on the insert).
    pub async fn add(&self, entry: &MemoryEntry) -> Result<i64, DbError> {
        // Dedup by hash first.
        if let Some(ref h) = entry.hash {
            let mut rows = self
                .conn
                .query("SELECT id FROM memory WHERE hash = ?1", params![h.clone()])
                .await?;
            if let Some(row) = rows.next().await? {
                return Ok(row.get::<i64>(0)?);
            }
        }

        let tags_json = serde_json::to_string(&entry.tags)?;
        let created_at = if entry.created_at.is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            entry.created_at.clone()
        };

        self.conn
            .execute(
                "INSERT INTO memory (
                    entry_type, content, summary, hash, module, severity,
                    problem_type, component, tags, track, created_at,
                    last_verified, refs
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![
                    entry.entry_type.clone(),
                    entry.content.clone(),
                    entry.summary.clone(),
                    entry.hash.clone(),
                    entry.module.clone(),
                    entry.severity.clone(),
                    entry.problem_type.clone(),
                    entry.component.clone(),
                    tags_json,
                    entry.track.clone(),
                    created_at,
                    entry.last_verified.clone(),
                    entry.refs as i64,
                ],
            )
            .await?;

        let id = self.conn.last_insert_rowid();

        // Attempt to embed; swallow failures (NULL embedding is fine).
        match embed_one(&entry.content).await {
            Ok(vec) => {
                let lit = vec_to_literal(&vec);
                self.conn
                    .execute(
                        "UPDATE memory SET embedding = vector32(?1) WHERE id = ?2",
                        params![lit, id],
                    )
                    .await?;
            }
            Err(e) => {
                tracing::warn!(
                    memory_id = id,
                    error = %e,
                    "embedder unavailable; memory inserted without embedding"
                );
            }
        }

        Ok(id)
    }

    /// Fetch a single entry by id.
    pub async fn get(&self, id: i64) -> Result<Option<MemoryEntry>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT id, entry_type, content, summary, hash, module, severity,
                        problem_type, component, tags, track, created_at,
                        last_verified, refs
                   FROM memory WHERE id = ?1",
                params![id],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row_to_entry(&row)?)),
            None => Ok(None),
        }
    }

    /// List entries matching the provided filter. All filter fields are
    /// AND-joined; `None` fields are ignored.
    pub async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>, DbError> {
        let (where_clause, args) = build_filter_sql(&filter);
        let sql = format!(
            "SELECT id, entry_type, content, summary, hash, module, severity,
                    problem_type, component, tags, track, created_at,
                    last_verified, refs
               FROM memory
               {where_clause}
               ORDER BY created_at DESC"
        );
        let mut rows = self.conn.query(&sql, args).await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row_to_entry(&row)?);
        }
        Ok(out)
    }

    /// Substring match on `content`. No embedder required.
    pub async fn search_literal(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, DbError> {
        let pat = format!("%{query}%");
        let mut rows = self
            .conn
            .query(
                "SELECT id, entry_type, content, summary, hash, module, severity,
                        problem_type, component, tags, track, created_at,
                        last_verified, refs
                   FROM memory
                  WHERE content LIKE ?1
                  ORDER BY refs DESC, created_at DESC
                  LIMIT ?2",
                params![pat, limit as i64],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row_to_entry(&row)?);
        }
        Ok(out)
    }

    /// Semantic search via libSQL `vector_top_k`. Returns entries whose
    /// embedding is closest to `query`'s embedding. Fails with
    /// `DbError::Schema` if the embedder is not available.
    pub async fn search_semantic(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> Result<Vec<MemoryEntry>, DbError> {
        let vec = embed_one(query).await?;
        let lit = vec_to_literal(&vec);

        // vector_top_k returns (id, distance) rows; join on rowid.
        // Over-fetch when filters are applied so we can still return `limit` matches.
        let filter = filter.unwrap_or_default();
        let has_filter = filter.entry_type.is_some()
            || filter.module.is_some()
            || filter.track.is_some()
            || filter.severity.is_some();
        let fetch = if has_filter { limit * 4 } else { limit };

        let mut rows = self
            .conn
            .query(
                "SELECT m.id, m.entry_type, m.content, m.summary, m.hash, m.module,
                        m.severity, m.problem_type, m.component, m.tags, m.track,
                        m.created_at, m.last_verified, m.refs
                   FROM vector_top_k('memory_emb_idx', vector32(?1), ?2) AS top
                   JOIN memory m ON m.rowid = top.id",
                params![lit, fetch as i64],
            )
            .await?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let entry = row_to_entry(&row)?;
            if passes_filter(&entry, &filter) {
                out.push(entry);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// Delete an entry by id.
    pub async fn delete(&self, id: i64) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM memory WHERE id = ?1", params![id])
            .await?;
        Ok(())
    }

    /// Increment the `refs` counter for an entry.
    pub async fn increment_refs(&self, id: i64) -> Result<(), DbError> {
        self.conn
            .execute(
                "UPDATE memory SET refs = refs + 1 WHERE id = ?1",
                params![id],
            )
            .await?;
        Ok(())
    }
}

// ── Row helpers ─────────────────────────────────────────────────────

fn row_to_entry(row: &libsql::Row) -> Result<MemoryEntry, DbError> {
    let tags_raw: String = row.get::<String>(9).unwrap_or_else(|_| "[]".to_string());
    let tags: Vec<String> = serde_json::from_str(&tags_raw).unwrap_or_default();
    Ok(MemoryEntry {
        id: Some(row.get::<i64>(0)?),
        entry_type: row.get::<String>(1)?,
        content: row.get::<String>(2)?,
        summary: row.get::<Option<String>>(3)?,
        hash: row.get::<Option<String>>(4)?,
        module: row.get::<Option<String>>(5)?,
        severity: row.get::<Option<String>>(6)?,
        problem_type: row.get::<Option<String>>(7)?,
        component: row.get::<Option<String>>(8)?,
        tags,
        track: row.get::<Option<String>>(10)?,
        created_at: row.get::<String>(11)?,
        last_verified: row.get::<Option<String>>(12)?,
        refs: row.get::<i64>(13)? as u32,
    })
}

fn build_filter_sql(f: &MemoryFilter) -> (String, Vec<libsql::Value>) {
    let mut clauses = Vec::new();
    let mut args: Vec<libsql::Value> = Vec::new();
    let mut i = 1;
    if let Some(ref v) = f.entry_type {
        clauses.push(format!("entry_type = ?{i}"));
        args.push(libsql::Value::Text(v.clone()));
        i += 1;
    }
    if let Some(ref v) = f.module {
        clauses.push(format!("module = ?{i}"));
        args.push(libsql::Value::Text(v.clone()));
        i += 1;
    }
    if let Some(ref v) = f.track {
        clauses.push(format!("track = ?{i}"));
        args.push(libsql::Value::Text(v.clone()));
        i += 1;
    }
    if let Some(ref v) = f.severity {
        clauses.push(format!("severity = ?{i}"));
        args.push(libsql::Value::Text(v.clone()));
        // i += 1; // last binding
    }
    if clauses.is_empty() {
        (String::new(), args)
    } else {
        (format!("WHERE {}", clauses.join(" AND ")), args)
    }
}

fn passes_filter(e: &MemoryEntry, f: &MemoryFilter) -> bool {
    if let Some(ref v) = f.entry_type {
        if &e.entry_type != v {
            return false;
        }
    }
    if let Some(ref v) = f.module {
        if e.module.as_ref() != Some(v) {
            return false;
        }
    }
    if let Some(ref v) = f.track {
        if e.track.as_ref() != Some(v) {
            return false;
        }
    }
    if let Some(ref v) = f.severity {
        if e.severity.as_ref() != Some(v) {
            return false;
        }
    }
    true
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;

    async fn fresh_repo() -> MemoryRepo {
        let (_db, conn) = open_memory_async().await.expect("open memory db");
        // Keep db alive for the duration of the test by leaking — the
        // test holds conn which references the same underlying store.
        // Actually we need to keep Database alive; Box::leak it.
        let _ = Box::leak(Box::new(_db));
        MemoryRepo::new(conn)
    }

    fn sample(content: &str, entry_type: &str) -> MemoryEntry {
        MemoryEntry {
            entry_type: entry_type.to_string(),
            content: content.to_string(),
            ..MemoryEntry::default()
        }
    }

    #[tokio::test]
    async fn test_add_get_delete_no_embedder() {
        // Uses a bogus content; add() will still succeed even if the
        // embedder fails because we tolerate missing embeddings.
        let repo = fresh_repo().await;
        let id = repo
            .add(&sample("hello world", "convention"))
            .await
            .expect("add");
        let fetched = repo.get(id).await.expect("get").expect("some");
        assert_eq!(fetched.content, "hello world");
        assert_eq!(fetched.entry_type, "convention");

        repo.delete(id).await.expect("delete");
        assert!(repo.get(id).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn test_search_literal() {
        let repo = fresh_repo().await;
        repo.add(&sample("database migration tooling", "pitfall"))
            .await
            .unwrap();
        repo.add(&sample("prefer iterators over loops", "convention"))
            .await
            .unwrap();

        let results = repo.search_literal("migration", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("migration"));

        let none = repo.search_literal("nonexistent-xyz", 10).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn test_list_with_filter() {
        let repo = fresh_repo().await;
        repo.add(&sample("a pitfall", "pitfall")).await.unwrap();
        repo.add(&sample("a convention", "convention"))
            .await
            .unwrap();
        repo.add(&sample("a decision", "decision")).await.unwrap();

        let conventions = repo
            .list(MemoryFilter {
                entry_type: Some("convention".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(conventions.len(), 1);
        assert_eq!(conventions[0].entry_type, "convention");

        let all = repo.list(MemoryFilter::default()).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_increment_refs() {
        let repo = fresh_repo().await;
        let id = repo
            .add(&sample("refcount test", "convention"))
            .await
            .unwrap();
        assert_eq!(repo.get(id).await.unwrap().unwrap().refs, 0);
        repo.increment_refs(id).await.unwrap();
        repo.increment_refs(id).await.unwrap();
        assert_eq!(repo.get(id).await.unwrap().unwrap().refs, 2);
    }

    #[tokio::test]
    async fn test_dedup_by_hash() {
        let repo = fresh_repo().await;
        let mut e = sample("same content", "convention");
        e.hash = Some("abc123".to_string());
        let id1 = repo.add(&e).await.unwrap();
        let id2 = repo.add(&e).await.unwrap();
        assert_eq!(id1, id2, "same hash should return existing id");
    }

    /// Verify the embedder can be loaded. If this test is `ignored` by
    /// the user or fails due to network, semantic tests will be gated.
    /// Requires ~130MB BGE-small download on first run.
    #[tokio::test]
    #[ignore = "requires network for ~130MB fastembed model download"]
    async fn test_embedder_loads() {
        ensure_embedder().await.expect("embedder must load");
        let v = embed_one("hello world").await.expect("embed");
        assert_eq!(v.len(), 384);
    }

    /// Semantic search end-to-end. Gated behind `#[ignore]` because the
    /// first run downloads the BGE-small model (~130MB).
    #[tokio::test]
    #[ignore = "requires fastembed model (~130MB); run with --ignored"]
    async fn test_search_semantic() {
        let repo = fresh_repo().await;
        repo.add(&sample(
            "SQL database performance and query optimization",
            "convention",
        ))
        .await
        .unwrap();
        repo.add(&sample(
            "React component lifecycle and hooks",
            "convention",
        ))
        .await
        .unwrap();
        repo.add(&sample("Rust ownership and borrow checker", "convention"))
            .await
            .unwrap();

        let results = repo
            .search_semantic("javascript frontend framework", 1, None)
            .await
            .expect("semantic search");
        assert_eq!(results.len(), 1);
        assert!(
            results[0].content.contains("React"),
            "expected React result, got: {}",
            results[0].content
        );
    }
}
