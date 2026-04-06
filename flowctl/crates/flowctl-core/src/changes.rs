//! Declarative mutation intents for flowctl entities.
//!
//! `Changes` captures all intended creates, updates, and removes as a
//! serializable bag of intents. No side effects — an applier in the service
//! layer executes them against storage.
//!
//! Modelled after IWE's `changes.rs` pattern, adapted for flowctl's
//! Epic/Task/TaskState entity model.

use serde::{Deserialize, Serialize};

use crate::json_store::TaskState;
use crate::types::{Epic, Task};

/// The kind of entity being mutated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Epic,
    Task,
    TaskState,
    EpicSpec,
    TaskSpec,
}

/// A single mutation intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Mutation {
    /// Create a new epic.
    CreateEpic { epic: Epic },
    /// Update an existing epic.
    UpdateEpic { epic: Epic },
    /// Remove an epic by ID.
    RemoveEpic { id: String },

    /// Create a new task.
    CreateTask { task: Task },
    /// Update an existing task.
    UpdateTask { task: Task },
    /// Remove a task by ID.
    RemoveTask { id: String },

    /// Create or update task runtime state.
    SetTaskState { task_id: String, state: TaskState },
    /// Remove task runtime state by task ID.
    RemoveTaskState { task_id: String },

    /// Write an epic spec (Markdown).
    SetEpicSpec { epic_id: String, content: String },
    /// Remove an epic spec.
    RemoveEpicSpec { epic_id: String },

    /// Write a task spec (Markdown).
    SetTaskSpec { task_id: String, content: String },
    /// Remove a task spec.
    RemoveTaskSpec { task_id: String },
}

impl Mutation {
    /// Human-readable event type string for audit logging.
    pub fn event_type(&self) -> &'static str {
        match self {
            Mutation::CreateEpic { .. } => "epic.create",
            Mutation::UpdateEpic { .. } => "epic.update",
            Mutation::RemoveEpic { .. } => "epic.remove",
            Mutation::CreateTask { .. } => "task.create",
            Mutation::UpdateTask { .. } => "task.update",
            Mutation::RemoveTask { .. } => "task.remove",
            Mutation::SetTaskState { .. } => "task_state.set",
            Mutation::RemoveTaskState { .. } => "task_state.remove",
            Mutation::SetEpicSpec { .. } => "epic_spec.set",
            Mutation::RemoveEpicSpec { .. } => "epic_spec.remove",
            Mutation::SetTaskSpec { .. } => "task_spec.set",
            Mutation::RemoveTaskSpec { .. } => "task_spec.remove",
        }
    }

    /// Extract the entity ID affected by this mutation.
    pub fn entity_id(&self) -> &str {
        match self {
            Mutation::CreateEpic { epic } | Mutation::UpdateEpic { epic } => &epic.id,
            Mutation::RemoveEpic { id } => id,
            Mutation::CreateTask { task } | Mutation::UpdateTask { task } => &task.id,
            Mutation::RemoveTask { id } => id,
            Mutation::SetTaskState { task_id, .. } | Mutation::RemoveTaskState { task_id } => {
                task_id
            }
            Mutation::SetEpicSpec { epic_id, .. } | Mutation::RemoveEpicSpec { epic_id } => {
                epic_id
            }
            Mutation::SetTaskSpec { task_id, .. } | Mutation::RemoveTaskSpec { task_id } => {
                task_id
            }
        }
    }

    /// Extract the epic ID for this mutation (for event logging).
    /// For task mutations, derives the epic ID from the task ID or task.epic field.
    pub fn epic_id(&self) -> Option<&str> {
        match self {
            Mutation::CreateEpic { epic } | Mutation::UpdateEpic { epic } => Some(&epic.id),
            Mutation::RemoveEpic { id } => Some(id),
            Mutation::CreateTask { task } | Mutation::UpdateTask { task } => Some(&task.epic),
            Mutation::RemoveTask { .. } => None, // caller must resolve
            Mutation::SetTaskState { .. } | Mutation::RemoveTaskState { .. } => None,
            Mutation::SetEpicSpec { epic_id, .. } | Mutation::RemoveEpicSpec { epic_id } => {
                Some(epic_id)
            }
            Mutation::SetTaskSpec { .. } | Mutation::RemoveTaskSpec { .. } => None,
        }
    }
}

