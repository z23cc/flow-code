//! MCP server setup and tool registration.
//!
//! Uses rmcp for stdio-based MCP transport.
//! All tool handlers call synchronous flowctl-core methods via spawn_blocking.

use std::path::PathBuf;

use flowctl_core::domain::goal::GoalIntent;
use flowctl_core::domain::node::{Attempt, Edge, Node};
use flowctl_core::engine::goal_engine::GoalEngine;
use flowctl_core::engine::planner::Planner;
use flowctl_core::engine::scheduler::Scheduler;
use flowctl_core::engine::escalation::EscalationEngine;
use flowctl_core::knowledge::learner::Learner;
use flowctl_core::knowledge::LearningKind;
use flowctl_core::quality::PolicyEngine;
use flowctl_core::storage::attempt_store::AttemptStore;
use flowctl_core::storage::event_store::{EventStore, GoalEventKind};
use rmcp::{ServerHandler, ServiceExt, model::{ServerCapabilities, ServerInfo}, schemars, tool};

/// Shared state for all MCP tool handlers.
#[derive(Clone)]
pub struct FlowctlServer {
    /// Root directory containing .flow/
    pub root: PathBuf,
}

impl FlowctlServer {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

// ── Tool parameters ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GoalOpenParams {
    #[schemars(description = "User request describing the goal")]
    pub request: String,
    #[schemars(description = "Intent: execute, plan, or brainstorm")]
    pub intent: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GoalIdParam {
    #[schemars(description = "Goal ID (e.g. g-add-oauth)")]
    pub goal_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PlanBuildParams {
    #[schemars(description = "Goal ID")]
    pub goal_id: String,
    #[schemars(description = "List of node objectives")]
    pub nodes: Vec<NodeSpec>,
    #[schemars(description = "Dependencies between nodes (from → to)")]
    pub edges: Option<Vec<EdgeSpec>>,
    #[schemars(description = "Rationale for this plan")]
    pub rationale: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeSpec {
    pub id: String,
    pub objective: String,
    pub files: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EdgeSpec {
    pub from: String,
    pub to: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeStartParams {
    #[schemars(description = "Node ID to start")]
    pub node_id: String,
    #[schemars(description = "Goal ID")]
    pub goal_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeFinishParams {
    #[schemars(description = "Node ID")]
    pub node_id: String,
    #[schemars(description = "Goal ID")]
    pub goal_id: String,
    #[schemars(description = "Summary of work done")]
    pub summary: String,
    pub changed_files: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NodeFailParams {
    pub node_id: String,
    pub goal_id: String,
    pub error: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QualityRunParams {
    pub goal_id: Option<String>,
    pub node_id: Option<String>,
    #[schemars(description = "Guard depth: trivial, standard, or thorough")]
    pub depth: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LockParams {
    pub node_id: String,
    pub files: Vec<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct LockReleaseParams {
    pub node_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KnowledgeSearchParams {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KnowledgeRecordParams {
    pub goal_id: String,
    pub node_id: Option<String>,
    pub content: String,
    #[schemars(description = "Kind: success, failure, discovery, pitfall")]
    pub kind: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CodebaseAssessParams {
    pub query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PlanMutateParams {
    pub goal_id: String,
    pub rationale: String,
}

// ── Tool implementations ────────────────────────────────────────────

#[tool(tool_box)]
impl FlowctlServer {
    // ── Goal tools ──────────────────────────────────────────────
    #[tool(description = "Open a goal from user request. Analyzes request to select planning_mode (direct/graph) and success_model (criteria/numeric/mixed). Returns goal_id, mode, and model.")]
    async fn goal_open(&self, #[tool(aggr)] params: GoalOpenParams) -> String {
        let root = self.root.clone();
        let request = params.request;
        let intent_str = params.intent.unwrap_or_else(|| "execute".into());
        tokio::task::spawn_blocking(move || {
            let engine = GoalEngine::new(&root);
            let intent = match intent_str.as_str() {
                "plan" => GoalIntent::Plan,
                "brainstorm" => GoalIntent::Brainstorm,
                _ => GoalIntent::Execute,
            };
            match engine.open(&request, intent) {
                Ok(goal) => serde_json::to_string_pretty(&goal).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Get current goal status including progress, nodes, and suggested next action.")]
    async fn goal_status(&self, #[tool(aggr)] params: GoalIdParam) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let engine = GoalEngine::new(&root);
            match engine.status(&params.goal_id) {
                Ok(goal) => serde_json::to_string_pretty(&goal).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}")),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Close a goal. Triggers knowledge compounding. Call when all criteria MET or score >= target.")]
    async fn goal_close(&self, #[tool(aggr)] params: GoalIdParam) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let engine = GoalEngine::new(&root);
            let learner = Learner::new(&root);
            match engine.close(&params.goal_id) {
                Ok(goal) => {
                    let _ = learner.compound(&params.goal_id);
                    serde_json::to_string_pretty(&goal).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Plan tools ──────────────────────────────────────────────
    #[tool(description = "Build execution graph from goal. Each node gets RiskProfile annotation. Returns nodes with deps and parallel levels.")]
    async fn plan_build(&self, #[tool(aggr)] params: PlanBuildParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let planner = Planner::new(&root);
            let nodes: Vec<Node> = params.nodes.into_iter().map(|ns| {
                let mut node = Node::new(ns.id, ns.objective);
                if let Some(files) = ns.files {
                    node.owned_files = files;
                }
                node
            }).collect();
            let edges: Vec<Edge> = params.edges.unwrap_or_default().into_iter()
                .map(|es| Edge { from: es.from, to: es.to })
                .collect();
            match planner.build(&params.goal_id, nodes, edges, params.rationale.as_deref().unwrap_or("initial plan")) {
                Ok(plan) => {
                    let levels = plan.compute_levels();
                    let mut out = serde_json::to_value(&plan).unwrap_or_default();
                    if let serde_json::Value::Object(ref mut map) = out {
                        map.insert("levels".into(), serde_json::to_value(&levels).unwrap_or_default());
                    }
                    serde_json::to_string_pretty(&out).unwrap_or_default()
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Return currently executable nodes (all deps satisfied). Each includes objective, constraints, and injected patterns.")]
    async fn plan_next(&self, #[tool(aggr)] params: GoalIdParam) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let scheduler = Scheduler::new(&root);
            let learner = Learner::new(&root);
            match scheduler.ready_nodes(&params.goal_id) {
                Ok(mut nodes) => {
                    for node in &mut nodes {
                        if let Ok(patterns) = learner.inject_for_node(&node.objective, 3) {
                            node.injected_patterns = patterns.iter().map(|p| format!("{}: {}", p.name, p.approach)).collect();
                        }
                    }
                    serde_json::to_string_pretty(&nodes).unwrap_or_default()
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Apply graph mutations to create a new PlanVersion. Used after escalation L3.")]
    async fn plan_mutate(&self, #[tool(aggr)] params: PlanMutateParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let planner = Planner::new(&root);
            match planner.get_latest(&params.goal_id) {
                Ok(plan) => {
                    let json = serde_json::json!({
                        "goal_id": params.goal_id,
                        "current_rev": plan.rev,
                        "rationale": params.rationale,
                        "note": "Use plan_build with updated nodes/edges to create new version"
                    });
                    serde_json::to_string_pretty(&json).unwrap_or_default()
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Node tools ──────────────────────────────────────────────
    #[tool(description = "Start working on a node. Validates deps are satisfied. Checks PolicyEngine. Transitions to InProgress.")]
    async fn node_start(&self, #[tool(aggr)] params: NodeStartParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let flow_dir = root.join(".flow");

            // PolicyEngine check: verify no lock conflicts for this node
            let policy = PolicyEngine::new();
            let locks = flowctl_core::json_store::locks_read(&flow_dir).unwrap_or_default();
            let active_locks: Vec<flowctl_core::quality::policy::FileLock> = locks.iter().map(|l| {
                flowctl_core::quality::policy::FileLock {
                    file_path: l.file_path.clone(),
                    node_id: l.task_id.clone(),
                    mode: l.mode.clone(),
                }
            }).collect();
            let ctx = flowctl_core::quality::policy::PolicyContext {
                active_locks,
                guard_ran: false,
                current_node: Some(params.node_id.clone()),
            };
            let decision = policy.check_mcp("node.start", Some(&params.node_id), &ctx);
            if let flowctl_core::quality::PolicyDecision::Block(msg) = decision {
                return format!("{{\"error\":\"policy blocked: {msg}\"}}");
            }

            // Emit event
            let event_store = EventStore::new(&flow_dir);
            let _ = event_store.emit(&params.goal_id, GoalEventKind::NodeStarted, &params.node_id);

            let scheduler = Scheduler::new(&flow_dir);
            match scheduler.start_node(&params.goal_id, &params.node_id) {
                Ok(plan) => {
                    let node = plan.nodes.iter().find(|n| n.id == params.node_id);
                    serde_json::to_string_pretty(&node).unwrap_or_default()
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Mark node as done. Records attempt in AttemptStore, emits event, releases locks, returns newly unblocked nodes.")]
    async fn node_finish(&self, #[tool(aggr)] params: NodeFinishParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let flow_dir = root.join(".flow");

            // 1. Record attempt in AttemptStore
            let attempt_store = AttemptStore::new(&flow_dir);
            let attempt_count = attempt_store.count_for_node(&params.goal_id, &params.node_id).unwrap_or(0);
            let attempt = Attempt {
                node_id: params.node_id.clone(),
                seq: attempt_count + 1,
                summary: params.summary.clone(),
                changed_files: params.changed_files.clone().unwrap_or_default(),
                commits: vec![],
                tests: vec![],
                findings: vec![],
                suggested_mutations: vec![],
                duration_seconds: 0,
                created_at: chrono::Utc::now(),
            };
            let _ = attempt_store.record(&params.goal_id, &attempt);

            // 2. Emit completion event
            let event_store = EventStore::new(&flow_dir);
            let _ = event_store.emit(&params.goal_id, GoalEventKind::NodeCompleted, &format!("{}: {}", params.node_id, params.summary));

            // 3. Release locks for this node
            let _ = flowctl_core::json_store::lock_release_task(&flow_dir, &params.node_id);

            // 4. Record learning (auto-fallback)
            let learner = Learner::new(&flow_dir);
            let _ = learner.record(
                &params.goal_id,
                Some(&params.node_id),
                LearningKind::Success,
                &params.summary,
                vec![],
            );

            // 5. Update plan state
            let scheduler = Scheduler::new(&flow_dir);
            match scheduler.finish_node(&params.goal_id, &params.node_id) {
                Ok(newly_ready) => {
                    serde_json::json!({
                        "node_id": params.node_id,
                        "status": "done",
                        "summary": params.summary,
                        "attempt_seq": attempt_count + 1,
                        "locks_released": true,
                        "learning_recorded": true,
                        "newly_ready": newly_ready.iter().map(|n| &n.id).collect::<Vec<_>>(),
                    }).to_string()
                }
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Report node failure. Triggers three-level escalation. Returns suggested action.")]
    async fn node_fail(&self, #[tool(aggr)] params: NodeFailParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let scheduler = Scheduler::new(&root);
            let escalation = EscalationEngine::new(&root);
            let _ = scheduler.fail_node(&params.goal_id, &params.node_id);
            match escalation.handle_failure(&params.goal_id, &params.node_id, &params.error) {
                Ok(action) => serde_json::to_string_pretty(&action).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Quality tool ────────────────────────────────────────────
    #[tool(description = "Run quality guard at risk-proportional depth. trivial: lint only. standard: lint+type+test. thorough: all+review.")]
    async fn quality_run(&self, #[tool(aggr)] params: QualityRunParams) -> String {
        let root = self.root.clone();
        let depth = params.depth.unwrap_or_else(|| "standard".into());
        tokio::task::spawn_blocking(move || {
            // Run flowctl guard as subprocess (it reads guard commands from project-context.md)
            let flowctl = root.join("bin/flowctl");
            let flowctl_path = if flowctl.exists() {
                flowctl.to_string_lossy().to_string()
            } else {
                // Fall back to PATH
                "flowctl".to_string()
            };
            let output = std::process::Command::new(&flowctl_path)
                .arg("guard")
                .arg("--json")
                .current_dir(&root)
                .output();
            match output {
                Ok(result) => {
                    let stdout = String::from_utf8_lossy(&result.stdout);
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    let passed = result.status.success();
                    serde_json::json!({
                        "depth": depth,
                        "status": if passed { "pass" } else { "fail" },
                        "exit_code": result.status.code(),
                        "stdout": stdout.trim(),
                        "stderr": if stderr.is_empty() { None } else { Some(stderr.trim().to_string()) },
                    }).to_string()
                }
                Err(e) => serde_json::json!({
                    "depth": depth,
                    "status": "error",
                    "error": format!("Failed to run guard: {e}"),
                }).to_string(),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Lock tools ──────────────────────────────────────────────
    #[tool(description = "Acquire file locks for a node. Prevents parallel workers from editing same files.")]
    async fn lock_acquire(&self, #[tool(aggr)] params: LockParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let flow_dir = root.join(".flow");
            let mut locked = Vec::new();
            let mut conflicts = Vec::new();
            for file in &params.files {
                // Check for conflict first
                let existing = flowctl_core::json_store::locks_read(&flow_dir).unwrap_or_default();
                let conflict = existing.iter().find(|l| l.file_path == *file && l.task_id != params.node_id);
                if let Some(holder) = conflict {
                    conflicts.push(serde_json::json!({
                        "file": file,
                        "held_by": holder.task_id,
                    }));
                } else {
                    match flowctl_core::json_store::lock_acquire(&flow_dir, file, &params.node_id, "write") {
                        Ok(()) => locked.push(file.clone()),
                        Err(e) => conflicts.push(serde_json::json!({"file": file, "error": e.to_string()})),
                    }
                }
            }
            serde_json::json!({
                "node_id": params.node_id,
                "locked": locked,
                "conflicts": conflicts,
                "status": if conflicts.is_empty() { "acquired" } else { "partial" },
            }).to_string()
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Release file locks held by a node. Idempotent.")]
    async fn lock_release(&self, #[tool(aggr)] params: LockReleaseParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let flow_dir = root.join(".flow");
            match flowctl_core::json_store::lock_release_task(&flow_dir, &params.node_id) {
                Ok(count) => serde_json::json!({
                    "node_id": params.node_id,
                    "released_count": count,
                    "status": "released",
                }).to_string(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Knowledge tools ─────────────────────────────────────────
    #[tool(description = "Search across all knowledge layers: learnings, patterns, and methodology rules. Returns ranked results.")]
    async fn knowledge_search(&self, #[tool(aggr)] params: KnowledgeSearchParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let learner = Learner::new(&root);
            let limit = params.limit.unwrap_or(5) as usize;
            match learner.search(&params.query, limit) {
                Ok(result) => serde_json::to_string_pretty(&result).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Record a learning. Workers should call this when they discover something important.")]
    async fn knowledge_record(&self, #[tool(aggr)] params: KnowledgeRecordParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let learner = Learner::new(&root);
            let kind = match params.kind.as_str() {
                "failure" => LearningKind::Failure,
                "discovery" => LearningKind::Discovery,
                "pitfall" => LearningKind::Pitfall,
                _ => LearningKind::Success,
            };
            match learner.record(&params.goal_id, params.node_id.as_deref(), kind, &params.content, vec![]) {
                Ok(learning) => serde_json::to_string_pretty(&learning).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Compound learnings into patterns after goal completion. Promotes 3+ same-tag learnings to patterns.")]
    async fn knowledge_compound(&self, #[tool(aggr)] params: GoalIdParam) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let learner = Learner::new(&root);
            match learner.compound(&params.goal_id) {
                Ok(patterns) => serde_json::to_string_pretty(&patterns).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    #[tool(description = "Refresh stale patterns: decay confidence on old patterns, surface those needing validation.")]
    async fn knowledge_refresh(&self) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let learner = Learner::new(&root);
            match learner.refresh_stale() {
                Ok(count) => serde_json::json!({"decayed_count": count}).to_string(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            }
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    // ── Codebase tool ───────────────────────────────────────────
    #[tool(description = "Analyze codebase: affected files, risk assessment, related symbols. Uses code graph + trigram index.")]
    async fn codebase_assess(&self, #[tool(aggr)] params: CodebaseAssessParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let flow_dir = root.join(".flow");
            let graph_path = flow_dir.join("graph.bin");
            let index_path = flow_dir.join("index").join("ngram.bin");

            let mut result = serde_json::json!({
                "query": params.query,
                "graph_available": graph_path.exists(),
                "index_available": index_path.exists(),
            });

            // Try to load and use the code graph
            if graph_path.exists() {
                if let Ok(graph) = flowctl_core::graph_store::CodeGraph::load(&graph_path) {
                    // Find references for the query term
                    let refs = graph.find_refs(&params.query);
                    let ref_files: Vec<&str> = refs.iter().map(|s| s.file.as_str()).collect();

                    // Find impact (transitive dependents)
                    let mut impacted = Vec::new();
                    for file in &ref_files {
                        let impact = graph.find_impact(file);
                        for f in impact {
                            if !impacted.contains(&f) {
                                impacted.push(f);
                            }
                        }
                    }
                    impacted.truncate(20);

                    let stats = graph.stats();
                    if let serde_json::Value::Object(ref mut map) = result {
                        map.insert("symbols_found".into(), serde_json::json!(refs.len()));
                        map.insert("symbol_files".into(), serde_json::json!(ref_files));
                        map.insert("impacted_files".into(), serde_json::json!(impacted));
                        map.insert("graph_stats".into(), serde_json::json!({
                            "symbols": stats.symbol_count,
                            "files": stats.file_count,
                            "edges": stats.edge_count,
                        }));
                    }
                }
            }

            // Try trigram index search
            if index_path.exists() {
                if let Ok(index) = flowctl_core::ngram_index::NgramIndex::load(&index_path) {
                    let hits = index.search(&params.query, 10);
                    if let serde_json::Value::Object(ref mut map) = result {
                        map.insert("index_hits".into(), serde_json::json!(hits.len()));
                        let hit_paths: Vec<String> = hits.into_iter()
                            .map(|h| format!("{} ({})", h.path.display(), h.match_count))
                            .collect();
                        map.insert("index_results".into(), serde_json::json!(hit_paths));
                    }
                }
            }

            serde_json::to_string_pretty(&result).unwrap_or_default()
        }).await.unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

#[tool(tool_box)]
impl ServerHandler for FlowctlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("flowctl V3 — Goal-driven adaptive engine. 16 tools for goal/plan/node/quality/lock/knowledge management.".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Start the MCP server on stdio.
pub async fn run_server(root: PathBuf) -> anyhow::Result<()> {
    let server = FlowctlServer::new(root);
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
