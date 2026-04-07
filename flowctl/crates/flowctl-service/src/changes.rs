//! Applies a `Changes` batch against JSON files and the JSONL event log.
//!
//! `ChangesApplier` is the single execution point for declarative mutations.
//! It iterates each `Mutation` in order, writes to the `.flow/` JSON store,
//! and auto-logs an event to the JSONL log for auditability.

use std::path::Path;

use flowctl_core::changes::{Changes, Mutation};
use flowctl_core::json_store;
use flowctl_db::FlowStore;

use crate::error::{ServiceError, ServiceResult};

/// Convert a `json_store::StoreError` into a `ServiceError`.
fn store_err(e: json_store::StoreError) -> ServiceError {
    ServiceError::IoError(std::io::Error::other(e.to_string()))
}

/// Result of applying a `Changes` batch.
#[derive(Debug)]
pub struct ApplyResult {
    /// Number of mutations successfully applied.
    pub applied: usize,
}

/// Executes a `Changes` batch against JSON file storage and the event log.
pub struct ChangesApplier<'a> {
    flow_dir: &'a Path,
    actor: Option<&'a str>,
    session_id: Option<&'a str>,
}

impl<'a> ChangesApplier<'a> {
    pub fn new(flow_dir: &'a Path) -> Self {
        Self {
            flow_dir,
            actor: None,
            session_id: None,
        }
    }

    /// Set the actor (who is applying the changes) for event logging.
    pub fn with_actor(mut self, actor: &'a str) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Set the session ID for event logging.
    pub fn with_session(mut self, session_id: &'a str) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Apply all mutations in order. Stops on first error.
    pub fn apply(&self, changes: &Changes) -> ServiceResult<ApplyResult> {
        let mut applied = 0;

        for mutation in &changes.mutations {
            self.apply_one(mutation)?;
            self.log_event(mutation);
            applied += 1;
        }

        Ok(ApplyResult { applied })
    }

    /// Apply a single mutation to the JSON file store.
    fn apply_one(&self, mutation: &Mutation) -> ServiceResult<()> {
        match mutation {
            Mutation::CreateEpic { epic } | Mutation::UpdateEpic { epic } => {
                json_store::epic_write(self.flow_dir, epic).map_err(store_err)?;
            }
            Mutation::RemoveEpic { id } => {
                json_store::epic_delete(self.flow_dir, id).map_err(store_err)?;
            }
            Mutation::CreateTask { task } | Mutation::UpdateTask { task } => {
                json_store::task_write_definition(self.flow_dir, task).map_err(store_err)?;
            }
            Mutation::RemoveTask { id } => {
                json_store::task_delete(self.flow_dir, id).map_err(store_err)?;
            }
            Mutation::SetTaskState { task_id, state } => {
                json_store::state_write(self.flow_dir, task_id, state).map_err(store_err)?;
            }
            Mutation::RemoveTaskState { task_id } => {
                let path = self.flow_dir.join(".state").join("tasks").join(format!("{task_id}.state.json"));
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            }
            Mutation::SetEpicSpec { epic_id, content } => {
                json_store::epic_spec_write(self.flow_dir, epic_id, content).map_err(store_err)?;
            }
            Mutation::RemoveEpicSpec { epic_id } => {
                let path = self.flow_dir.join("specs").join(format!("{epic_id}.md"));
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            }
            Mutation::SetTaskSpec { task_id, content } => {
                json_store::task_spec_write(self.flow_dir, task_id, content).map_err(store_err)?;
            }
            Mutation::RemoveTaskSpec { task_id } => {
                let path = self.flow_dir.join("tasks").join(format!("{task_id}.md"));
                if path.exists() {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    /// Log a mutation to the JSONL event log. Best-effort: failures are ignored.
    fn log_event(&self, mutation: &Mutation) {
        let store = FlowStore::new(self.flow_dir.to_path_buf());
        let event_type = mutation.event_type();
        let entity_id = mutation.entity_id();

        let epic_id = mutation
            .epic_id()
            .unwrap_or(entity_id);
        let task_id = match mutation {
            Mutation::CreateTask { task } | Mutation::UpdateTask { task } => Some(task.id.as_str()),
            Mutation::RemoveTask { id } => Some(id.as_str()),
            Mutation::SetTaskState { task_id, .. } | Mutation::RemoveTaskState { task_id } => {
                Some(task_id.as_str())
            }
            Mutation::SetTaskSpec { task_id, .. } | Mutation::RemoveTaskSpec { task_id } => {
                Some(task_id.as_str())
            }
            _ => None,
        };

        let event = serde_json::json!({
            "stream_id": format!("mutation:{epic_id}"),
            "type": event_type,
            "entity_id": entity_id,
            "epic_id": epic_id,
            "task_id": task_id,
            "actor": self.actor.unwrap_or("system"),
            "session_id": self.session_id.unwrap_or(""),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let _ = store.events().append(&event.to_string());
    }
}
