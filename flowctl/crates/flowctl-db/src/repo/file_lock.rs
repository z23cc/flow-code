//! Async repository for runtime file locks (Teams mode concurrency).
//!
//! Uses PID-based crash detection + TTL fallback for hung processes.
//! Stale locks (dead PID or expired TTL) are auto-cleaned on `acquire()`.
//!
//! Lock modes: `write` (exclusive), `read` (shared with other reads),
//! `directory_add` (shared with reads and other directory_adds).

use chrono::{Duration, Utc};
use libsql::{params, Connection};
use nix::sys::signal;
use nix::unistd::Pid;

use crate::error::DbError;

/// Default lock TTL: 45 minutes.
const LOCK_TTL_MINUTES: i64 = 45;

/// Lock mode for file locking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockMode {
    Read,
    Write,
    DirectoryAdd,
}

impl LockMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            LockMode::Read => "read",
            LockMode::Write => "write",
            LockMode::DirectoryAdd => "directory_add",
        }
    }

    pub fn from_str(s: &str) -> Result<Self, DbError> {
        match s {
            "read" => Ok(LockMode::Read),
            "write" => Ok(LockMode::Write),
            "directory_add" => Ok(LockMode::DirectoryAdd),
            _ => Err(DbError::Schema(format!("invalid lock mode: {s}"))),
        }
    }

    /// Check if two lock modes are compatible (can coexist on the same file).
    pub fn is_compatible(&self, other: &LockMode) -> bool {
        matches!(
            (self, other),
            (LockMode::Read, LockMode::Read)
                | (LockMode::Read, LockMode::DirectoryAdd)
                | (LockMode::DirectoryAdd, LockMode::Read)
                | (LockMode::DirectoryAdd, LockMode::DirectoryAdd)
        )
    }
}

/// A lock entry returned by `check_locks`.
#[derive(Debug, Clone)]
pub struct LockEntry {
    pub task_id: String,
    pub lock_mode: LockMode,
}

/// Async repository for runtime file locks. Load-bearing for Teams-mode
/// concurrency: `acquire` on an incompatibly-locked file returns
/// `DbError::Constraint`.
pub struct FileLockRepo {
    conn: Connection,
}

impl FileLockRepo {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Acquire a lock on a file for a task with a given mode.
    ///
    /// Calls `cleanup_stale()` first, then checks existing locks for
    /// compatibility. Compatible locks (e.g. read+read) coexist;
    /// incompatible locks return `DbError::Constraint`.
    pub async fn acquire(
        &self,
        file_path: &str,
        task_id: &str,
        mode: &LockMode,
    ) -> Result<(), DbError> {
        self.cleanup_stale().await?;

        // Check existing locks on this file.
        let existing = self.check_locks(file_path).await?;

        for entry in &existing {
            if entry.task_id == task_id {
                // Same task re-locking — idempotent.
                return Ok(());
            }
            if !mode.is_compatible(&entry.lock_mode) {
                return Err(DbError::Constraint(format!(
                    "file already locked: {file_path} (by {} in {} mode)",
                    entry.task_id,
                    entry.lock_mode.as_str()
                )));
            }
        }

        let pid = std::process::id();
        let expires_at = (Utc::now() + Duration::minutes(LOCK_TTL_MINUTES)).to_rfc3339();

        self.conn
            .execute(
                "INSERT OR IGNORE INTO file_locks (file_path, task_id, locked_at, holder_pid, expires_at, lock_mode)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    file_path.to_string(),
                    task_id.to_string(),
                    Utc::now().to_rfc3339(),
                    pid as i64,
                    expires_at,
                    mode.as_str().to_string(),
                ],
            )
            .await?;

