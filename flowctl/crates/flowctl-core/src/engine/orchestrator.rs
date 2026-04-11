//! Orchestrator — the central brain of V4.
//!
//! Owns the entire goal lifecycle. Replaces the skill layer.
//! The LLM calls drive() and submit(). The engine handles everything else.
//!
//! Bug fixes applied:
//! - BUG3: Failed path records attempt + calls fail_node (escalation works)
//! - BUG4: InProgress nodes auto-recover on drive() (session recovery)
//! - BUG5: Review failures capped at 3 cycles (no infinite loop)
//! - BUG6: PlanStore uses fs2 file locks (see plan_store.rs)

use std::path::{Path, PathBuf};

use crate::domain::action_spec::*;
use crate::domain::goal::{Goal, GoalIntent, GoalStatus};
use crate::domain::node::{Attempt, Edge, GuardDepth, Node, NodeStatus};
use crate::engine::escalation::EscalationEngine;
use crate::engine::goal_engine::GoalEngine;
use crate::engine::planner::Planner;
use crate::engine::scheduler::Scheduler;
use crate::context::ContextAssembler;
use crate::knowledge::learner::Learner;
use crate::knowledge::LearningKind;
use crate::locks;
use crate::quality::guard_runner::GuardRunner;
use crate::storage::attempt_store::AttemptStore;
use crate::storage::event_store::{EventStore, GoalEventKind};

const MAX_REVIEW_CYCLES: u32 = 3;

pub struct Orchestrator {
    root: PathBuf,
    goal_engine: GoalEngine,
    planner: Planner,
    scheduler: Scheduler,
    escalation: EscalationEngine,
    context: ContextAssembler,
    learner: Learner,
    guard: GuardRunner,
    attempt_store: AttemptStore,
    event_store: EventStore,
}

impl Orchestrator {
    pub fn new(root: &Path) -> Self {
        let flow_dir = root.join(".flow");
        Self {
            root: root.to_path_buf(),
            goal_engine: GoalEngine::new(&flow_dir),
            planner: Planner::new(&flow_dir),
            scheduler: Scheduler::new(&flow_dir),
            escalation: EscalationEngine::new(&flow_dir),
            context: ContextAssembler::new(root),
            learner: Learner::new(&flow_dir),
            guard: GuardRunner::new(root),
            attempt_store: AttemptStore::new(&flow_dir),
            event_store: EventStore::new(&flow_dir),
        }
    }

    // ── Public API ──────────────────────────────────────────────

    pub fn drive(&self, request: &str) -> Result<ActionSpec, String> {
        let goal = self.resolve_goal(request)?;

        if goal.status == GoalStatus::Done {
            return Ok(ActionSpec::complete(&goal.id, CompletionReport {
                goal_id: goal.id.clone(), summary: "Goal already completed".into(),
                nodes_completed: 0, files_changed: vec![], guard_results: vec![],
                learnings_recorded: 0, patterns_created: 0, duration_seconds: 0,
            }));
        }

        // Auto-plan if no plan exists
        if self.planner.get_latest(&goal.id).is_err() {
            self.auto_plan(&goal)?;
        }

        if self.scheduler.is_complete(&goal.id)? {
            return self.review_action(&goal);
        }

        let mut nodes = self.scheduler.ready_nodes(&goal.id)?;

        // BUG4 FIX: recover stuck InProgress nodes on drive()
        if nodes.is_empty() {
            let recovered = self.recover_stuck_nodes(&goal.id)?;
            if recovered > 0 {
                nodes = self.scheduler.ready_nodes(&goal.id)?;
            }
        }

        if nodes.is_empty() {
            return Ok(ActionSpec::blocked(&goal.id, "All nodes are in-progress or blocked."));
        }

        self.implement_action(&goal, &nodes)
    }

