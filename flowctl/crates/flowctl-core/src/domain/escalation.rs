//! Three-level escalation control (SWE-AF pattern).
//!
//! L1: Worker retry (change approach, same objective)
//! L2: Strategy change (modify constraints/catalog, no replan)
//! L3: Replan (create new PlanVersion with graph mutations)

use serde::{Deserialize, Serialize};

use super::node::{Node, NodeId};

/// Current escalation level for a goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EscalationLevel {
    #[default]
    None,
    WorkerRetry,
    StrategyChange,
    Replan,
}

/// Action recommended by the escalation engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum EscalationAction {
    Retry {
        node_id: NodeId,
        suggestion: String,
    },
    ChangeStrategy {
        node_id: NodeId,
        new_constraints: Vec<String>,
        catalog_update: Option<String>,
    },
    Replan {
        affected_nodes: Vec<NodeId>,
        suggested_mutations: Vec<GraphMutation>,
    },
}

/// Graph mutation — replan returns suggestions, caller decides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum GraphMutation {
    AddNode {
        node: Node,
        deps: Vec<NodeId>,
    },
    RemoveNode {
        id: NodeId,
    },
    SplitNode {
        id: NodeId,
        into: Vec<Node>,
        #[serde(default)]
        chain: bool,
    },
    SkipNode {
        id: NodeId,
        reason: String,
    },
    AddEdge {
        from: NodeId,
        to: NodeId,
    },
    RemoveEdge {
        from: NodeId,
        to: NodeId,
    },
    UpdateConstraints {
        id: NodeId,
        new_constraints: Vec<String>,
    },
}

/// Determine escalation level based on consecutive failure count.
pub fn determine_escalation(consecutive_failures: u32) -> EscalationLevel {
    match consecutive_failures {
        0 => EscalationLevel::None,
        1..=2 => EscalationLevel::WorkerRetry,
        3..=4 => EscalationLevel::StrategyChange,
        _ => EscalationLevel::Replan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escalation_levels() {
        assert_eq!(determine_escalation(0), EscalationLevel::None);
        assert_eq!(determine_escalation(1), EscalationLevel::WorkerRetry);
        assert_eq!(determine_escalation(2), EscalationLevel::WorkerRetry);
        assert_eq!(determine_escalation(3), EscalationLevel::StrategyChange);
        assert_eq!(determine_escalation(4), EscalationLevel::StrategyChange);
        assert_eq!(determine_escalation(5), EscalationLevel::Replan);
        assert_eq!(determine_escalation(10), EscalationLevel::Replan);
    }
}
