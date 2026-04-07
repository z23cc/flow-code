//! Gap store — delegates to `json_store::gaps_*`.

use std::path::Path;

use crate::error::DbError;

// Re-export the GapEntry type from json_store.
pub use flowctl_core::json_store::GapEntry;

/// Sync gap store backed by `gaps/<epic-id>.json`.
pub struct GapStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> GapStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Read gaps for an epic.
    pub fn read(&self, epic_id: &str) -> Result<Vec<GapEntry>, DbError> {
        let gaps = flowctl_core::json_store::gaps_read(self.flow_dir, epic_id)?;
        Ok(gaps)
    }

    /// Write gaps for an epic (atomic replacement).
    pub fn write(&self, epic_id: &str, gaps: &[GapEntry]) -> Result<(), DbError> {
        flowctl_core::json_store::gaps_write(self.flow_dir, epic_id, gaps)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn gaps_round_trip() {
        let tmp = TempDir::new().unwrap();
        let store = GapStore::new(tmp.path());

        assert!(store.read("fn-1").unwrap().is_empty());

        let gaps = vec![
            GapEntry {
                id: 1,
                capability: "auth".into(),
                priority: "required".into(),
                source: "test".into(),
                resolved: false,
            },
            GapEntry {
                id: 2,
                capability: "logging".into(),
                priority: "nice-to-have".into(),
                source: "test".into(),
                resolved: true,
            },
        ];
        store.write("fn-1", &gaps).unwrap();

        let read_back = store.read("fn-1").unwrap();
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].capability, "auth");
        assert!(read_back[1].resolved);
    }
}
