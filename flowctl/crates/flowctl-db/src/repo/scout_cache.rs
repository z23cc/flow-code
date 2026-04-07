//! Async repository for scout result caching.
//!
//! Caches scout results keyed by `{scout_type}:{commit_hash}` with a
//! configurable TTL (default 24h). Auto-evicts expired entries on `set`.

use chrono::{Duration, Utc};
use libsql::{params, Connection};

use crate::error::DbError;

/// Default cache TTL: 24 hours.
const CACHE_TTL_HOURS: i64 = 24;

/// Reduced TTL for git-less fallback: 1 hour.
const NO_GIT_TTL_HOURS: i64 = 1;

/// Async repository for scout result caching.
pub struct ScoutCacheRepo {
    conn: Connection,
}

impl ScoutCacheRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Get a cached scout result. Returns `None` if miss (not found or expired).
    ///
    /// TTL: 24h for normal keys, 1h for `*:no-git` keys.
    pub async fn get(&self, key: &str) -> Result<Option<String>, DbError> {
        let ttl_hours = if key.ends_with(":no-git") {
            NO_GIT_TTL_HOURS
        } else {
            CACHE_TTL_HOURS
        };

        let cutoff = (Utc::now() - Duration::hours(ttl_hours)).format("%Y-%m-%d %H:%M:%S").to_string();

        let mut rows = self
            .conn
            .query(
                "SELECT result FROM scout_cache WHERE key = ?1 AND created_at >= ?2",
                params![key.to_string(), cutoff],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            let result: String = row.get(0)?;
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    /// Set a cached scout result. Auto-evicts expired entries first, then upserts.
    pub async fn set(
        &self,
        key: &str,
        commit_hash: &str,
        scout_type: &str,
        result: &str,
    ) -> Result<(), DbError> {
        // Evict expired entries (older than 24h).
        let cutoff = (Utc::now() - Duration::hours(CACHE_TTL_HOURS)).format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn
            .execute(
                "DELETE FROM scout_cache WHERE created_at < ?1",
                params![cutoff],
            )
            .await?;

        // Upsert the new entry.
        self.conn
            .execute(
                "INSERT INTO scout_cache (key, commit_hash, scout_type, result, created_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET
                     commit_hash = excluded.commit_hash,
                     result = excluded.result,
                     created_at = excluded.created_at",
                params![
                    key.to_string(),
                    commit_hash.to_string(),
                    scout_type.to_string(),
                    result.to_string(),
                ],
            )
            .await?;

        Ok(())
    }

    /// Clear all cached scout results.
    pub async fn clear(&self) -> Result<u64, DbError> {
        let n = self.conn.execute("DELETE FROM scout_cache", ()).await?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;

    #[tokio::test]
    async fn test_cache_hit() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn);

        repo.set("repo-scout:abc123", "abc123", "repo-scout", r#"{"findings":[]}"#)
            .await
            .unwrap();

        let result = repo.get("repo-scout:abc123").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"findings":[]}"#);
    }

    #[tokio::test]
    async fn test_cache_miss_commit() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn);

        repo.set("repo-scout:abc123", "abc123", "repo-scout", r#"{"findings":[]}"#)
            .await
            .unwrap();

        // Different commit hash → different key → miss.
        let result = repo.get("repo-scout:def456").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_miss_ttl() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn.clone());

        // Insert with a past timestamp to simulate expiry.
        let past = (Utc::now() - Duration::hours(25)).format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO scout_cache (key, commit_hash, scout_type, result, created_at)
             VALUES ('repo-scout:old', 'old', 'repo-scout', '{}', ?1)",
            params![past],
        )
        .await
        .unwrap();

        let result = repo.get("repo-scout:old").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_auto_eviction() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn.clone());

        // Insert an expired entry.
        let past = (Utc::now() - Duration::hours(25)).format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO scout_cache (key, commit_hash, scout_type, result, created_at)
             VALUES ('old-scout:expired', 'expired', 'old-scout', '{}', ?1)",
            params![past],
        )
        .await
        .unwrap();

        // Set a new entry — should evict the expired one.
        repo.set("repo-scout:new", "new", "repo-scout", r#"{"data":"fresh"}"#)
            .await
            .unwrap();

        // Verify expired entry is gone.
        let mut rows = conn
            .query("SELECT key FROM scout_cache WHERE key = 'old-scout:expired'", ())
            .await
            .unwrap();
        assert!(rows.next().await.unwrap().is_none());

        // Verify new entry exists.
        let result = repo.get("repo-scout:new").await.unwrap();
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_upsert() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn.clone());

        repo.set("repo-scout:abc", "abc", "repo-scout", "v1")
            .await
            .unwrap();

        repo.set("repo-scout:abc", "abc", "repo-scout", "v2")
            .await
            .unwrap();

        let result = repo.get("repo-scout:abc").await.unwrap();
        assert_eq!(result.unwrap(), "v2");

        // Verify only one row exists.
        let mut rows = conn
            .query("SELECT COUNT(*) FROM scout_cache WHERE key = 'repo-scout:abc'", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_clear() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = ScoutCacheRepo::new(conn);

        repo.set("a:1", "1", "a", "data1").await.unwrap();
        repo.set("b:2", "2", "b", "data2").await.unwrap();

        let n = repo.clear().await.unwrap();
        assert_eq!(n, 2);

        let result = repo.get("a:1").await.unwrap();
        assert!(result.is_none());
    }
}
