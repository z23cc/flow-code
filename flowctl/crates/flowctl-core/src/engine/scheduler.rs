//! Scheduler — returns ready nodes, manages node status transitions.

use std::path::Path;

use crate::domain::node::{Node, NodeStatus};
use crate::domain::plan::PlanVersion;
use crate::storage::event_store::{EventStore, GoalEventKind};
use crate::storage::plan_store::PlanStore;

/// The scheduler resolves dependencies and returns executable nodes.
pub struct Scheduler {
    pub plan_store: PlanStore,
    pub event_store: EventStore,
}

impl Scheduler {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            plan_store: PlanStore::new(flow_root),
            event_store: EventStore::new(flow_root),
        }
    }

    /// Get currently ready nodes (all deps satisfied, not started).
    pub fn ready_nodes(&self, goal_id: &str) -> Result<Vec<Node>, String> {
        let plan = self.plan_store.get_latest(goal_id)?;
        Ok(find_ready_nodes(&plan))
    }

    /// Mark a node as in-progress.
    pub fn start_node(&self, goal_id: &str, node_id: &str) -> Result<PlanVersion, String> {
        let mut plan = self.plan_store.get_latest(goal_id)?;
        let node = plan.nodes.iter_mut().find(|n| n.id == node_id)
            .ok_or_else(|| format!("node {node_id} not found"))?;

        if node.status != NodeStatus::Ready {
            return Err(format!("node {node_id} is {:?}, not Ready", node.status));
        }
        node.status = NodeStatus::InProgress;

        // Save as new version (status update)
        plan.rev = self.plan_store.next_rev(goal_id)?;
        self.plan_store.create_version(&plan)?;
        self.event_store.emit(goal_id, GoalEventKind::NodeStarted, node_id)?;
        Ok(plan)
    }

    /// Mark a node as done. Returns newly unblocked nodes.
    pub fn finish_node(&self, goal_id: &str, node_id: &str) -> Result<Vec<Node>, String> {
        let mut plan = self.plan_store.get_latest(goal_id)?;
        let node = plan.nodes.iter_mut().find(|n| n.id == node_id)
            .ok_or_else(|| format!("node {node_id} not found"))?;

        node.status = NodeStatus::Done;

        plan.rev = self.plan_store.next_rev(goal_id)?;
        self.plan_store.create_version(&plan)?;
        self.event_store.emit(goal_id, GoalEventKind::NodeCompleted, node_id)?;

        Ok(find_ready_nodes(&plan))
    }

    /// Mark a node as failed.
    pub fn fail_node(&self, goal_id: &str, node_id: &str) -> Result<PlanVersion, String> {
        let mut plan = self.plan_store.get_latest(goal_id)?;
        let node = plan.nodes.iter_mut().find(|n| n.id == node_id)
            .ok_or_else(|| format!("node {node_id} not found"))?;

        node.status = NodeStatus::Failed;

        plan.rev = self.plan_store.next_rev(goal_id)?;
        self.plan_store.create_version(&plan)?;
        self.event_store.emit(goal_id, GoalEventKind::NodeFailed, node_id)?;
        Ok(plan)
    }

    /// Check if all nodes are terminal (done/skipped/failed).
    pub fn is_complete(&self, goal_id: &str) -> Result<bool, String> {
        let plan = self.plan_store.get_latest(goal_id)?;
        Ok(plan.nodes.iter().all(|n| n.status.is_terminal() || n.status == NodeStatus::Failed))
    }

    /// Reset an InProgress node back to Ready (for session recovery).
    pub fn reset_node(&self, goal_id: &str, node_id: &str) -> Result<PlanVersion, String> {
        let mut plan = self.plan_store.get_latest(goal_id)?;
        let node = plan.nodes.iter_mut().find(|n| n.id == node_id)
            .ok_or_else(|| format!("node {node_id} not found"))?;

        if node.status != NodeStatus::InProgress && node.status != NodeStatus::Failed {
            return Err(format!("node {node_id} is {:?}, expected InProgress or Failed", node.status));
        }
        node.status = NodeStatus::Ready;

        plan.rev = self.plan_store.next_rev(goal_id)?;
        self.plan_store.create_version(&plan)?;
        self.event_store.emit(goal_id, GoalEventKind::NodeStarted, &format!("{node_id}:reset"))?;
        Ok(plan)
    }
}

/// Find nodes that are Ready and have all dependencies satisfied.
fn find_ready_nodes(plan: &PlanVersion) -> Vec<Node> {
    let mut ready = Vec::new();
    for node in &plan.nodes {
        if node.status != NodeStatus::Ready {
            continue;
        }
        // Check if all dependencies are satisfied
        let deps_satisfied = plan.edges.iter()
            .filter(|e| e.to == node.id)
            .all(|e| {
                plan.nodes.iter()
                    .find(|n| n.id == e.from)
                    .map_or(true, |dep| dep.status.is_satisfied())
            });
        if deps_satisfied {
            ready.push(node.clone());
        }
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::Edge;
    use crate::engine::Planner;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Planner, Scheduler) {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("goals").join("g-1")).unwrap();
        let planner = Planner::new(tmp.path());
        let scheduler = Scheduler::new(tmp.path());
        (tmp, planner, scheduler)
    }

    #[test]
    fn test_ready_nodes_initial() {
        let (_tmp, planner, scheduler) = setup();
        let nodes = vec![
            Node::new("n-1".into(), "first".into()),
            Node::new("n-2".into(), "second".into()),
        ];
        let edges = vec![Edge { from: "n-1".into(), to: "n-2".into() }];
        planner.build(&"g-1".into(), nodes, edges, "test").unwrap();

        let ready = scheduler.ready_nodes("g-1").unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "n-1");
    }

    #[test]
    fn test_start_and_finish() {
        let (_tmp, planner, scheduler) = setup();
        let nodes = vec![
            Node::new("n-1".into(), "first".into()),
            Node::new("n-2".into(), "second".into()),
        ];
        let edges = vec![Edge { from: "n-1".into(), to: "n-2".into() }];
        planner.build(&"g-1".into(), nodes, edges, "test").unwrap();

        scheduler.start_node("g-1", "n-1").unwrap();
        let newly_ready = scheduler.finish_node("g-1", "n-1").unwrap();
        assert_eq!(newly_ready.len(), 1);
        assert_eq!(newly_ready[0].id, "n-2");
    }

    #[test]
    fn test_is_complete() {
        let (_tmp, planner, scheduler) = setup();
        let nodes = vec![Node::new("n-1".into(), "only".into())];
        planner.build(&"g-1".into(), nodes, vec![], "test").unwrap();

        assert!(!scheduler.is_complete("g-1").unwrap());
        scheduler.start_node("g-1", "n-1").unwrap();
        scheduler.finish_node("g-1", "n-1").unwrap();
        assert!(scheduler.is_complete("g-1").unwrap());
    }
}
