//! GoalStore — CRUD for .flow/goals/{id}/goal.json

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::goal::{Goal, GoalId};
use crate::fs_utils::atomic_write;

/// Store for Goal objects with goal-scoped directories.
pub struct GoalStore {
    root: PathBuf,
}

impl GoalStore {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            root: flow_root.join("goals"),
        }
    }

    fn goal_dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn goal_path(&self, id: &str) -> PathBuf {
        self.goal_dir(id).join("goal.json")
    }

    /// Create a new goal. Creates the goal-scoped directory.
    pub fn create(&self, goal: &Goal) -> Result<(), String> {
        let dir = self.goal_dir(&goal.id);
        fs::create_dir_all(&dir).map_err(|e| format!("create goal dir: {e}"))?;

        let json = serde_json::to_string_pretty(goal).map_err(|e| format!("serialize: {e}"))?;
        atomic_write(&self.goal_path(&goal.id), json.as_bytes()).map_err(|e| format!("write goal: {e}"))
    }

    /// Read a goal by ID.
    pub fn get(&self, id: &str) -> Result<Goal, String> {
        let path = self.goal_path(id);
        let data = fs::read_to_string(&path).map_err(|e| format!("read goal {id}: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("parse goal {id}: {e}"))
    }

    /// Update an existing goal.
    pub fn update(&self, goal: &Goal) -> Result<(), String> {
        if !self.goal_dir(&goal.id).exists() {
            return Err(format!("goal {} does not exist", goal.id));
        }
        let json = serde_json::to_string_pretty(goal).map_err(|e| format!("serialize: {e}"))?;
        atomic_write(&self.goal_path(&goal.id), json.as_bytes()).map_err(|e| format!("write goal: {e}"))
    }

    /// List all goal IDs.
    pub fn list(&self) -> Result<Vec<GoalId>, String> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| format!("read goals dir: {e}"))? {
            let entry = entry.map_err(|e| format!("read entry: {e}"))?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Delete a goal and its entire directory.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let dir = self.goal_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("delete goal {id}: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::goal::{Goal, PlanningMode, SuccessModel};
    use tempfile::TempDir;

    fn setup() -> (TempDir, GoalStore) {
        let tmp = TempDir::new().unwrap();
        let store = GoalStore::new(tmp.path());
        (tmp, store)
    }

    #[test]
    fn test_create_and_get() {
        let (_tmp, store) = setup();
        let goal = Goal::new("g-1-test".into(), "test request".into(), PlanningMode::Direct, SuccessModel::Criteria);
        store.create(&goal).unwrap();
        let loaded = store.get("g-1-test").unwrap();
        assert_eq!(loaded.id, "g-1-test");
        assert_eq!(loaded.request, "test request");
    }

    #[test]
    fn test_list() {
        let (_tmp, store) = setup();
        let g1 = Goal::new("g-1-a".into(), "a".into(), PlanningMode::Direct, SuccessModel::Criteria);
        let g2 = Goal::new("g-2-b".into(), "b".into(), PlanningMode::Graph, SuccessModel::Numeric);
        store.create(&g1).unwrap();
        store.create(&g2).unwrap();
        let ids = store.list().unwrap();
        assert_eq!(ids, vec!["g-1-a", "g-2-b"]);
    }

    #[test]
    fn test_update() {
        let (_tmp, store) = setup();
        let mut goal = Goal::new("g-1".into(), "original".into(), PlanningMode::Direct, SuccessModel::Criteria);
        store.create(&goal).unwrap();
        goal.request = "updated".into();
        store.update(&goal).unwrap();
        let loaded = store.get("g-1").unwrap();
        assert_eq!(loaded.request, "updated");
    }

    #[test]
    fn test_delete() {
        let (_tmp, store) = setup();
        let goal = Goal::new("g-1".into(), "test".into(), PlanningMode::Direct, SuccessModel::Criteria);
        store.create(&goal).unwrap();
        store.delete("g-1").unwrap();
        assert!(store.get("g-1").is_err());
    }

    #[test]
    fn test_list_empty() {
        let (_tmp, store) = setup();
        assert_eq!(store.list().unwrap(), Vec::<String>::new());
    }
}
