//! Pipeline progress store — delegates to `json_store::pipeline_*`.

use std::path::Path;

use crate::error::DbError;

/// Sync pipeline store backed by `.state/pipeline.json`.
pub struct PipelineStore<'a> {
    flow_dir: &'a Path,
}

impl<'a> PipelineStore<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self { flow_dir }
    }

    /// Read the current pipeline phase for an epic.
    pub fn read(&self, epic_id: &str) -> Result<Option<String>, DbError> {
        let phase = flowctl_core::json_store::pipeline_read(self.flow_dir, epic_id)?;
        Ok(phase)
    }

    /// Set the pipeline phase for an epic.
    pub fn write(&self, epic_id: &str, phase: &str) -> Result<(), DbError> {
        flowctl_core::json_store::pipeline_write(self.flow_dir, epic_id, phase)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn pipeline_read_write() {
        let tmp = TempDir::new().unwrap();
        let store = PipelineStore::new(tmp.path());

        assert_eq!(store.read("fn-1").unwrap(), None);

        store.write("fn-1", "plan").unwrap();
        assert_eq!(store.read("fn-1").unwrap().as_deref(), Some("plan"));

        store.write("fn-1", "work").unwrap();
        assert_eq!(store.read("fn-1").unwrap().as_deref(), Some("work"));

        store.write("fn-2", "plan").unwrap();
        assert_eq!(store.read("fn-2").unwrap().as_deref(), Some("plan"));
        assert_eq!(store.read("fn-1").unwrap().as_deref(), Some("work"));
    }
}