/// A batch of declarative mutation intents.
///
/// Build up mutations, then hand the `Changes` to an applier which executes
/// them against JSON files and the libSQL database.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Changes {
    /// Ordered list of mutations to apply.
    pub mutations: Vec<Mutation>,
}

impl Changes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: append a mutation and return self.
    pub fn with(mut self, mutation: Mutation) -> Self {
        self.mutations.push(mutation);
        self
    }

    /// Push a mutation.
    pub fn push(&mut self, mutation: Mutation) {
        self.mutations.push(mutation);
    }

    /// Whether there are no mutations.
    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    /// Number of mutations.
    pub fn len(&self) -> usize {
        self.mutations.len()
    }

    /// All entity IDs affected by these changes.
    pub fn affected_ids(&self) -> Vec<&str> {
        self.mutations.iter().map(Mutation::entity_id).collect()
    }

    /// Merge another `Changes` into this one.
    pub fn extend(&mut self, other: Changes) {
        self.mutations.extend(other.mutations);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state_machine::Status;
    use crate::types::{Domain, EpicStatus, ReviewStatus};
    use chrono::Utc;

    fn make_epic(id: &str) -> Epic {
        Epic {
            schema_version: 1,
            id: id.to_string(),
            title: "Test".to_string(),
            status: EpicStatus::Open,
            branch_name: None,
            plan_review: ReviewStatus::Unknown,
            completion_review: ReviewStatus::Unknown,
            depends_on_epics: vec![],
            default_impl: None,
            default_review: None,
            default_sync: None,
            auto_execute_pending: None,
            auto_execute_set_at: None,
            archived: false,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_task(id: &str, epic: &str) -> Task {
        Task {
            schema_version: 1,
            id: id.to_string(),
            epic: epic.to_string(),
            title: "Test Task".to_string(),
            status: Status::Todo,
            priority: None,
            domain: Domain::General,
            depends_on: vec![],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_changes() {
        let c = Changes::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.affected_ids().is_empty());
    }

    #[test]
    fn builder_pattern() {
        let c = Changes::new()
            .with(Mutation::CreateEpic {
                epic: make_epic("fn-1-test"),
            })
            .with(Mutation::CreateTask {
                task: make_task("fn-1-test.1", "fn-1-test"),
            });
        assert_eq!(c.len(), 2);
        assert_eq!(c.affected_ids(), vec!["fn-1-test", "fn-1-test.1"]);
    }

    #[test]
    fn push_pattern() {
        let mut c = Changes::new();
        c.push(Mutation::RemoveEpic {
            id: "fn-1-test".into(),
        });
        assert_eq!(c.len(), 1);
        assert!(!c.is_empty());
    }

    #[test]
    fn extend_merges() {
        let mut a = Changes::new().with(Mutation::RemoveEpic {
            id: "fn-1-a".into(),
        });
        let b = Changes::new().with(Mutation::RemoveEpic {
            id: "fn-2-b".into(),
        });
        a.extend(b);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn mutation_event_types() {
        assert_eq!(
            Mutation::CreateEpic {
                epic: make_epic("x")
            }
            .event_type(),
            "epic.create"
        );
        assert_eq!(
            Mutation::RemoveTask { id: "x".into() }.event_type(),
            "task.remove"
        );
        assert_eq!(
            Mutation::SetTaskState {
                task_id: "x".into(),
                state: TaskState::default()
            }
            .event_type(),
            "task_state.set"
        );
    }

    #[test]
    fn serde_roundtrip() {
        let c = Changes::new()
            .with(Mutation::CreateEpic {
                epic: make_epic("fn-1-test"),
            })
            .with(Mutation::SetTaskSpec {
                task_id: "fn-1-test.1".into(),
                content: "# Spec".into(),
            });
        let json = serde_json::to_string(&c).unwrap();
        let back: Changes = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
    }

    #[test]
    fn entity_id_extraction() {
        let epic = make_epic("fn-1-test");
        let m = Mutation::UpdateEpic {
            epic: epic.clone(),
        };
        assert_eq!(m.entity_id(), "fn-1-test");
        assert_eq!(m.epic_id(), Some("fn-1-test"));

        let m2 = Mutation::RemoveTask {
            id: "fn-1-test.3".into(),
        };
        assert_eq!(m2.entity_id(), "fn-1-test.3");
        assert_eq!(m2.epic_id(), None); // cannot derive without context
    }
}
