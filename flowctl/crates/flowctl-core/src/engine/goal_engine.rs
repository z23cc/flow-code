//! GoalEngine — assess, open, close, status.

use std::path::Path;

use crate::domain::goal::*;
use crate::provider::ProviderRegistry;
use crate::storage::event_store::{EventStore, GoalEventKind};
use crate::storage::goal_store::GoalStore;

/// The goal engine manages goal lifecycle.
pub struct GoalEngine {
    pub goal_store: GoalStore,
    pub event_store: EventStore,
    pub providers: ProviderRegistry,
}

impl GoalEngine {
    pub fn new(flow_root: &Path) -> Self {
        Self {
            goal_store: GoalStore::new(flow_root),
            event_store: EventStore::new(flow_root),
            providers: ProviderRegistry::new(),
        }
    }

    /// Assess request and determine PlanningMode × SuccessModel.
    pub fn assess_goal(request: &str) -> (PlanningMode, SuccessModel) {
        let words: Vec<&str> = request.split_whitespace().collect();
        let word_count = words.len();

        let is_trivial = word_count <= 5
            && words.iter().any(|w| {
                let lower = w.to_lowercase();
                ["fix", "typo", "rename", "bump", "simple", "trivial", "config"]
                    .iter()
                    .any(|t| lower.contains(t))
            });

        let has_numeric = request.contains('%')
            || request.contains("coverage")
            || request.contains("lint error")
            || request.contains("score")
            || request.contains("benchmark")
            || request.contains("performance");

        let has_criteria = request.contains("add ")
            || request.contains("implement")
            || request.contains("create")
            || request.contains("build")
            || request.contains("integrate");

        let planning = if is_trivial { PlanningMode::Direct } else { PlanningMode::Graph };

        let success = match (has_numeric, has_criteria) {
            (true, false) => SuccessModel::Numeric,
            (false, _) => SuccessModel::Criteria,
            (true, true) => SuccessModel::Mixed,
        };

        (planning, success)
    }

    /// Open a new goal. Auto-populates fields based on mode selection.
    pub fn open(&self, request: &str, intent: GoalIntent) -> Result<Goal, String> {
        let trimmed = request.trim();
        if trimmed.is_empty() {
            return Err("goal request cannot be empty".into());
        }
        let (planning_mode, success_model) = Self::assess_goal(trimmed);
        let slug = slugify(trimmed);
        let id = format!("g-{slug}");

        let mut goal = Goal::new(id.clone(), request.to_string(), planning_mode, success_model);
        goal.intent = intent;
        goal.status = GoalStatus::Active;

        // Auto-populate fields based on success model
        match success_model {
            SuccessModel::Numeric | SuccessModel::Mixed => {
                // Try to detect a fitness script
                goal.fitness_script = Self::detect_fitness_script(request);
                goal.score_baseline = Some(0.0);
                goal.score_target = Self::extract_target(request);
            }
            SuccessModel::Criteria => {}
        }

        // Generate acceptance criteria from request
        goal.acceptance_criteria = Self::extract_criteria(request);

        self.goal_store.create(&goal)?;
        self.event_store.emit(&id, GoalEventKind::GoalCreated, request)?;

        Ok(goal)
    }

    /// Detect a fitness script from common project patterns.
    fn detect_fitness_script(request: &str) -> Option<String> {
        let lower = request.to_lowercase();
        if lower.contains("coverage") || lower.contains("test") {
            Some("cargo test 2>&1 | tail -1".into())
        } else if lower.contains("lint") || lower.contains("clippy") {
            Some("cargo clippy --message-format json 2>&1 | grep -c 'warning'".into())
        } else if lower.contains("benchmark") || lower.contains("perf") {
            Some("cargo bench 2>&1 | tail -5".into())
        } else {
            None
        }
    }

