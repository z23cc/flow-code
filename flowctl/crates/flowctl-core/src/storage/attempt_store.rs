//! AttemptStore — per-node attempts at .flow/goals/{id}/attempts/{node}/{seq}.json

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::node::Attempt;

/// Store for Attempt records. One node can have multiple attempts (retries).
pub struct AttemptStore {
    root: PathBuf,
}

impl AttemptStore {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            root: flow_root.join("goals"),
        }
    }

    fn attempts_dir(&self, goal_id: &str, node_id: &str) -> PathBuf {
        self.root.join(goal_id).join("attempts").join(node_id)
    }

    fn attempt_path(&self, goal_id: &str, node_id: &str, seq: u32) -> PathBuf {
        self.attempts_dir(goal_id, node_id).join(format!("{seq:04}.json"))
    }

    /// Record a new attempt.
    pub fn record(&self, goal_id: &str, attempt: &Attempt) -> Result<(), String> {
        let dir = self.attempts_dir(goal_id, &attempt.node_id);
        fs::create_dir_all(&dir).map_err(|e| format!("create attempts dir: {e}"))?;

        let json = serde_json::to_string_pretty(attempt).map_err(|e| format!("serialize: {e}"))?;
        let path = self.attempt_path(goal_id, &attempt.node_id, attempt.seq);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &json).map_err(|e| format!("write: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))
    }

    /// Get a specific attempt.
    pub fn get(&self, goal_id: &str, node_id: &str, seq: u32) -> Result<Attempt, String> {
        let path = self.attempt_path(goal_id, node_id, seq);
        let data = fs::read_to_string(&path).map_err(|e| format!("read attempt: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("parse attempt: {e}"))
    }

    /// List all attempts for a node.
    pub fn list_for_node(&self, goal_id: &str, node_id: &str) -> Result<Vec<Attempt>, String> {
        let dir = self.attempts_dir(goal_id, node_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut attempts = Vec::new();
        let mut paths: Vec<_> = fs::read_dir(&dir)
            .map_err(|e| format!("read attempts: {e}"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .map(|e| e.path())
            .collect();
        paths.sort();

        for path in paths {
            let data = fs::read_to_string(&path).map_err(|e| format!("read: {e}"))?;
            let attempt: Attempt = serde_json::from_str(&data).map_err(|e| format!("parse: {e}"))?;
            attempts.push(attempt);
        }
        Ok(attempts)
    }

    /// Count attempts for a node (useful for escalation level calculation).
    pub fn count_for_node(&self, goal_id: &str, node_id: &str) -> Result<u32, String> {
        Ok(self.list_for_node(goal_id, node_id)?.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn setup() -> (TempDir, AttemptStore) {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let store = AttemptStore::new(tmp.path());
        (tmp, store)
    }

    fn make_attempt(node_id: &str, seq: u32) -> Attempt {
        Attempt {
            node_id: node_id.into(),
            seq,
            summary: format!("attempt {seq}"),
            status: "done".into(),
            changed_files: vec![],
            commits: vec![],
            tests: vec![],
            findings: vec![],
            suggested_mutations: vec![],
            duration_seconds: 10,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_record_and_get() {
        let (_tmp, store) = setup();
        let attempt = make_attempt("n-1", 1);
        store.record("g-1", &attempt).unwrap();
        let loaded = store.get("g-1", "n-1", 1).unwrap();
        assert_eq!(loaded.summary, "attempt 1");
    }

    #[test]
    fn test_list_for_node() {
        let (_tmp, store) = setup();
        store.record("g-1", &make_attempt("n-1", 1)).unwrap();
        store.record("g-1", &make_attempt("n-1", 2)).unwrap();
        let attempts = store.list_for_node("g-1", "n-1").unwrap();
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].seq, 1);
        assert_eq!(attempts[1].seq, 2);
    }

    #[test]
    fn test_count_empty() {
        let (_tmp, store) = setup();
        assert_eq!(store.count_for_node("g-1", "n-99").unwrap(), 0);
    }
}
