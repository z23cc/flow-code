//! Outputs layer: lightweight narrative handoff between worker tasks.
//!
//! Stored as `.flow/outputs/<task-id>.md` files containing `## Summary` /
//! `## Surprises` / `## Decisions` sections. This is separate from the
//! verified memory system — outputs is a lightweight, file-native narrative
//! layer gated on its own `outputs.enabled` config key.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An entry in the outputs store — pointer + metadata for a `.flow/outputs/*.md` file.
///
/// Per memory convention #008: protocol types live in flowctl-core so all
/// transport layers (CLI, MCP) share the same shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEntry {
    /// Task ID (e.g. `fn-20-abf.2`).
    pub task_id: String,
    /// Absolute path to the output markdown file.
    pub path: PathBuf,
    /// File mtime as seconds since UNIX epoch.
    pub mtime: u64,
}

// ── OutputsStore ────────────────────────────────────────────────────

use std::fs;
use std::time::UNIX_EPOCH;

use crate::error::ServiceResult;
use crate::id::epic_id_from_task;

/// File-system store for `.flow/outputs/*.md` entries.
///
/// Rooted at `<flow_dir>/outputs/`. Callers construct with a `.flow/` path.
pub struct OutputsStore {
    root: PathBuf,
}

impl OutputsStore {
    /// Build a store rooted under `<flow_dir>/outputs/`. Creates the dir if missing.
    pub fn new(flow_dir: &Path) -> ServiceResult<Self> {
        let root = flow_dir.join("outputs");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Return the path `<root>/<task_id>.md`.
    pub fn path_for(&self, task_id: &str) -> PathBuf {
        self.root.join(format!("{task_id}.md"))
    }

    /// Write `content` to `<root>/<task_id>.md`, overwriting any existing file.
    ///
    /// Returns the absolute path written.
    pub fn write(&self, task_id: &str, content: &str) -> ServiceResult<PathBuf> {
        let path = self.path_for(task_id);
        fs::write(&path, content)?;
        Ok(path)
    }

    /// Read the full content of `<root>/<task_id>.md`.
    pub fn read(&self, task_id: &str) -> ServiceResult<String> {
        let path = self.path_for(task_id);
        let content = fs::read_to_string(&path)?;
        Ok(content)
    }

    /// List outputs for an epic, newest-first, optionally capped at `limit`.
    ///
    /// Matches any `<task_id>.md` whose epic-id prefix equals `epic_id`.
    /// Invalid task IDs are silently skipped. Files with unreadable mtime
    /// fall back to mtime=0.
    pub fn list_for_epic(
        &self,
        epic_id: &str,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<OutputEntry>> {
        let mut entries: Vec<OutputEntry> = Vec::new();
        let read_dir = match fs::read_dir(&self.root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(entries),
            Err(e) => return Err(e.into()),
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(task_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // Derive epic and skip if mismatch.
            let Ok(ep) = epic_id_from_task(task_id) else {
                continue;
            };
            if ep != epic_id {
                continue;
            }
            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            entries.push(OutputEntry {
                task_id: task_id.to_string(),
                path: path.clone(),
                mtime,
            });
        }

        // Newest-first.
        entries.sort_by(|a, b| b.mtime.cmp(&a.mtime));
        if let Some(n) = limit {
            entries.truncate(n);
        }
        Ok(entries)
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    fn tmp_flow() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn write_read_roundtrip() {
        let tmp = tmp_flow();
        let store = OutputsStore::new(tmp.path()).unwrap();
        let content = "## Summary\n\nTest summary.\n\n## Surprises\n\nNone.\n";
        let path = store.write("fn-1.1", content).unwrap();
        assert!(path.exists());
        let got = store.read("fn-1.1").unwrap();
        assert_eq!(got, content);
    }

    #[test]
    fn list_for_epic_filters_by_prefix() {
        let tmp = tmp_flow();
        let store = OutputsStore::new(tmp.path()).unwrap();
        store.write("fn-1.1", "a").unwrap();
        store.write("fn-1.2", "b").unwrap();
        store.write("fn-2-other.1", "c").unwrap();

        let listed = store.list_for_epic("fn-1", None).unwrap();
        assert_eq!(listed.len(), 2);
        for e in &listed {
            assert!(e.task_id.starts_with("fn-1."));
        }

        let other = store.list_for_epic("fn-2-other", None).unwrap();
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].task_id, "fn-2-other.1");
    }

    #[test]
    fn list_newest_first_and_limit() {
        let tmp = tmp_flow();
        let store = OutputsStore::new(tmp.path()).unwrap();
        store.write("fn-1.1", "first").unwrap();
        sleep(Duration::from_millis(1100));
        store.write("fn-1.2", "second").unwrap();
        sleep(Duration::from_millis(1100));
        store.write("fn-1.3", "third").unwrap();

        let listed = store.list_for_epic("fn-1", Some(2)).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].task_id, "fn-1.3");
        assert_eq!(listed[1].task_id, "fn-1.2");
    }

    #[test]
    fn list_empty_dir_returns_empty() {
        let tmp = tmp_flow();
        let store = OutputsStore::new(tmp.path()).unwrap();
        let listed = store.list_for_epic("fn-1", Some(3)).unwrap();
        assert!(listed.is_empty());
    }

    #[test]
    fn skips_non_md_and_invalid_ids() {
        let tmp = tmp_flow();
        let store = OutputsStore::new(tmp.path()).unwrap();
        store.write("fn-1.1", "ok").unwrap();
        // Drop a non-md file.
        std::fs::write(tmp.path().join("outputs").join("junk.txt"), "x").unwrap();
        // Drop an invalid task-id md.
        std::fs::write(tmp.path().join("outputs").join("not-a-task.md"), "x").unwrap();

        let listed = store.list_for_epic("fn-1", None).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].task_id, "fn-1.1");
    }
}