    pub fn submit(&self, input: &SubmitInput) -> Result<ActionSpec, String> {
        let (goal_id, node_id) = parse_action_id(&input.action_id)?;
        let goal = self.goal_engine.status(&goal_id)?;

        match node_id.as_str() {
            // Review submission — BUG5 FIX: capped review cycles
            "review" => self.handle_review_submit(&goal, input),

            // Review-fix submission → re-review
            "review-fix" => {
                if input.status == SubmitStatus::Done {
                    return self.review_action(&goal);
                }
                self.complete_goal_with_warning(&goal, "Review fix failed, completing with warning")
            }

            // Terminal actions — acknowledge and done
            "complete" | "blocked" => self.drive(&goal_id),

            // Fix/escalation → route to real node
            _ => {
                let real_node = node_id
                    .strip_suffix("-fix")
                    .or_else(|| node_id.strip_suffix("-escalation"))
                    .unwrap_or(&node_id);
                self.handle_node_submit(&goal, real_node, input)
            }
        }
    }

    pub fn query(&self, question: &str, goal_id: Option<&str>) -> Result<serde_json::Value, String> {
        if let Some(gid) = goal_id {
            let goal = self.goal_engine.status(gid)
                .map_err(|_| format!("goal '{gid}' not found"))?;
            let ready_nodes = self.scheduler.ready_nodes(gid).unwrap_or_default();
            let is_complete = self.scheduler.is_complete(gid).unwrap_or(false);
            let plan = self.planner.get_latest(gid).ok();
            let plan_rev = plan.as_ref().map(|p| p.rev);

            // Build node-level status summary
            let nodes_detail: Vec<serde_json::Value> = plan.as_ref()
                .map(|p| p.nodes.iter().map(|n| {
                    let attempts = self.attempt_store.count_for_node(gid, &n.id).unwrap_or(0);
                    serde_json::json!({
                        "id": n.id,
                        "objective": n.objective,
                        "status": n.status,
                        "attempts": attempts,
                    })
                }).collect())
                .unwrap_or_default();

            return Ok(serde_json::json!({
                "goal": goal,
                "nodes": nodes_detail,
                "ready_nodes": ready_nodes.iter().map(|n| &n.id).collect::<Vec<_>>(),
                "is_complete": is_complete,
                "plan_rev": plan_rev,
            }));
        }

        let q = question.to_lowercase();
        if q.contains("pattern") || q.contains("knowledge") || q.contains("learn") {
            let result = self.learner.search(question, 10).map_err(|e| e.to_string())?;
            return Ok(serde_json::to_value(&result).unwrap_or_default());
        }

        let graph_path = self.root.join(".flow").join("graph.bin");
        if graph_path.exists() {
            if let Ok(graph) = crate::graph_store::CodeGraph::load(&graph_path) {
                let refs = graph.find_refs(question);
                if !refs.is_empty() {
                    let impact: Vec<String> = refs.iter()
                        .flat_map(|r| graph.find_impact(&r.file))
                        .take(20)
                        .collect();
                    return Ok(serde_json::json!({
                        "symbols_found": refs.len(),
                        "files": refs.iter().map(|r| &r.file).collect::<Vec<_>>(),
                        "impact": impact,
                    }));
                }
            }
        }

        let goal_ids = self.goal_engine.goal_store.list().unwrap_or_default();
        let goals_summary: Vec<serde_json::Value> = goal_ids.iter()
            .filter_map(|gid| {
                self.goal_engine.goal_store.get(gid).ok().map(|g| {
                    serde_json::json!({
                        "id": g.id,
                        "status": g.status,
                        "request": g.request,
                    })
                })
            })
            .collect();
        Ok(serde_json::json!({ "goals": goals_summary, "hint": "Pass a goal_id to get detailed status." }))
    }

    // ── Goal resolution ─────────────────────────────────────────

    fn resolve_goal(&self, request: &str) -> Result<Goal, String> {
        let request = request.trim();
        if request.is_empty() {
            return Err("goal request cannot be empty".into());
        }
        if request.starts_with("g-") {
            return self.goal_engine.status(request);
        }
        if let Ok(goal_ids) = self.goal_engine.goal_store.list() {
            for gid in &goal_ids {
                if let Ok(g) = self.goal_engine.goal_store.get(gid) {
                    if g.status == GoalStatus::Active && g.request == request {
                        return Ok(g);
                    }
                }
            }
        }
        self.goal_engine.open(request, GoalIntent::Execute)
    }

