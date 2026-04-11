//! Escalation engine — wraps domain escalation with storage integration.

use std::path::Path;

use crate::domain::escalation::{EscalationAction, EscalationLevel, GraphMutation, determine_escalation};
use crate::domain::node::Node;
use crate::storage::attempt_store::AttemptStore;
use crate::storage::event_store::{EventStore, GoalEventKind};

/// Escalation engine with storage awareness.
pub struct EscalationEngine {
    pub attempt_store: AttemptStore,
    pub event_store: EventStore,
}

impl EscalationEngine {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            attempt_store: AttemptStore::new(flow_root),
            event_store: EventStore::new(flow_root),
        }
    }

    /// Handle a node failure. Returns escalation action based on attempt history.
    pub fn handle_failure(
        &self,
        goal_id: &str,
        node_id: &str,
        error: &str,
    ) -> Result<EscalationAction, String> {
        let attempts = self.attempt_store.list_for_node(goal_id, node_id)?;
        let fail_count = attempts.len() as u32;
        let level = determine_escalation(fail_count);

        self.event_store.emit(
            goal_id,
            GoalEventKind::EscalationTriggered,
            &format!("{node_id}: {level:?} (attempt {fail_count})"),
        )?;

        match level {
            EscalationLevel::None | EscalationLevel::WorkerRetry => {
                Ok(EscalationAction::Retry {
                    node_id: node_id.to_string(),
                    suggestion: format!("Try a different approach. Previous error: {error}"),
                })
            }
            EscalationLevel::StrategyChange => {
                Ok(EscalationAction::ChangeStrategy {
                    node_id: node_id.to_string(),
                    new_constraints: vec![format!("Avoid approach that caused: {error}")],
                    catalog_update: Some("Review and modify action catalog".into()),
                })
            }
            EscalationLevel::Replan => {
                Ok(EscalationAction::Replan {
                    affected_nodes: vec![node_id.to_string()],
                    suggested_mutations: vec![GraphMutation::SplitNode {
                        id: node_id.to_string(),
                        into: vec![
                            Node::new(format!("{node_id}-a"), format!("investigate: {error}")),
                            Node::new(format!("{node_id}-b"), "retry after investigation".into()),
                        ],
                        chain: true,
                    }],
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::Attempt;
    use chrono::Utc;
    use tempfile::TempDir;

    fn setup() -> (TempDir, EscalationEngine) {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let engine = EscalationEngine::new(tmp.path());
        (tmp, engine)
    }

    fn make_attempt(node_id: &str, seq: u32) -> Attempt {
        Attempt {
            node_id: node_id.into(),
            seq,
            summary: "failed".into(),
            changed_files: vec![],
            commits: vec![],
            tests: vec![],
            findings: vec![],
            suggested_mutations: vec![],
            duration_seconds: 10,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_first_failure_retries() {
        let (_tmp, engine) = setup();
        let action = engine.handle_failure("g-1", "n-1", "test error").unwrap();
        assert!(matches!(action, EscalationAction::Retry { .. }));
    }

    #[test]
    fn test_repeated_failure_changes_strategy() {
        let (_tmp, engine) = setup();
        // Record 3 prior attempts
        for i in 1..=3 {
            engine.attempt_store.record("g-1", &make_attempt("n-1", i)).unwrap();
        }
        let action = engine.handle_failure("g-1", "n-1", "persistent error").unwrap();
        assert!(matches!(action, EscalationAction::ChangeStrategy { .. }));
    }

    #[test]
    fn test_many_failures_replans() {
        let (_tmp, engine) = setup();
        for i in 1..=5 {
            engine.attempt_store.record("g-1", &make_attempt("n-1", i)).unwrap();
        }
        let action = engine.handle_failure("g-1", "n-1", "unsolvable").unwrap();
        assert!(matches!(action, EscalationAction::Replan { .. }));
    }
}
