//! PlanStore — versioned plans at .flow/goals/{id}/plans/{rev}.json

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::plan::PlanVersion;

/// Store for PlanVersion objects. Plans are immutable — replan creates new version.
pub struct PlanStore {
    root: PathBuf,
}

impl PlanStore {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            root: flow_root.join("goals"),
        }
    }

    fn plans_dir(&self, goal_id: &str) -> PathBuf {
        self.root.join(goal_id).join("plans")
    }

    fn plan_path(&self, goal_id: &str, rev: u32) -> PathBuf {
        self.plans_dir(goal_id).join(format!("{rev:04}.json"))
    }

    /// Create a new plan version. Returns the assigned revision number.
    pub fn create_version(&self, plan: &PlanVersion) -> Result<u32, String> {
        let dir = self.plans_dir(&plan.goal_id);
        fs::create_dir_all(&dir).map_err(|e| format!("create plans dir: {e}"))?;

        let rev = plan.rev;
        let json = serde_json::to_string_pretty(plan).map_err(|e| format!("serialize: {e}"))?;
        let path = self.plan_path(&plan.goal_id, rev);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &json).map_err(|e| format!("write: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("rename: {e}"))?;
        Ok(rev)
    }

    /// Get a specific plan version.
    pub fn get(&self, goal_id: &str, rev: u32) -> Result<PlanVersion, String> {
        let path = self.plan_path(goal_id, rev);
        let data = fs::read_to_string(&path).map_err(|e| format!("read plan {goal_id} rev {rev}: {e}"))?;
        serde_json::from_str(&data).map_err(|e| format!("parse plan: {e}"))
    }

    /// Get the latest plan version.
    pub fn get_latest(&self, goal_id: &str) -> Result<PlanVersion, String> {
        let revs = self.list_revisions(goal_id)?;
        let max_rev = revs.last().ok_or_else(|| format!("no plans for {goal_id}"))?;
        self.get(goal_id, *max_rev)
    }

    /// List all revision numbers for a goal.
    pub fn list_revisions(&self, goal_id: &str) -> Result<Vec<u32>, String> {
        let dir = self.plans_dir(goal_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut revs = Vec::new();
        for entry in fs::read_dir(&dir).map_err(|e| format!("read plans: {e}"))? {
            let entry = entry.map_err(|e| format!("entry: {e}"))?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(rev_str) = name.strip_suffix(".json") {
                    if let Ok(rev) = rev_str.parse::<u32>() {
                        revs.push(rev);
                    }
                }
            }
        }
        revs.sort();
        Ok(revs)
    }

    /// Compute the next revision number.
    pub fn next_rev(&self, goal_id: &str) -> Result<u32, String> {
        let revs = self.list_revisions(goal_id)?;
        Ok(revs.last().map_or(1, |r| r + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::Node;
    use crate::domain::plan::PlanVersion;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PlanStore) {
        let tmp = TempDir::new().unwrap();
        // Create the goal directory first
        fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let store = PlanStore::new(tmp.path());
        (tmp, store)
    }

    #[test]
    fn test_create_and_get() {
        let (_tmp, store) = setup();
        let plan = PlanVersion::initial(
            "g-1".into(),
            vec![Node::new("n-1".into(), "task 1".into())],
            vec![],
            "initial plan".into(),
        );
        let rev = store.create_version(&plan).unwrap();
        assert_eq!(rev, 1);
        let loaded = store.get("g-1", 1).unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.rationale, "initial plan");
    }

    #[test]
    fn test_versioning() {
        let (_tmp, store) = setup();
        let p1 = PlanVersion::initial("g-1".into(), vec![], vec![], "v1".into());
        store.create_version(&p1).unwrap();

        let mut p2 = PlanVersion::initial("g-1".into(), vec![Node::new("n-1".into(), "new".into())], vec![], "v2".into());
        p2.rev = 2;
        store.create_version(&p2).unwrap();

        let revs = store.list_revisions("g-1").unwrap();
        assert_eq!(revs, vec![1, 2]);

        let latest = store.get_latest("g-1").unwrap();
        assert_eq!(latest.rev, 2);
        assert_eq!(latest.rationale, "v2");
    }

    #[test]
    fn test_next_rev() {
        let (_tmp, store) = setup();
        assert_eq!(store.next_rev("g-1").unwrap(), 1);
        let p = PlanVersion::initial("g-1".into(), vec![], vec![], "test".into());
        store.create_version(&p).unwrap();
        assert_eq!(store.next_rev("g-1").unwrap(), 2);
    }
}