    /// Extract a numeric target from request text (e.g., "80%" → 80.0).
    fn extract_target(request: &str) -> Option<f64> {
        let re = regex::Regex::new(r"(\d+)%").ok()?;
        re.captures(request)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<f64>().ok())
    }

    /// Extract acceptance criteria from request as individual items.
    fn extract_criteria(request: &str) -> Vec<Criterion> {
        // Split on common separators: "and", ",", ";"
        let parts: Vec<&str> = request
            .split(|c: char| c == ',' || c == ';')
            .flat_map(|s| s.split(" and "))
            .map(str::trim)
            .filter(|s| s.len() > 3)
            .collect();

        if parts.len() <= 1 {
            // Single criterion = the whole request
            vec![Criterion {
                description: request.to_string(),
                met: false,
                evidence: None,
            }]
        } else {
            parts
                .into_iter()
                .map(|p| Criterion {
                    description: p.to_string(),
                    met: false,
                    evidence: None,
                })
                .collect()
        }
    }

    /// Get goal status.
    pub fn status(&self, goal_id: &str) -> Result<Goal, String> {
        self.goal_store.get(goal_id)
    }

    /// Close a goal (mark as done). Runs review provider if configured.
    pub fn close(&self, goal_id: &str) -> Result<Goal, String> {
        let mut goal = self.goal_store.get(goal_id)?;

        // Run review provider if configured on the goal
        if let Some(ref ps) = goal.providers {
            if let Some(ref review_name) = ps.review {
                if let Ok(provider) = self.providers.get_review(review_name) {
                    let _review_result = provider.review("", &goal.request);
                    // Review result is informational at close time — not blocking
                }
            }
        }

        goal.status = GoalStatus::Done;
        for criterion in &mut goal.acceptance_criteria {
            criterion.met = true;
        }
        goal.updated_at = chrono::Utc::now();
        self.goal_store.update(&goal)?;
        self.event_store.emit(goal_id, GoalEventKind::GoalCompleted, "goal completed")?;
        Ok(goal)
    }
}

/// Simple slugification for goal IDs. ASCII-only for cross-platform safety.
fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let parts: Vec<&str> = slug.split('-').filter(|p| !p.is_empty()).collect();
    let truncated: Vec<&str> = parts.into_iter().take(5).collect();
    truncated.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, GoalEngine) {
        let tmp = TempDir::new().unwrap();
        let engine = GoalEngine::new(tmp.path());
        (tmp, engine)
    }

    #[test]
    fn test_assess_trivial() {
        let (pm, sm) = GoalEngine::assess_goal("fix typo");
        assert_eq!(pm, PlanningMode::Direct);
        assert_eq!(sm, SuccessModel::Criteria);
    }

    #[test]
    fn test_assess_numeric() {
        let (pm, sm) = GoalEngine::assess_goal("improve test coverage to 80%");
        assert_eq!(pm, PlanningMode::Graph);
        assert_eq!(sm, SuccessModel::Numeric);
    }

    #[test]
    fn test_assess_criteria() {
        let (pm, sm) = GoalEngine::assess_goal("add OAuth login with Google and GitHub providers");
        assert_eq!(pm, PlanningMode::Graph);
        assert_eq!(sm, SuccessModel::Criteria);
    }

    #[test]
    fn test_assess_mixed() {
        let (pm, sm) = GoalEngine::assess_goal("add tests to improve coverage to 80%");
        assert_eq!(pm, PlanningMode::Graph);
        assert_eq!(sm, SuccessModel::Mixed);
    }

    #[test]
    fn test_open_and_status() {
        let (_tmp, engine) = setup();
        let goal = engine.open("add OAuth login", GoalIntent::Execute).unwrap();
        assert_eq!(goal.status, GoalStatus::Active);
        let loaded = engine.status(&goal.id).unwrap();
        assert_eq!(loaded.request, "add OAuth login");
    }

    #[test]
    fn test_open_and_close() {
        let (_tmp, engine) = setup();
        let goal = engine.open("test goal", GoalIntent::Execute).unwrap();
        let closed = engine.close(&goal.id).unwrap();
        assert_eq!(closed.status, GoalStatus::Done);
    }
}
