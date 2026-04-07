//! FlowStore — the main entry point for file-based storage.
//!
//! Wraps a `.flow/` directory path and provides access to sub-stores
//! for epics, tasks, events, pipeline, phases, locks, memory, approvals, and gaps.

use std::path::{Path, PathBuf};

use crate::approvals::ApprovalStore;
use crate::events::EventStore;
use crate::gaps::GapStore;
use crate::locks::LockStore;
use crate::memory::MemoryStore;
use crate::phases::PhaseStore;
use crate::pipeline::PipelineStore;

/// Top-level store backed by a `.flow/` directory.
pub struct FlowStore {
    flow_dir: PathBuf,
}

impl FlowStore {
    /// Create a new store rooted at the given `.flow/` directory.
    pub fn new(flow_dir: PathBuf) -> Self {
        Self { flow_dir }
    }

    /// Ensure all required subdirectories exist.
    pub fn ensure_dirs(&self) -> Result<(), crate::error::DbError> {
        flowctl_core::json_store::ensure_dirs(&self.flow_dir)?;
        Ok(())
    }

    /// Return the flow directory path.
    pub fn flow_dir(&self) -> &Path {
        &self.flow_dir
    }

    /// Access the event store.
    pub fn events(&self) -> EventStore<'_> {
        EventStore::new(&self.flow_dir)
    }

    /// Access the pipeline store.
    pub fn pipeline(&self) -> PipelineStore<'_> {
        PipelineStore::new(&self.flow_dir)
    }

    /// Access the phase store.
    pub fn phases(&self) -> PhaseStore<'_> {
        PhaseStore::new(&self.flow_dir)
    }

    /// Access the lock store.
    pub fn locks(&self) -> LockStore<'_> {
        LockStore::new(&self.flow_dir)
    }

    /// Access the memory store.
    pub fn memory(&self) -> MemoryStore<'_> {
        MemoryStore::new(&self.flow_dir)
    }

    /// Access the approval store.
    pub fn approvals(&self) -> ApprovalStore<'_> {
        ApprovalStore::new(&self.flow_dir)
    }

    /// Access the gap store.
    pub fn gaps(&self) -> GapStore<'_> {
        GapStore::new(&self.flow_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn store_ensure_dirs() {
        let tmp = TempDir::new().unwrap();
        let store = FlowStore::new(tmp.path().to_path_buf());
        store.ensure_dirs().unwrap();
        assert!(tmp.path().join("epics").exists());
        assert!(tmp.path().join("tasks").exists());
        assert!(tmp.path().join("specs").exists());
        assert!(tmp.path().join(".state").exists());
        assert!(tmp.path().join("memory").exists());
    }

    #[test]
    fn store_accessors_return_sub_stores() {
        let tmp = TempDir::new().unwrap();
        let store = FlowStore::new(tmp.path().to_path_buf());
        // Just verify the accessors compile and return the right types.
        let _ = store.events();
        let _ = store.pipeline();
        let _ = store.phases();
        let _ = store.locks();
        let _ = store.memory();
        let _ = store.approvals();
        let _ = store.gaps();
    }
}