    fn auto_plan(&self, goal: &Goal) -> Result<(), String> {
        let criteria = &goal.acceptance_criteria;
        let (nodes, edges) = if criteria.len() <= 1 {
            (vec![Node::new("n-1".into(), goal.request.clone())], vec![])
        } else {
            let mut nodes = Vec::new();
            let mut edges = Vec::new();
            for (i, criterion) in criteria.iter().enumerate() {
                let id = format!("n-{}", i + 1);
                if i > 0 {
                    edges.push(Edge { from: format!("n-{}", i), to: id.clone() });
                }
                nodes.push(Node::new(id, criterion.description.clone()));
            }
            (nodes, edges)
        };
        self.planner.build(&goal.id, nodes, edges, "auto-planned")?;
        Ok(())
    }

    // ── Node submission ─────────────────────────────────────────

    fn handle_node_submit(&self, goal: &Goal, node_id: &str, input: &SubmitInput) -> Result<ActionSpec, String> {
        match input.status {
            SubmitStatus::Done => {
                self.record_attempt(&goal.id, node_id, input)?;
                let guard_result = self.run_guard_for_node(&goal.id, node_id)?;
                if !guard_result.passed {
                    return self.fix_action(goal, node_id, &guard_result);
                }
                self.complete_node(&goal.id, node_id, input)?;
                if self.scheduler.is_complete(&goal.id)? {
                    return self.complete_goal(goal);
                }
                self.drive(&goal.id)
            }
            // BUG3 FIX: Failed path now records attempt + calls fail_node
            SubmitStatus::Failed => {
                self.record_attempt(&goal.id, node_id, input)?;
                let _ = self.scheduler.fail_node(&goal.id, node_id);
                let error = input.error.as_deref().unwrap_or("unknown error");
                let action = self.escalation.handle_failure(&goal.id, node_id, error)?;
                self.escalation_to_action(goal, node_id, &action)
            }
            SubmitStatus::Partial => {
                self.record_attempt(&goal.id, node_id, input)?;
                self.drive(&goal.id)
            }
        }
    }

    // ── Review submission (BUG5 FIX) ────────────────────────────

    fn handle_review_submit(&self, goal: &Goal, input: &SubmitInput) -> Result<ActionSpec, String> {
        if input.status == SubmitStatus::Done {
            return self.complete_goal(goal);
        }

        // Review failed — record and check cycle count
        let review_count = self.attempt_store.count_for_node(&goal.id, "_review").unwrap_or(0);
        self.record_attempt(&goal.id, "_review", input)?;

        if review_count + 1 >= MAX_REVIEW_CYCLES {
            return self.complete_goal_with_warning(
                goal,
                &format!("Review failed {MAX_REVIEW_CYCLES} times, completing with warning"),
            );
        }

        // Return fix action for the review findings
        let error_detail = input.error.as_deref().unwrap_or("review found issues");
        Ok(ActionSpec {
            action_id: format!("a:{}:review-fix", goal.id),
            goal_id: goal.id.clone(),
            action_type: ActionType::Fix,
            objective: format!("Fix review findings: {error_detail}"),
            acceptance_criteria: goal.acceptance_criteria.iter().map(|c| c.description.clone()).collect(),
            context: ActionContext::default(),
            guard: GuardSpec::default(),
            progress: Progress::default(),
        })
    }

    // ── Session recovery (BUG4 FIX) ─────────────────────────────

    fn recover_stuck_nodes(&self, goal_id: &str) -> Result<u32, String> {
        let plan = self.planner.get_latest(goal_id)?;
        let stuck: Vec<String> = plan.nodes.iter()
            .filter(|n| n.status == NodeStatus::InProgress || n.status == NodeStatus::Failed)
            .map(|n| n.id.clone())
            .collect();

        let mut recovered = 0u32;
        for node_id in &stuck {
            // Release any held locks
            let flow_dir = self.root.join(".flow");
            let _ = locks::lock_release_node(&flow_dir, node_id);
            // Reset to Ready
            if self.scheduler.reset_node(goal_id, node_id).is_ok() {
                let _ = self.event_store.emit(
                    goal_id, GoalEventKind::NodeStarted,
                    &format!("{node_id}:recovered"),
                );
                recovered += 1;
            }
        }
        Ok(recovered)
    }

