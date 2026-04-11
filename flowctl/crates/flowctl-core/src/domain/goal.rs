//! Goal — the V3 core unit, replacing Epic + Pipeline.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Unique identifier for a goal.
pub type GoalId = String;

/// Goal is the V3 core unit. Orthogonal PlanningMode × SuccessModel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: GoalId,
    pub request: String,
    pub intent: GoalIntent,
    pub planning_mode: PlanningMode,
    pub success_model: SuccessModel,
    pub status: GoalStatus,
    pub current_plan_rev: u32,

    // Numeric mode fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fitness_script: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_baseline: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_target: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_current: Option<f64>,
    #[serde(default)]
    pub action_catalog: Vec<Action>,

    // Criteria mode fields
    #[serde(default)]
    pub acceptance_criteria: Vec<Criterion>,

    // Common fields
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub known_facts: Vec<Fact>,
    #[serde(default)]
    pub open_questions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub providers: Option<ProviderSet>,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User intent — determines how far the pipeline runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalIntent {
    #[default]
    Execute,
    Plan,
    Brainstorm,
}

/// Planning approach — independent of success metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanningMode {
    /// No graph needed. Single-node direct execution (≤2 files).
    Direct,
    /// Generate execution graph with parallel levels.
    #[default]
    Graph,
}

/// Success metric — independent of planning approach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SuccessModel {
    /// All acceptance criteria must be MET.
    #[default]
    Criteria,
    /// Fitness function score must reach target.
    Numeric,
    /// Criteria as gate + score as accelerator.
    Mixed,
}

/// Goal lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[default]
    Open,
    Active,
    Done,
    Failed,
}

/// An action in the action catalog (score-driven mode).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub description: String,
    pub estimated_impact: f64,
    pub risk: Risk,
    #[serde(default)]
    pub tried: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Risk level for actions and tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    #[default]
    Low,
    Medium,
    High,
}

/// Acceptance criterion with pass/fail tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub description: String,
    #[serde(default)]
    pub met: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// A fact discovered during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub discovered_at: DateTime<Utc>,
}

/// Provider bindings for a goal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
}

impl Goal {
    /// Create a new goal with sensible defaults.
    pub fn new(id: GoalId, request: String, planning_mode: PlanningMode, success_model: SuccessModel) -> Self {
        let now = Utc::now();
        Self {
            id,
            request,
            intent: GoalIntent::Execute,
            planning_mode,
            success_model,
            status: GoalStatus::Open,
            current_plan_rev: 0,
            fitness_script: None,
            score_baseline: None,
            score_target: None,
            score_current: None,
            action_catalog: Vec::new(),
            acceptance_criteria: Vec::new(),
            constraints: Vec::new(),
            known_facts: Vec::new(),
            open_questions: Vec::new(),
            providers: None,
            created_at: now,
            updated_at: now,
        }
    }
}
