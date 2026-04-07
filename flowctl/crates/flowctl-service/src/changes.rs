//! Applies a `Changes` batch against JSON files and the libSQL event log.
//!
//! `ChangesApplier` is the single execution point for declarative mutations.
//! It iterates each `Mutation` in order, writes to the `.flow/` JSON store,
//! and auto-logs an event to the `events` table for auditability.

use std::path::Path;

use flowctl_core::changes::{Changes, Mutation};
use flowctl_core::events::{
    EpicEvent, EventMetadata, FlowEvent, TaskEvent, epic_stream_id, task_stream_id,
};
use flowctl_core::json_store;
use flowctl_db::{EventRepo, EventStoreRepo};

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
    /// Event IDs for each logged event (one per mutation).
    pub event_ids: Vec<i64>,
}

/// Executes a `Changes` batch against JSON file storage and the event log.
pub struct ChangesApplier<'a> {
    flow_dir: &'a Path,
    event_repo: &'a EventRepo,
    event_store: Option<EventStoreRepo>,
    actor: Option<&'a str>,
    session_id: Option<&'a str>,
}

impl<'a> ChangesApplier<'a> {
    pub fn new(flow_dir: &'a Path, event_repo: &'a EventRepo) -> Self {
        Self {
            flow_dir,
            event_repo,
            event_store: None,
            actor: None,
            session_id: None,
        }
    }

    /// Set an event store for domain event emission alongside audit logging.
    pub fn with_event_store(mut self, store: EventStoreRepo) -> Self {
        self.event_store = Some(store);
        self
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
    pub async fn apply(&self, changes: &Changes) -> ServiceResult<ApplyResult> {
        let mut applied = 0;
        let mut event_ids = Vec::with_capacity(changes.len());

        for mutation in &changes.mutations {
            // Emit domain event to event store (best-effort, before mutation)
            self.emit_domain_event(mutation).await;

            self.apply_one(mutation)?;

            let event_id = self.log_event(mutation).await?;
            event_ids.push(event_id);
            applied += 1;
        }

        Ok(ApplyResult { applied, event_ids })
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

    /// Log a mutation to the events table.
    async fn log_event(&self, mutation: &Mutation) -> ServiceResult<i64> {
        let event_type = mutation.event_type();
        let entity_id = mutation.entity_id();

        // Derive epic_id and task_id for the event row.
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

        // Payload: JSON of the entity ID for traceability.
        let payload = serde_json::json!({ "entity_id": entity_id }).to_string();

        let row_id = self
            .event_repo
            .insert(
                epic_id,
                task_id,
                event_type,
                self.actor,
                Some(&payload),
                self.session_id,
            )
            .await
            .map_err(ServiceError::DbError)?;

        Ok(row_id)
    }

    /// Emit a domain event to the event store for create mutations.
    /// Best-effort: failures are silently ignored so they don't block the pipeline.
    async fn emit_domain_event(&self, mutation: &Mutation) {
        let store = match self.event_store {
            Some(ref s) => s,
            None => return,
        };

        let (stream, flow_event) = match mutation {
            Mutation::CreateEpic { epic } => (
                epic_stream_id(&epic.id),
                FlowEvent::Epic(EpicEvent::Created),
            ),
            Mutation::CreateTask { task } => (
                task_stream_id(&task.id),
                FlowEvent::Task(TaskEvent::Created),
            ),
            _ => return,
        };

        let metadata = EventMetadata {
            actor: self.actor.unwrap_or("system").into(),
            source_cmd: "changes_applier".into(),
            session_id: self.session_id.unwrap_or("").into(),
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        };

        let _ = store.append(&stream, &flow_event, &metadata).await.ok();
    }
}
