//! Planner — generates PlanVersion with parallel levels and RiskProfile per node.

use std::path::Path;

use crate::domain::goal::GoalId;
use crate::domain::node::{Edge, GuardDepth, Node, RiskProfile, Scope};
use crate::domain::plan::{PlanTrigger, PlanVersion};
use crate::storage::event_store::{EventStore, GoalEventKind};
use crate::storage::plan_store::PlanStore;

/// The planner generates and stores execution plans.
pub struct Planner {
    pub plan_store: PlanStore,
    pub event_store: EventStore,
}

impl Planner {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            plan_store: PlanStore::new(flow_root),
            event_store: EventStore::new(flow_root),
        }
    }

    /// Build a new plan from a list of nodes and edges.
    /// Assigns RiskProfile to each node based on heuristics.
    pub fn build(
        &self,
        goal_id: &GoalId,
        mut nodes: Vec<Node>,
        edges: Vec<Edge>,
        rationale: &str,
    ) -> Result<PlanVersion, String> {
        // Assign risk profiles based on heuristics
        for node in &mut nodes {
            node.risk = estimate_risk(&node.objective, &node.owned_files);
        }

        let rev = self.plan_store.next_rev(goal_id)?;
        let plan = PlanVersion {
            goal_id: goal_id.clone(),
            rev,
            nodes,
            edges,
            rationale: rationale.to_string(),
            trigger: PlanTrigger::Initial,
            created_at: chrono::Utc::now(),
        };

        self.plan_store.create_version(&plan)?;
        self.event_store.emit(goal_id, GoalEventKind::PlanCreated, &format!("plan v{rev}"))?;

        Ok(plan)
    }

    /// Get the latest plan for a goal.
    pub fn get_latest(&self, goal_id: &str) -> Result<PlanVersion, String> {
        self.plan_store.get_latest(goal_id)
    }
}

/// Heuristic risk estimation based on objective text and file count.
fn estimate_risk(objective: &str, owned_files: &[String]) -> RiskProfile {
    let file_count = owned_files.len();
    let obj_lower = objective.to_lowercase();

    let touches_interfaces = obj_lower.contains("api")
        || obj_lower.contains("interface")
        || obj_lower.contains("schema")
        || obj_lower.contains("migration")
        || obj_lower.contains("protocol");

    let needs_deeper_qa = touches_interfaces
        || obj_lower.contains("security")
        || obj_lower.contains("auth")
        || obj_lower.contains("payment");

    let scope = match file_count {
        0..=1 => Scope::Trivial,
        2..=3 => Scope::Small,
        4..=8 => Scope::Medium,
        _ => Scope::Large,
    };

    let guard_depth = if needs_deeper_qa {
        GuardDepth::Thorough
    } else if file_count <= 2 {
        GuardDepth::Trivial
    } else {
        GuardDepth::Standard
    };

    RiskProfile {
        estimated_scope: scope,
        needs_deeper_qa,
        touches_interfaces,
        risk_rationale: format!(
            "{} files, interfaces={touches_interfaces}, deep_qa={needs_deeper_qa}",
            file_count
        ),
        guard_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_risk_trivial() {
        let risk = estimate_risk("fix typo", &["README.md".into()]);
        assert_eq!(risk.estimated_scope, Scope::Trivial);
        assert_eq!(risk.guard_depth, GuardDepth::Trivial);
        assert!(!risk.needs_deeper_qa);
    }

    #[test]
    fn test_estimate_risk_api() {
        let risk = estimate_risk("update API schema", &["a.rs".into(), "b.rs".into(), "c.rs".into()]);
        assert_eq!(risk.estimated_scope, Scope::Small);
        assert_eq!(risk.guard_depth, GuardDepth::Thorough);
        assert!(risk.touches_interfaces);
    }

    #[test]
    fn test_estimate_risk_large() {
        let files: Vec<String> = (0..10).map(|i| format!("file{i}.rs")).collect();
        let risk = estimate_risk("major refactor", &files);
        assert_eq!(risk.estimated_scope, Scope::Large);
        assert_eq!(risk.guard_depth, GuardDepth::Standard);
    }

    #[test]
    fn test_planner_build() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let planner = Planner::new(tmp.path());
        let nodes = vec![
            Node::new("n-1".into(), "research".into()),
            Node::new("n-2".into(), "implement".into()),
        ];
        let edges = vec![Edge { from: "n-1".into(), to: "n-2".into() }];
        let plan = planner.build(&"g-1".into(), nodes, edges, "initial").unwrap();
        assert_eq!(plan.rev, 1);
        assert_eq!(plan.nodes.len(), 2);
    }
}
