//! Schema migration infrastructure for flowctl-db.
//!
//! Tracks schema version in a `_meta` table and runs numbered migrations
//! sequentially. Migrations are idempotent (safe to re-run).

use libsql::Connection;

use crate::error::DbError;

/// Current target schema version. Bump this when adding new migrations.
const TARGET_VERSION: i64 = 5;

/// Ensure `_meta` table exists and run any pending migrations.
pub async fn migrate(conn: &Connection) -> Result<(), DbError> {
    // Create the _meta table if it doesn't exist.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS _meta (key TEXT PRIMARY KEY, value TEXT)",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("_meta table creation failed: {e}")))?;

    let current = get_version(conn).await?;

    if current < 2 {
        migrate_v2(conn).await?;
    }

    if current < 3 {
        migrate_v3(conn).await?;
    }

    if current < 4 {
        migrate_v4(conn).await?;
    }

    if current < 5 {
        migrate_v5(conn).await?;
    }

    // Update stored version to target.
    if current < TARGET_VERSION {
        set_version(conn, TARGET_VERSION).await?;
    }

    Ok(())
}

/// Read current schema version from `_meta`. Returns 1 if no version is set
/// (meaning the DB has the original schema but no migration history).
async fn get_version(conn: &Connection) -> Result<i64, DbError> {
    let mut rows = conn
        .query(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            (),
        )
        .await
        .map_err(|e| DbError::Schema(format!("_meta query failed: {e}")))?;

    if let Some(row) = rows
        .next()
        .await
        .map_err(|e| DbError::Schema(format!("_meta row read failed: {e}")))?
    {
        let val: String = row
            .get(0)
            .map_err(|e| DbError::Schema(format!("_meta value read failed: {e}")))?;
        val.parse::<i64>()
            .map_err(|e| DbError::Schema(format!("_meta version parse failed: {e}")))
    } else {
        // No version stored yet — this is a v1 database (original schema).
        Ok(1)
    }
}

/// Write schema version to `_meta`.
async fn set_version(conn: &Connection, version: i64) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        libsql::params![version.to_string()],
    )
    .await
    .map_err(|e| DbError::Schema(format!("_meta version update failed: {e}")))?;
    Ok(())
}

/// Migration v2: Add TTL columns to file_locks.
///
/// - `holder_pid INTEGER` — PID of the process holding the lock
/// - `expires_at TEXT` — ISO-8601 expiry timestamp for TTL-based cleanup
///
/// Uses `.ok()` on each ALTER TABLE because the column may already exist
/// on re-run (ALTER TABLE ADD COLUMN is not idempotent in SQLite/libSQL).
async fn migrate_v2(conn: &Connection) -> Result<(), DbError> {
    let _ = conn
        .execute(
            "ALTER TABLE file_locks ADD COLUMN holder_pid INTEGER",
            (),
        )
        .await
        .ok();

    let _ = conn
        .execute(
            "ALTER TABLE file_locks ADD COLUMN expires_at TEXT",
            (),
        )
        .await
        .ok();

    Ok(())
}

/// Migration v3: Change file_locks PK to composite (file_path, task_id)
/// and add `lock_mode TEXT DEFAULT 'write'`.
///
/// SQLite can't ALTER PRIMARY KEY, so we recreate the table.
async fn migrate_v3(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_locks_new (
            file_path  TEXT NOT NULL,
            task_id    TEXT NOT NULL,
            locked_at  TEXT NOT NULL,
            holder_pid INTEGER,
            expires_at TEXT,
            lock_mode  TEXT NOT NULL DEFAULT 'write',
            PRIMARY KEY (file_path, task_id)
        )",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("file_locks_new creation failed: {e}")))?;

    // Copy existing data (add default lock_mode for old rows).
    conn.execute(
        "INSERT OR IGNORE INTO file_locks_new (file_path, task_id, locked_at, holder_pid, expires_at, lock_mode)
         SELECT file_path, task_id, locked_at, holder_pid, expires_at, COALESCE(lock_mode, 'write')
         FROM file_locks",
        (),
    )
    .await
    .ok(); // May fail if file_locks doesn't have lock_mode column yet — that's fine.

    conn.execute("DROP TABLE IF EXISTS file_locks", ())
        .await
        .map_err(|e| DbError::Schema(format!("file_locks drop failed: {e}")))?;

    conn.execute(
        "ALTER TABLE file_locks_new RENAME TO file_locks",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("file_locks rename failed: {e}")))?;

    Ok(())
}

/// Migration v4: Add scout_cache table for caching scout results.
async fn migrate_v4(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS scout_cache (
            key TEXT PRIMARY KEY,
            commit_hash TEXT NOT NULL,
            scout_type TEXT NOT NULL,
            result TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("scout_cache creation failed: {e}")))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_scout_cache_type ON scout_cache(scout_type)",
        (),
    )
    .await
    .ok();

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_scout_cache_created ON scout_cache(created_at)",
        (),
    )
    .await
    .ok();

    Ok(())
}

/// Migration v5: Add event_store and pipeline_progress tables for event sourcing.
///
/// These tables are created in `schema.sql` for fresh databases; this migration
/// adds them to databases created before v5.
async fn migrate_v5(conn: &Connection) -> Result<(), DbError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS event_store (
            event_id    INTEGER PRIMARY KEY AUTOINCREMENT,
            stream_id   TEXT NOT NULL,
            version     INTEGER NOT NULL,
            event_type  TEXT NOT NULL,
            payload     TEXT NOT NULL,
            metadata    TEXT,
            created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
        )",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("event_store creation failed: {e}")))?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_event_store_stream_version
            ON event_store(stream_id, version)",
        (),
    )
    .await
    .ok();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS pipeline_progress (
            epic_id     TEXT PRIMARY KEY,
            phase       TEXT NOT NULL DEFAULT 'plan',
            started_at  TEXT,
            updated_at  TEXT
        )",
        (),
    )
    .await
    .map_err(|e| DbError::Schema(format!("pipeline_progress creation failed: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool;

    #[tokio::test]
    async fn test_migrate_fresh_db() {
        let (_db, conn) = pool::open_memory_async().await.unwrap();

        // Verify _meta table exists and version is set.
        let version = get_version(&conn).await.unwrap();
        assert_eq!(version, TARGET_VERSION, "version should be {TARGET_VERSION} after open");

        // Verify file_locks has the new columns.
        let mut rows = conn
            .query("SELECT name FROM pragma_table_info('file_locks')", ())
            .await
            .unwrap();

        let mut cols: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            cols.push(row.get::<String>(0).unwrap());
        }

        assert!(cols.contains(&"holder_pid".to_string()), "holder_pid missing: {cols:?}");
        assert!(cols.contains(&"expires_at".to_string()), "expires_at missing: {cols:?}");
    }

    #[tokio::test]
    async fn test_migrate_idempotent() {
        let (_db, conn) = pool::open_memory_async().await.unwrap();

        // Run migrate again — should not error.
        migrate(&conn).await.expect("second migrate should be idempotent");

        let version = get_version(&conn).await.unwrap();
        assert_eq!(version, TARGET_VERSION);
    }
}
