//! ActionSpec — the structured protocol between engine and LLM.
//!
//! V4: Engine returns self-contained work packages. LLM is stateless.
//! Action IDs use colon separator: "a:{goal_id}:{node_id}"

use serde::{Deserialize, Serialize};

/// What the engine tells the LLM to do next.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpec {
    pub action_id: String,
    pub goal_id: String,
    pub action_type: ActionType,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub context: ActionContext,
    pub guard: GuardSpec,
    pub progress: Progress,
    /// Recommended workflow steps. Each step has a phase, tool, and reason.
    /// Guides the LLM through the optimal tool sequence for this action.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_workflow: Vec<WorkflowStep>,
}

/// The type of action the LLM should perform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    Implement,
    Review,
    Fix,
    Test,
    Complete,
    Blocked,
}

/// Everything the LLM needs to work on an action.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionContext {
    pub files: Vec<FileSlice>,
    pub patterns: Vec<PatternRef>,
    pub constraints: Vec<String>,
    pub related_symbols: Vec<SymbolRef>,
    pub prior_attempts: Vec<AttemptRef>,
    pub guard_failures: Vec<GuardFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSlice {
    pub path: String,
    /// Summary: first few lines or structure signature. Use flow_query to get full content.
    pub content: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<(usize, usize)>,
    /// Total lines in file — helps LLM judge complexity without reading full content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRef {
    pub name: String,
    pub approach: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolRef {
    pub name: String,
    pub file: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptRef {
    pub seq: u32,
    pub summary: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardFailure {
    pub command: String,
    pub output: String,
    pub severity: String,
}

/// A step in the recommended workflow for an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Phase: "understand" (before coding), "implement" (during), "verify" (after).
    pub phase: String,
    /// The MCP tool to call.
    pub tool: String,
    /// Why this step matters.
    pub reason: String,
    /// Suggested parameters as a JSON-like hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuardSpec {
    pub depth: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub total_nodes: usize,
    pub completed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_node: Option<String>,
    pub parallel_ready: Vec<String>,
}

/// What the LLM submits back to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitInput {
    pub action_id: String,
    pub status: SubmitStatus,
    pub summary: String,
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubmitStatus {
    Done,
    Failed,
    Partial,
}

/// Final report when a goal completes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReport {
    pub goal_id: String,
    pub summary: String,
    pub nodes_completed: usize,
    pub files_changed: Vec<String>,
    pub guard_results: Vec<GuardCommandResult>,
    pub learnings_recorded: usize,
    pub patterns_created: usize,
    pub duration_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardCommandResult {
    pub command: String,
    pub passed: bool,
    pub stdout: String,
    pub stderr: String,
}

impl ActionSpec {
    pub fn complete(goal_id: &str, report: CompletionReport) -> Self {
        Self {
            action_id: format!("a:{goal_id}:complete"),
            goal_id: goal_id.to_string(),
            action_type: ActionType::Complete,
            objective: report.summary.clone(),
            acceptance_criteria: vec![],
            context: ActionContext::default(),
            guard: GuardSpec::default(),
            progress: Progress {
                total_nodes: report.nodes_completed,
                completed: report.nodes_completed,
                current_node: None,
                parallel_ready: vec![],
            },
            recommended_workflow: vec![],
        }
    }

    pub fn blocked(goal_id: &str, reason: &str) -> Self {
        Self {
            action_id: format!("a:{goal_id}:blocked"),
            goal_id: goal_id.to_string(),
            action_type: ActionType::Blocked,
            objective: reason.to_string(),
            acceptance_criteria: vec![],
            context: ActionContext::default(),
            guard: GuardSpec::default(),
            progress: Progress::default(),
            recommended_workflow: vec![],
        }
    }
}

/// Parse "a:{goal_id}:{node_id}" into (goal_id, node_id).
/// Colon separator avoids collision with slug hyphens.
pub fn parse_action_id(action_id: &str) -> Result<(String, String), String> {
    let stripped = action_id.strip_prefix("a:")
        .ok_or_else(|| format!("invalid action_id '{action_id}': must start with 'a:'"))?;
    match stripped.split_once(':') {
        Some((goal, node)) if !goal.is_empty() && !node.is_empty() => {
            Ok((goal.to_string(), node.to_string()))
        }
        Some(_) => Err(format!("invalid action_id '{action_id}': goal_id and node_id cannot be empty")),
        None => Err(format!("invalid action_id '{action_id}': expected format 'a:{{goal_id}}:{{node_id}}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_action_id_with_node() {
        let (g, n) = parse_action_id("a:g-oauth:n-1").unwrap();
        assert_eq!(g, "g-oauth");
        assert_eq!(n, "n-1");
    }

    #[test]
    fn test_parse_action_id_no_node() {
        let (g, n) = parse_action_id("a:g-oauth:review").unwrap();
        assert_eq!(g, "g-oauth");
        assert_eq!(n, "review");
    }

    #[test]
    fn test_parse_action_id_goal_with_hyphens() {
        let (g, n) = parse_action_id("a:g-add-oauth-login:n-2").unwrap();
        assert_eq!(g, "g-add-oauth-login");
        assert_eq!(n, "n-2");
    }

    #[test]
    fn test_parse_action_id_missing_prefix() {
        assert!(parse_action_id("g-oauth:n-1").is_err());
    }

    #[test]
    fn test_parse_action_id_missing_node() {
        assert!(parse_action_id("a:g-oauth").is_err());
    }

    #[test]
    fn test_parse_action_id_empty_parts() {
        assert!(parse_action_id("a::n-1").is_err());
        assert!(parse_action_id("a:g-oauth:").is_err());
    }

    #[test]
    fn test_action_spec_complete() {
        let report = CompletionReport {
            goal_id: "g-1".into(),
            summary: "done".into(),
            nodes_completed: 3,
            files_changed: vec!["a.rs".into()],
            guard_results: vec![],
            learnings_recorded: 2,
            patterns_created: 1,
            duration_seconds: 100,
        };
        let spec = ActionSpec::complete("g-1", report);
        assert_eq!(spec.action_type, ActionType::Complete);
        assert_eq!(spec.action_id, "a:g-1:complete");
    }
}
