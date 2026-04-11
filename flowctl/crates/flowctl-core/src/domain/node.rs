//! Node + Attempt — execution units in the V3 adaptive graph.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a node.
pub type NodeId = String;

/// A node in the execution graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub objective: String,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub owned_files: Vec<String>,
    pub risk: RiskProfile,
    pub status: NodeStatus,
    #[serde(default)]
    pub injected_patterns: Vec<String>,
}

/// SWE-AF IssueGuidance: risk-proportional annotation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RiskProfile {
    pub estimated_scope: Scope,
    #[serde(default)]
    pub needs_deeper_qa: bool,
    #[serde(default)]
    pub touches_interfaces: bool,
    #[serde(default)]
    pub risk_rationale: String,
    pub guard_depth: GuardDepth,
}

/// Scope estimate for a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Trivial,
    #[default]
    Small,
    Medium,
    Large,
}

/// Guard depth — risk-proportional quality checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GuardDepth {
    /// Lint only.
    Trivial,
    /// Lint + type + test.
    #[default]
    Standard,
    /// Lint + type + test + review.
    Thorough,
}

/// Node lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    #[default]
    Ready,
    InProgress,
    Done,
    Failed,
    Skipped,
}

impl NodeStatus {
    /// Whether this status satisfies downstream dependencies.
    pub fn is_satisfied(&self) -> bool {
        matches!(self, NodeStatus::Done | NodeStatus::Skipped)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, NodeStatus::Done | NodeStatus::Skipped)
    }
}

/// A single attempt at executing a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub node_id: NodeId,
    pub seq: u32,
    pub summary: String,
    /// The submit status: "done", "failed", or "partial".
    #[serde(default = "default_attempt_status")]
    pub status: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default)]
    pub tests: Vec<TestResult>,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub suggested_mutations: Vec<super::escalation::GraphMutation>,
    pub duration_seconds: u32,
    pub created_at: DateTime<Utc>,
}

fn default_attempt_status() -> String {
    "done".into()
}

/// Test result from a node attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A finding (issue) from review or guard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Finding severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    P0,
    P1,
    P2,
    P3,
}

/// Edge in the execution graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
}

impl Node {
    pub fn new(id: NodeId, objective: String) -> Self {
        Self {
            id,
            objective,
            constraints: Vec::new(),
            owned_files: Vec::new(),
            risk: RiskProfile::default(),
            status: NodeStatus::Ready,
            injected_patterns: Vec::new(),
        }
    }
}
