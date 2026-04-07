//! Phase progress store — delegates to `json_store::phase_*` / `phases_*`.

use std::path::Path;

use crate::error::DbError;

/// Sync phase progress store backed by `.state/phases.json`.
pub struct PhaseStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> PhaseStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Mark a phase as completed for a task.
    pub fn mark_done(&self, task_id: &str, phase: &str) -> Result<(), DbError> {
        flowctl_core::json_store::phase_mark_done(self.flow_dir, task_id, phase)?;
        Ok(())
    }

    /// Get all completed phases for a task.
    pub fn get_completed(&self, task_id: &str) -> Result<Vec<String>, DbError> {
        let phases = flowctl_core::json_store::phases_completed(self.flow_dir, task_id)?;
        Ok(phases)
    }

    /// Reset all phase progress for a task. Returns the number cleared.
    pub fn reset(&self, task_id: &str) -> Result<(), DbError> {
        flowctl_core::json_store::phases_reset(self.flow_dir, task_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn mark_done_and_get() {
        let tmp = TempDir::new().unwrap();
        let store = PhaseStore::new(tmp.path());

        store.mark_done("t1", "plan").unwrap();
        store.mark_done("t1", "implement").unwrap();

        let phases = store.get_completed("t1").unwrap();
        assert_eq!(phases, vec!["plan", "implement"]);

        // Idempotent re-mark.
        store.mark_done("t1", "plan").unwrap();
        assert_eq!(store.get_completed("t1").unwrap().len(), 2);
    }

    #[test]
    fn reset_clears_phases() {
        let tmp = TempDir::new().unwrap();
        let store = PhaseStore::new(tmp.path());

        store.mark_done("t1", "1").unwrap();
        store.mark_done("t1", "5").unwrap();
        store.mark_done("t2", "1").unwrap();

        store.reset("t1").unwrap();
        assert!(store.get_completed("t1").unwrap().is_empty());
        assert_eq!(store.get_completed("t2").unwrap(), vec!["1"]);
    }
}
