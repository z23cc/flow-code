//! PlanVersion — explicit versioning for replans.
//!
//! Replan creates a new PlanVersion; never mutates existing ones.
//! Rollback = switch to older version. Audit = diff two versions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::goal::GoalId;
use super::node::{Edge, Node};

/// A snapshot of the execution plan. Immutable after creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanVersion {
    pub goal_id: GoalId,
    pub rev: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub rationale: String,
    pub trigger: PlanTrigger,
    pub created_at: DateTime<Utc>,
}

/// What triggered this plan version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PlanTrigger {
    Initial,
    Replan { reason: String },
    ScopeChange { delta: String },
    ScoreRegression { from: f64, to: f64 },
}

impl PlanVersion {
    /// Create an initial plan version.
    pub fn initial(goal_id: GoalId, nodes: Vec<Node>, edges: Vec<Edge>, rationale: String) -> Self {
        Self {
            goal_id,
            rev: 1,
            nodes,
            edges,
            rationale,
            trigger: PlanTrigger::Initial,
            created_at: Utc::now(),
        }
    }

    /// Compute parallel levels via topological sort (Kahn's algorithm).
    pub fn compute_levels(&self) -> Vec<Vec<String>> {
        use std::collections::{HashMap, VecDeque};

        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        for node in &self.nodes {
            in_degree.entry(&node.id).or_insert(0);
            adjacency.entry(&node.id).or_default();
        }

        for edge in &self.edges {
            *in_degree.entry(&edge.to).or_insert(0) += 1;
            adjacency.entry(&edge.from).or_default().push(&edge.to);
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut levels: Vec<Vec<String>> = Vec::new();

        while !queue.is_empty() {
            let level: Vec<String> = queue.drain(..).map(String::from).collect();
            let mut next_queue = VecDeque::new();

            for node_id in &level {
                if let Some(dependents) = adjacency.get(node_id.as_str()) {
                    for &dep in dependents {
                        if let Some(deg) = in_degree.get_mut(dep) {
                            *deg -= 1;
                            if *deg == 0 {
                                next_queue.push_back(dep);
                            }
                        }
                    }
                }
            }

            levels.push(level);
            queue = next_queue;
        }

        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::node::Node;

    #[test]
    fn test_compute_levels_linear() {
        let plan = PlanVersion::initial(
            "g-1".into(),
            vec![
                Node::new("n-1".into(), "first".into()),
                Node::new("n-2".into(), "second".into()),
                Node::new("n-3".into(), "third".into()),
            ],
            vec![
                Edge { from: "n-1".into(), to: "n-2".into() },
                Edge { from: "n-2".into(), to: "n-3".into() },
            ],
            "test".into(),
        );
        let levels = plan.compute_levels();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["n-1"]);
        assert_eq!(levels[1], vec!["n-2"]);
        assert_eq!(levels[2], vec!["n-3"]);
    }

    #[test]
    fn test_compute_levels_parallel() {
        let plan = PlanVersion::initial(
            "g-1".into(),
            vec![
                Node::new("n-1".into(), "root".into()),
                Node::new("n-2".into(), "left".into()),
                Node::new("n-3".into(), "right".into()),
                Node::new("n-4".into(), "join".into()),
            ],
            vec![
                Edge { from: "n-1".into(), to: "n-2".into() },
                Edge { from: "n-1".into(), to: "n-3".into() },
                Edge { from: "n-2".into(), to: "n-4".into() },
                Edge { from: "n-3".into(), to: "n-4".into() },
            ],
            "test".into(),
        );
        let levels = plan.compute_levels();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["n-1"]);
        assert!(levels[1].len() == 2); // n-2 and n-3 in parallel
        assert_eq!(levels[2], vec!["n-4"]);
    }
}