        Ok(())
    }

    /// Remove stale locks: dead PIDs (via `kill(pid, 0)`) and expired TTLs.
    pub async fn cleanup_stale(&self) -> Result<u64, DbError> {
        let mut total_cleaned = 0u64;

        // 1. Delete expired locks (expires_at < now).
        let now = Utc::now().to_rfc3339();
        let expired = self
            .conn
            .execute(
                "DELETE FROM file_locks WHERE expires_at IS NOT NULL AND expires_at < ?1",
                params![now],
            )
            .await?;
        total_cleaned += expired;

        // 2. Check PIDs of remaining locks — delete dead ones.
        let mut rows = self
            .conn
            .query(
                "SELECT file_path, task_id, holder_pid FROM file_locks WHERE holder_pid IS NOT NULL",
                (),
            )
            .await?;

        let mut dead_keys: Vec<(String, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            let fp: String = row.get(0)?;
            let tid: String = row.get(1)?;
            let pid: i64 = row.get(2)?;

            if !is_process_alive(pid as i32) {
                dead_keys.push((fp, tid));
            }
        }

        for (fp, tid) in &dead_keys {
            let n = self
                .conn
                .execute(
                    "DELETE FROM file_locks WHERE file_path = ?1 AND task_id = ?2",
                    params![fp.clone(), tid.clone()],
                )
                .await?;
            total_cleaned += n;
        }

        Ok(total_cleaned)
    }

    /// Extend `expires_at` for all locks held by a task (heartbeat).
    pub async fn heartbeat(&self, task_id: &str) -> Result<u64, DbError> {
        let new_expires = (Utc::now() + Duration::minutes(LOCK_TTL_MINUTES)).to_rfc3339();
        let n = self
            .conn
            .execute(
                "UPDATE file_locks SET expires_at = ?1 WHERE task_id = ?2",
                params![new_expires, task_id.to_string()],
            )
            .await?;
        Ok(n)
    }

    /// Release locks held by a task. Returns number of rows deleted.
    pub async fn release_for_task(&self, task_id: &str) -> Result<u64, DbError> {
        let n = self
            .conn
            .execute(
                "DELETE FROM file_locks WHERE task_id = ?1",
                params![task_id.to_string()],
            )
            .await?;
        Ok(n)
    }

    /// Release all locks (between waves). Returns number of rows deleted.
    pub async fn release_all(&self) -> Result<u64, DbError> {
        let n = self.conn.execute("DELETE FROM file_locks", ()).await?;
        Ok(n)
    }

    /// Check all locks on a file. Returns list of (task_id, lock_mode) pairs.
    pub async fn check_locks(&self, file_path: &str) -> Result<Vec<LockEntry>, DbError> {
        let mut rows = self
            .conn
            .query(
                "SELECT task_id, lock_mode FROM file_locks WHERE file_path = ?1",
                params![file_path.to_string()],
            )
            .await?;

        let mut entries = Vec::new();
        while let Some(row) = rows.next().await? {
            let task_id: String = row.get(0)?;
            let mode_str: String = row.get(1)?;
            entries.push(LockEntry {
                task_id,
                lock_mode: LockMode::from_str(&mode_str)?,
            });
        }
        Ok(entries)
    }

    /// Check if a file is locked. Returns the first locking task_id if so.
    /// For backward compatibility — use `check_locks` for full info.
    pub async fn check(&self, file_path: &str) -> Result<Option<String>, DbError> {
        let entries = self.check_locks(file_path).await?;
        Ok(entries.into_iter().next().map(|e| e.task_id))
    }
}

/// Check if a process is alive using `kill(pid, 0)`.
fn is_process_alive(pid: i32) -> bool {
    // kill(pid, 0) returns Ok if process exists, Err(ESRCH) if not.
    signal::kill(Pid::from_raw(pid), None).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::open_memory_async;

    #[tokio::test]
    async fn lock_mode_compatibility() {
        assert!(LockMode::Read.is_compatible(&LockMode::Read));
        assert!(LockMode::Read.is_compatible(&LockMode::DirectoryAdd));
        assert!(LockMode::DirectoryAdd.is_compatible(&LockMode::Read));
        assert!(LockMode::DirectoryAdd.is_compatible(&LockMode::DirectoryAdd));

        assert!(!LockMode::Read.is_compatible(&LockMode::Write));
        assert!(!LockMode::Write.is_compatible(&LockMode::Read));
        assert!(!LockMode::Write.is_compatible(&LockMode::Write));
        assert!(!LockMode::Write.is_compatible(&LockMode::DirectoryAdd));
        assert!(!LockMode::DirectoryAdd.is_compatible(&LockMode::Write));
    }

    #[tokio::test]
    async fn acquire_write_write_conflicts() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        let err = repo.acquire("src/a.rs", "fn-1.2", &LockMode::Write).await.unwrap_err();
        assert!(matches!(err, DbError::Constraint(_)));
    }

    #[tokio::test]
    async fn acquire_read_read_compatible() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Read).await.unwrap();
        repo.acquire("src/a.rs", "fn-1.2", &LockMode::Read).await.unwrap();

        let entries = repo.check_locks("src/a.rs").await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn acquire_read_write_conflicts() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Read).await.unwrap();
        let err = repo.acquire("src/a.rs", "fn-1.2", &LockMode::Write).await.unwrap_err();
        assert!(matches!(err, DbError::Constraint(_)));
    }

    #[tokio::test]
    async fn acquire_directory_add_compatible() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/", "fn-1.1", &LockMode::DirectoryAdd).await.unwrap();
        repo.acquire("src/", "fn-1.2", &LockMode::DirectoryAdd).await.unwrap();
        repo.acquire("src/", "fn-1.3", &LockMode::Read).await.unwrap();

        let entries = repo.check_locks("src/").await.unwrap();
        assert_eq!(entries.len(), 3);

        // Write conflicts with directory_add
        let err = repo.acquire("src/", "fn-1.4", &LockMode::Write).await.unwrap_err();
        assert!(matches!(err, DbError::Constraint(_)));
    }

    #[tokio::test]
    async fn acquire_idempotent_same_task() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        assert_eq!(repo.check("src/a.rs").await.unwrap().as_deref(), Some("fn-1.1"));
    }

    #[tokio::test]
    async fn release_and_reacquire() {
        let (_db, conn) = open_memory_async().await.unwrap();
        let repo = FileLockRepo::new(conn);

        repo.acquire("src/a.rs", "fn-1.1", &LockMode::Write).await.unwrap();
        repo.release_for_task("fn-1.1").await.unwrap();
        repo.acquire("src/a.rs", "fn-1.2", &LockMode::Write).await.unwrap();
        assert_eq!(repo.check("src/a.rs").await.unwrap().as_deref(), Some("fn-1.2"));
    }
}
