//! File lock primitives for concurrent node coordination.
//!
//! Extracted from the legacy json_store module. Provides advisory file locks
//! to prevent parallel workers from editing the same files.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::fs_utils::atomic_write;

const STATE_DIR: &str = ".state";

/// Error type for lock operations.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LockError>;

/// A file lock entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub file_path: String,
    pub task_id: String,
    pub mode: String,
    pub locked_at: String,
}

fn locks_path(flow_dir: &Path) -> PathBuf {
    flow_dir.join(STATE_DIR).join("locks.json")
}

fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn acquire_flock(path: &Path) -> Result<File> {
    let lock_path = path.with_extension("lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&lock_path)?;
    lock_file.lock_exclusive().map_err(|e| {
        LockError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to acquire lock on {}: {}", lock_path.display(), e),
        ))
    })?;
    Ok(lock_file)
}

/// Read all current locks.
pub fn locks_read(flow_dir: &Path) -> Result<Vec<LockEntry>> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(&path)?;
    let locks: Vec<LockEntry> = serde_json::from_str(&content)?;
    Ok(locks)
}

/// Acquire a lock on a file for a node (file-locked read-modify-write).
pub fn lock_acquire(flow_dir: &Path, file_path: &str, node_id: &str, mode: &str) -> Result<()> {
    ensure_dir(&flow_dir.join(STATE_DIR))?;
    let path = locks_path(flow_dir);
    let _flock = acquire_flock(&path);
    let mut locks = locks_read(flow_dir)?;
    // Remove existing lock by same node on same file (idempotent)
    locks.retain(|l| !(l.file_path == file_path && l.task_id == node_id));
    locks.push(LockEntry {
        file_path: file_path.to_string(),
        task_id: node_id.to_string(),
        mode: mode.to_string(),
        locked_at: Utc::now().to_rfc3339(),
    });
    let content = serde_json::to_string_pretty(&locks)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(())
}

/// Release all locks held by a node. Returns number released.
pub fn lock_release_node(flow_dir: &Path, node_id: &str) -> Result<u32> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(0);
    }
    let _flock = acquire_flock(&path);
    let mut locks = locks_read(flow_dir)?;
    let before = locks.len();
    locks.retain(|l| l.task_id != node_id);
    let removed = (before - locks.len()) as u32;
    let content = serde_json::to_string_pretty(&locks)?;
    atomic_write(&path, content.as_bytes())?;
    Ok(removed)
}

/// Clear all locks. Returns number cleared.
pub fn locks_clear(flow_dir: &Path) -> Result<u32> {
    let path = locks_path(flow_dir);
    if !path.exists() {
        return Ok(0);
    }
    let locks = locks_read(flow_dir)?;
    let count = locks.len() as u32;
    atomic_write(&path, b"[]")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_lock_lifecycle() {
        let tmp = TempDir::new().unwrap();
        let flow = tmp.path();
        fs::create_dir_all(flow.join(STATE_DIR)).unwrap();

        lock_acquire(flow, "src/main.rs", "n-1", "write").unwrap();
        let locks = locks_read(flow).unwrap();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].file_path, "src/main.rs");

        let released = lock_release_node(flow, "n-1").unwrap();
        assert_eq!(released, 1);
        assert!(locks_read(flow).unwrap().is_empty());
    }

    #[test]
    fn test_lock_idempotent() {
        let tmp = TempDir::new().unwrap();
        let flow = tmp.path();
        fs::create_dir_all(flow.join(STATE_DIR)).unwrap();

        lock_acquire(flow, "a.rs", "n-1", "write").unwrap();
        lock_acquire(flow, "a.rs", "n-1", "write").unwrap();
        assert_eq!(locks_read(flow).unwrap().len(), 1);
    }
}
