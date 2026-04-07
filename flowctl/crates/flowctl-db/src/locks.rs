//! File lock store — delegates to `json_store::lock_*` / `locks_*`.

use std::path::Path;

use crate::error::DbError;

// Re-export the LockEntry type from json_store.
pub use flowctl_core::json_store::LockEntry;

/// Sync lock store backed by `.state/locks.json`.
pub struct LockStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> LockStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Acquire a lock on a file for a task.
    ///
    /// If another task already holds a lock on the file, returns
    /// `DbError::Constraint`.
    pub fn acquire(&self, file_path: &str, task_id: &str, mode: &str) -> Result<(), DbError> {
        // Check for conflict: another task holding the file.
        let locks = flowctl_core::json_store::locks_read(self.flow_dir)?;
        for lock in &locks {
            if lock.file_path == file_path && lock.task_id != task_id {
                return Err(DbError::Constraint(format!(
                    "file '{}' already locked by task '{}'",
                    file_path, lock.task_id
                )));
            }
        }
        flowctl_core::json_store::lock_acquire(self.flow_dir, file_path, task_id, mode)?;
        Ok(())
    }

    /// Check which task holds a lock on a file.
    pub fn check(&self, file_path: &str) -> Result<Option<String>, DbError> {
        let locks = flowctl_core::json_store::locks_read(self.flow_dir)?;
        for lock in &locks {
            if lock.file_path == file_path {
                return Ok(Some(lock.task_id.clone()));
            }
        }
        Ok(None)
    }

    /// Release all locks held by a task. Returns number released.
    pub fn release_for_task(&self, task_id: &str) -> Result<u32, DbError> {
        let n = flowctl_core::json_store::lock_release_task(self.flow_dir, task_id)?;
        Ok(n)
    }

    /// Release all locks. Returns number released.
    pub fn release_all(&self) -> Result<u32, DbError> {
        let n = flowctl_core::json_store::locks_clear(self.flow_dir)?;
        Ok(n)
    }

    /// List all current locks.
    pub fn list(&self) -> Result<Vec<LockEntry>, DbError> {
        let locks = flowctl_core::json_store::locks_read(self.flow_dir)?;
        Ok(locks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn acquire_and_check() {
        let tmp = TempDir::new().unwrap();
        let store = LockStore::new(tmp.path());

        store.acquire("src/a.rs", "t1", "write").unwrap();
        assert_eq!(store.check("src/a.rs").unwrap().as_deref(), Some("t1"));
        assert!(store.check("src/missing.rs").unwrap().is_none());
    }

    #[test]
    fn acquire_conflict() {
        let tmp = TempDir::new().unwrap();
        let store = LockStore::new(tmp.path());

        store.acquire("src/a.rs", "t1", "write").unwrap();
        let err = store.acquire("src/a.rs", "t2", "write").unwrap_err();
        assert!(matches!(err, DbError::Constraint(_)));
    }

    #[test]
    fn acquire_idempotent() {
        let tmp = TempDir::new().unwrap();
        let store = LockStore::new(tmp.path());

        store.acquire("src/a.rs", "t1", "write").unwrap();
        store.acquire("src/a.rs", "t1", "write").unwrap();
        assert_eq!(store.check("src/a.rs").unwrap().as_deref(), Some("t1"));
    }

    #[test]
    fn release_for_task_and_all() {
        let tmp = TempDir::new().unwrap();
        let store = LockStore::new(tmp.path());

        store.acquire("src/a.rs", "t1", "write").unwrap();
        store.acquire("src/b.rs", "t1", "write").unwrap();
        store.acquire("src/c.rs", "t2", "write").unwrap();

        let n = store.release_for_task("t1").unwrap();
        assert_eq!(n, 2);
        assert!(store.check("src/a.rs").unwrap().is_none());
        assert_eq!(store.check("src/c.rs").unwrap().as_deref(), Some("t2"));

        let n2 = store.release_all().unwrap();
        assert_eq!(n2, 1);
        assert!(store.check("src/c.rs").unwrap().is_none());
    }
}
