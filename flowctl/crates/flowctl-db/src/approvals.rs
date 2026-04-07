//! Approval store — delegates to `json_store::approvals_*`.

use std::path::Path;

use crate::error::DbError;

/// Sync approval store backed by `.state/approvals.json`.
pub struct ApprovalStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> ApprovalStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Read all approval records.
    pub fn read(&self) -> Result<Vec<serde_json::Value>, DbError> {
        let approvals = flowctl_core::json_store::approvals_read(self.flow_dir)?;
        Ok(approvals)
    }

    /// Write approval records (atomic replacement).
    pub fn write(&self, approvals: &[serde_json::Value]) -> Result<(), DbError> {
        flowctl_core::json_store::approvals_write(self.flow_dir, approvals)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn approvals_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = ApprovalStore::new(tmp.path());

        assert!(store.read().unwrap().is_empty());

        let approvals = vec![
            serde_json::json!({"reviewer": "alice", "status": "approved"}),
            serde_json::json!({"reviewer": "bob", "status": "needs_work"}),
        ];
        store.write(&approvals).unwrap();

        let read_back = store.read().unwrap();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0]["reviewer"], "alice");
    }
}