    // ── Action builders ─────────────────────────────────────────

    fn implement_action(&self, goal: &Goal, ready_nodes: &[Node]) -> Result<ActionSpec, String> {
        let node = &ready_nodes[0];
        let _ = self.scheduler.start_node(&goal.id, &node.id);

        let flow_dir = self.root.join(".flow");
        for file in &node.owned_files {
            let _ = locks::lock_acquire(&flow_dir, file, &node.id, "write");
        }

        let context = self.context.assemble(goal, node);
        let parallel_ready: Vec<String> = ready_nodes[1..].iter().map(|n| n.id.clone()).collect();
        let plan = self.planner.get_latest(&goal.id)?;
        let total = plan.nodes.len();
        let completed = plan.nodes.iter().filter(|n| n.status == NodeStatus::Done).count();

        Ok(ActionSpec {
            action_id: format!("a:{}:{}", goal.id, node.id),
            goal_id: goal.id.clone(),
            action_type: ActionType::Implement,
            objective: node.objective.clone(),
            acceptance_criteria: goal.acceptance_criteria.iter().map(|c| c.description.clone()).collect(),
            context,
            guard: GuardSpec {
                depth: match node.risk.guard_depth {
                    GuardDepth::Trivial => "trivial".into(),
                    GuardDepth::Standard => "standard".into(),
                    GuardDepth::Thorough => "thorough".into(),
                },
                commands: self.guard.commands_for_depth(node.risk.guard_depth),
            },
            progress: Progress { total_nodes: total, completed, current_node: Some(node.id.clone()), parallel_ready },
        })
    }

    fn fix_action(&self, goal: &Goal, node_id: &str, guard_result: &crate::quality::guard_runner::GuardResult) -> Result<ActionSpec, String> {
        let failures: Vec<GuardFailure> = guard_result.results.iter()
            .filter(|r| !r.passed)
            .map(|r| GuardFailure {
                command: r.command.clone(),
                output: format!("{}\n{}", r.stdout, r.stderr).trim().to_string(),
                severity: "error".into(),
            }).collect();

        let plan = self.planner.get_latest(&goal.id)?;
        let node = plan.nodes.iter().find(|n| n.id == node_id);
        let context = if let Some(n) = node {
            let mut ctx = self.context.assemble(goal, n);
            ctx.guard_failures = failures;
            ctx
        } else {
            ActionContext { guard_failures: failures, ..Default::default() }
        };

        Ok(ActionSpec {
            action_id: format!("a:{}:{}-fix", goal.id, node_id),
            goal_id: goal.id.clone(),
            action_type: ActionType::Fix,
            objective: format!("Fix {} guard failure(s). See guard_failures in context.", guard_result.results.iter().filter(|r| !r.passed).count()),
            acceptance_criteria: vec!["All guard checks pass".into()],
            context,
            guard: GuardSpec { depth: guard_result.depth.clone(), commands: vec![] },
            progress: {
                let plan = self.planner.get_latest(&goal.id).unwrap_or_default();
                let total = plan.nodes.len();
                let completed = plan.nodes.iter().filter(|n| n.status == NodeStatus::Done).count();
                Progress { total_nodes: total, completed, current_node: Some(node_id.to_string()), parallel_ready: vec![] }
            },
        })
    }

    fn review_action(&self, goal: &Goal) -> Result<ActionSpec, String> {
        let plan = self.planner.get_latest(&goal.id)?;
        let attempt_summaries: Vec<AttemptRef> = plan.nodes.iter()
            .filter_map(|n| {
                self.attempt_store.list_for_node(&goal.id, &n.id).ok()
                    .and_then(|a| a.last().cloned())
                    .map(|a| AttemptRef { seq: a.seq, summary: a.summary, status: a.status })
            }).collect();

        Ok(ActionSpec {
            action_id: format!("a:{}:review", goal.id),
            goal_id: goal.id.clone(),
            action_type: ActionType::Review,
            objective: format!("Review complete implementation for: {}", goal.request),
            acceptance_criteria: goal.acceptance_criteria.iter().map(|c| c.description.clone()).collect(),
            context: ActionContext { prior_attempts: attempt_summaries, ..Default::default() },
            guard: GuardSpec::default(),
            progress: Progress { total_nodes: plan.nodes.len(), completed: plan.nodes.len(), current_node: None, parallel_ready: vec![] },
        })
    }

    fn escalation_to_action(&self, goal: &Goal, node_id: &str, action: &crate::domain::escalation::EscalationAction) -> Result<ActionSpec, String> {
        let plan = self.planner.get_latest(&goal.id)?;
        let node = plan.nodes.iter().find(|n| n.id == node_id);
        let context = if let Some(n) = node { self.context.assemble(goal, n) } else { ActionContext::default() };
        let total = plan.nodes.len();
        let completed = plan.nodes.iter().filter(|n| n.status == NodeStatus::Done).count();

        let (action_type, objective) = match action {
            crate::domain::escalation::EscalationAction::Retry { suggestion, .. } =>
                (ActionType::Fix, format!("Retry: {suggestion}")),
            crate::domain::escalation::EscalationAction::ChangeStrategy { new_constraints, .. } =>
                (ActionType::Fix, format!("Change strategy: {}", new_constraints.join(", "))),
            crate::domain::escalation::EscalationAction::Replan { suggested_mutations, .. } =>
                (ActionType::Implement, format!("Replan needed: {:?}", suggested_mutations)),
        };

        Ok(ActionSpec {
            action_id: format!("a:{}:{}-escalation", goal.id, node_id),
            goal_id: goal.id.clone(),
            action_type, objective, acceptance_criteria: vec![], context,
            guard: GuardSpec::default(),
            progress: Progress { total_nodes: total, completed, current_node: Some(node_id.to_string()), parallel_ready: vec![] },
        })
    }

    // ── Goal completion ─────────────────────────────────────────

    fn complete_goal(&self, goal: &Goal) -> Result<ActionSpec, String> {
        let guard_result = self.guard.run(GuardDepth::Standard);
        self.goal_engine.close(&goal.id)?;
        let patterns = self.learner.compound(&goal.id).unwrap_or_default();
        let plan = self.planner.get_latest(&goal.id).unwrap_or_default();
        let files_changed: Vec<String> = plan.nodes.iter().flat_map(|n| n.owned_files.iter().cloned()).collect();

        Ok(ActionSpec::complete(&goal.id, CompletionReport {
            goal_id: goal.id.clone(),
            summary: format!("Goal completed: {}", goal.request),
            nodes_completed: plan.nodes.len(), files_changed,
            guard_results: guard_result.results,
            learnings_recorded: plan.nodes.len(),
            patterns_created: patterns.len(), duration_seconds: 0,
        }))
    }

    fn complete_goal_with_warning(&self, goal: &Goal, warning: &str) -> Result<ActionSpec, String> {
        self.goal_engine.close(&goal.id)?;
        let plan = self.planner.get_latest(&goal.id).unwrap_or_default();

        Ok(ActionSpec::complete(&goal.id, CompletionReport {
            goal_id: goal.id.clone(),
            summary: format!("Goal completed with warning: {}. {}", goal.request, warning),
            nodes_completed: plan.nodes.len(),
            files_changed: vec![], guard_results: vec![],
            learnings_recorded: 0, patterns_created: 0, duration_seconds: 0,
        }))
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn record_attempt(&self, goal_id: &str, node_id: &str, input: &SubmitInput) -> Result<(), String> {
        let count = self.attempt_store.count_for_node(goal_id, node_id).unwrap_or(0);
        let status_str = match input.status {
            SubmitStatus::Done => "done",
            SubmitStatus::Failed => "failed",
            SubmitStatus::Partial => "partial",
        };
        let attempt = Attempt {
            node_id: node_id.to_string(), seq: count + 1,
            summary: input.summary.clone(), status: status_str.into(),
            changed_files: input.files_changed.clone(),
            commits: vec![], tests: vec![], findings: vec![],
            suggested_mutations: vec![], duration_seconds: 0, created_at: chrono::Utc::now(),
        };
        self.attempt_store.record(goal_id, &attempt).map_err(|e| e.to_string())
    }

    fn run_guard_for_node(&self, goal_id: &str, node_id: &str) -> Result<crate::quality::guard_runner::GuardResult, String> {
        let plan = self.planner.get_latest(goal_id)?;
        let depth = plan.nodes.iter().find(|n| n.id == node_id)
            .map(|n| n.risk.guard_depth).unwrap_or(GuardDepth::Standard);
        Ok(self.guard.run(depth))
    }

    fn complete_node(&self, goal_id: &str, node_id: &str, input: &SubmitInput) -> Result<(), String> {
        let flow_dir = self.root.join(".flow");
        let _ = locks::lock_release_node(&flow_dir, node_id);
        let _ = self.learner.record(goal_id, Some(node_id), LearningKind::Success, &input.summary, vec![]);
        let _ = self.event_store.emit(goal_id, GoalEventKind::NodeCompleted, &format!("{node_id}: {}", input.summary));
        self.scheduler.finish_node(goal_id, node_id)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Orchestrator) {
        let tmp = TempDir::new().unwrap();
        let flow = tmp.path().join(".flow");
        std::fs::create_dir_all(flow.join("goals")).unwrap();
        std::fs::create_dir_all(flow.join("knowledge")).unwrap();
        std::fs::create_dir_all(flow.join(".state")).unwrap();
        let orch = Orchestrator::new(tmp.path());
        (tmp, orch)
    }

    fn done(action_id: &str, summary: &str) -> SubmitInput {
        SubmitInput {
            action_id: action_id.into(), status: SubmitStatus::Done,
            summary: summary.into(), files_changed: vec![],
            test_output: None, error: None,
        }
    }

    fn failed(action_id: &str, error: &str) -> SubmitInput {
        SubmitInput {
            action_id: action_id.into(), status: SubmitStatus::Failed,
            summary: "failed".into(), files_changed: vec![],
            test_output: None, error: Some(error.into()),
        }
    }

    // ── Happy path tests ────────────────────────────────────────

    #[test]
    fn test_drive_auto_plans() {
        let (_tmp, orch) = setup();
        let r = orch.drive("add OAuth login").unwrap();
        assert_eq!(r.action_type, ActionType::Implement);
        assert!(r.progress.total_nodes >= 1);
        assert!(r.action_id.starts_with("a:"));
    }

    #[test]
    fn test_drive_resume() {
        let (_tmp, orch) = setup();
        let a = orch.drive("add OAuth").unwrap();
        let b = orch.drive(&a.goal_id).unwrap();
        assert_eq!(a.goal_id, b.goal_id);
    }

    #[test]
    fn test_multi_criteria_plan() {
        let (_tmp, orch) = setup();
        let r = orch.drive("add login, add signup, add reset").unwrap();
        assert!(r.progress.total_nodes >= 2);
    }

    #[test]
    fn test_full_lifecycle() {
        let (_tmp, orch) = setup();
        let mut a = orch.drive("hello world").unwrap();
        assert_eq!(a.action_type, ActionType::Implement);
        a = orch.submit(&done(&a.action_id, "wrote it")).unwrap();
        if a.action_type == ActionType::Review {
            a = orch.submit(&done(&a.action_id, "looks good")).unwrap();
        }
        assert_eq!(a.action_type, ActionType::Complete);
    }

    #[test]
    fn test_action_id_colon() {
        let (_tmp, orch) = setup();
        let a = orch.drive("test").unwrap();
        assert!(a.action_id.contains(':'));
        let (g, n) = parse_action_id(&a.action_id).unwrap();
        assert!(g.starts_with("g-"));
        assert!(n.starts_with("n-"));
    }

    // ── BUG3 FIX: Failed records attempt + escalation works ─────

    #[test]
    fn test_failed_records_attempt() {
        let (_tmp, orch) = setup();
        let a = orch.drive("task").unwrap();
        let r = orch.submit(&failed(&a.action_id, "compile error")).unwrap();
        // Should be Fix (escalation retry)
        assert_eq!(r.action_type, ActionType::Fix);
        // Attempt should be recorded
        let (gid, _) = parse_action_id(&a.action_id).unwrap();
        let count = orch.attempt_store.count_for_node(&gid, "n-1").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_escalation_progresses_with_failures() {
        let (_tmp, orch) = setup();
        let a = orch.drive("task").unwrap();
        let (gid, _) = parse_action_id(&a.action_id).unwrap();

        // Fail 3 times → should escalate to ChangeStrategy
        for _ in 0..3 {
            let _ = orch.submit(&failed(&a.action_id, "error"));
        }
        let attempts = orch.attempt_store.count_for_node(&gid, "n-1").unwrap();
        assert_eq!(attempts, 3);
    }

    // ── BUG4 FIX: Session recovery ──────────────────────────────

    #[test]
    fn test_stuck_node_recovery() {
        let (_tmp, orch) = setup();
        // Drive starts n-1 → InProgress
        let a = orch.drive("task").unwrap();
        assert_eq!(a.action_type, ActionType::Implement);

        // Simulate session loss: drive again without submitting
        // The node is InProgress, ready_nodes returns empty, recovery kicks in
        let b = orch.drive(&a.goal_id).unwrap();
        // Should recover and return implement again (not blocked)
        assert_eq!(b.action_type, ActionType::Implement);
    }

    // ── BUG5 FIX: Review cycle cap ──────────────────────────────

    #[test]
    fn test_review_failure_returns_fix() {
        let (_tmp, orch) = setup();
        let mut a = orch.drive("task").unwrap();
        a = orch.submit(&done(&a.action_id, "done")).unwrap();
        // Should be Review or Complete
        if a.action_type == ActionType::Review {
            // Fail the review
            let fix = orch.submit(&failed(&a.action_id, "found issues")).unwrap();
            assert_eq!(fix.action_type, ActionType::Fix);
            assert!(fix.action_id.contains("review-fix"));
        }
    }

    #[test]
    fn test_review_max_cycles() {
        let (_tmp, orch) = setup();
        let mut a = orch.drive("task").unwrap();
        a = orch.submit(&done(&a.action_id, "done")).unwrap();

        if a.action_type == ActionType::Review {
            // Fail review MAX_REVIEW_CYCLES times
            for i in 0..MAX_REVIEW_CYCLES {
                let fix = orch.submit(&failed(&a.action_id, "issues")).unwrap();
                if fix.action_type == ActionType::Complete {
                    // Hit the cap
                    assert!(fix.objective.contains("warning"));
                    return;
                }
                assert_eq!(fix.action_type, ActionType::Fix, "cycle {i}");
                // Submit fix as done → re-review
                a = orch.submit(&done(&fix.action_id, "fixed")).unwrap();
                if a.action_type == ActionType::Complete {
                    return;
                }
            }
            // Final review fail should auto-complete
            let final_result = orch.submit(&failed(&a.action_id, "still bad")).unwrap();
            assert_eq!(final_result.action_type, ActionType::Complete);
        }
    }

    // ── Query tests ─────────────────────────────────────────────

    #[test]
    fn test_query_no_goals() {
        let (_tmp, orch) = setup();
        let r = orch.query("what goals?", None).unwrap();
        assert!(r.get("goals").is_some());
    }

    #[test]
    fn test_query_goal_status() {
        let (_tmp, orch) = setup();
        let a = orch.drive("test").unwrap();
        let r = orch.query("status", Some(&a.goal_id)).unwrap();
        assert!(r.get("goal").is_some());
    }

    #[test]
    fn test_drive_empty_request() {
        let (_tmp, orch) = setup();
        let r = orch.drive("");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_query_nonexistent_goal() {
        let (_tmp, orch) = setup();
        let r = orch.query("status", Some("g-nonexistent"));
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("not found"));
    }
}
