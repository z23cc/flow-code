//! V4 MCP server — 3 tools: flow_drive, flow_submit, flow_query.
//!
//! Engine-driven protocol. The LLM calls drive/submit/query.
//! The engine handles all orchestration, scheduling, and quality.

use std::path::PathBuf;

use flowctl_core::domain::action_spec::SubmitInput;
use flowctl_core::engine::orchestrator::Orchestrator;
use rmcp::{ServerHandler, ServiceExt, model::{ServerCapabilities, ServerInfo}, schemars, tool};

/// The MCP server — thin wrapper around the Orchestrator.
#[derive(Clone)]
pub struct FlowctlServer {
    pub root: PathBuf,
}

impl FlowctlServer {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

// ── Tool parameters ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct DriveParams {
    #[schemars(description = "Natural language goal description, OR an existing goal_id (g-*) to resume")]
    pub request: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SubmitParams {
    #[schemars(description = "Action ID returned by flow_drive (a-{goal}-{node})")]
    pub action_id: String,
    #[schemars(description = "Result status: done, failed, or partial")]
    pub status: String,
    #[schemars(description = "Summary of work performed")]
    pub summary: String,
    #[schemars(description = "List of files changed")]
    pub files_changed: Option<Vec<String>>,
    #[schemars(description = "Test output (if tests were run)")]
    pub test_output: Option<String>,
    #[schemars(description = "Error details (if status is failed)")]
    pub error: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryParams {
    #[schemars(description = "Natural language question about goal status, knowledge, or codebase")]
    pub question: String,
    #[schemars(description = "Optional goal ID to scope the query")]
    pub goal_id: Option<String>,
}

// ── 3 Tool Implementations ──────────────────────────────────────────

#[tool(tool_box)]
impl FlowctlServer {
    #[tool(description = "Start or resume a development goal. Pass a natural language description \
        (e.g. 'add OAuth login with Google') or an existing goal_id (g-*) to resume. \
        Returns an ActionSpec containing: what to do (objective), relevant code files (context.files — \
        summaries only, use flow_query 'read file <path>' for full content), quality gate commands \
        (guard.commands), and progress (total_nodes, completed, parallel_ready). \
        After completing the work, call flow_submit with the action_id and results. \
        The engine auto-plans multi-step goals into a DAG with parallel execution where possible.")]
    async fn flow_drive(&self, #[tool(aggr)] params: DriveParams) -> String {
        let root = self.root.clone();
        let request = params.request;
        tokio::task::spawn_blocking(move || {
            let orchestrator = Orchestrator::new(&root);
            match orchestrator.drive(&request) {
                Ok(spec) => serde_json::to_string_pretty(&spec)
                    .unwrap_or_else(|e| format_error("serialization", &e.to_string(), false, None)),
                Err(e) => classify_error(&e),
            }
        })
        .await
        .unwrap_or_else(|e: tokio::task::JoinError| format_error("internal", &e.to_string(), true, Some("retry the request")))
    }

    #[tool(description = "Submit work results after completing an action from flow_drive. \
        Pass action_id (from the ActionSpec), status ('done', 'failed', or 'partial'), \
        a summary of what you did, and optionally files_changed and test_output. \
        Returns the next ActionSpec (next task, fix request, review, or 'complete'). \
        On failure: include error details — the engine escalates automatically \
        (retry → change strategy → replan). On partial: engine re-issues the same node \
        with your prior attempts as context. Errors are structured with category, \
        retry_safe flag, and recovery suggestion.")]
    async fn flow_submit(&self, #[tool(aggr)] params: SubmitParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let orchestrator = Orchestrator::new(&root);
            let status = match params.status.as_str() {
                "failed" => flowctl_core::SubmitStatus::Failed,
                "partial" => flowctl_core::SubmitStatus::Partial,
                _ => flowctl_core::SubmitStatus::Done,
            };
            let input = SubmitInput {
                action_id: params.action_id,
                status,
                summary: params.summary,
                files_changed: params.files_changed.unwrap_or_default(),
                test_output: params.test_output,
                error: params.error,
            };
            match orchestrator.submit(&input) {
                Ok(spec) => serde_json::to_string_pretty(&spec)
                    .unwrap_or_else(|e| format_error("serialization", &e.to_string(), false, None)),
                Err(e) => classify_error(&e),
            }
        })
        .await
        .unwrap_or_else(|e: tokio::task::JoinError| format_error("internal", &e.to_string(), true, Some("retry the request")))
    }

    #[tool(description = "Query goal state, knowledge, or files. Supports: \
        (1) Goal status — pass goal_id to get per-node progress with attempt counts. \
        (2) File content — 'read file <path>' returns full file (progressive disclosure). \
        (3) Knowledge — ask about 'patterns' or 'learnings' from past goals. \
        (4) Code graph — ask about symbol references and impact analysis. \
        (5) Goal list — without goal_id, returns all goals with status. \
        Errors include category, retry_safe, and recovery suggestion.")]
    async fn flow_query(&self, #[tool(aggr)] params: QueryParams) -> String {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            let orchestrator = Orchestrator::new(&root);
            match orchestrator.query(&params.question, params.goal_id.as_deref()) {
                Ok(result) => serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|e| format_error("serialization", &e.to_string(), false, None)),
                Err(e) => classify_error(&e),
            }
        })
        .await
        .unwrap_or_else(|e: tokio::task::JoinError| format_error("internal", &e.to_string(), true, Some("retry the request")))
    }
}

#[tool(tool_box)]
impl ServerHandler for FlowctlServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "flowctl V4 — Goal-driven adaptive engine. Workflow: \
                 (1) flow_drive('goal description') → get ActionSpec with objective + context. \
                 (2) Do the work described in the ActionSpec. \
                 (3) flow_submit(action_id, 'done', 'summary') → get next ActionSpec or 'complete'. \
                 Use flow_query to check status, read file contents, or search knowledge. \
                 Context files are summaries — use flow_query('read file <path>') for full content, \
                 or use RepoPrompt tools (get_code_structure, file_search, read_file) for richer \
                 context. ActionSpec.tool_hints suggests which RP tools to call. \
                 Errors are structured: {error: {category, message, retry_safe, recovery}}."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Structured Error Helpers (SERF pattern) ────────────────────────

/// Classify an error string into a structured error response.
fn classify_error(error: &str) -> String {
    let e_lower = error.to_lowercase();
    let (category, retry_safe, recovery) = if e_lower.contains("not found") {
        ("not_found", false, "check the ID and try again")
    } else if e_lower.contains("empty") {
        ("validation", false, "provide a non-empty value")
    } else if e_lower.contains("invalid") {
        ("validation", false, "check the input format")
    } else if e_lower.contains("lock") || e_lower.contains("busy") {
        ("transient", true, "retry after a moment")
    } else if e_lower.contains("permission") || e_lower.contains("denied") {
        ("auth", false, "check file permissions")
    } else {
        ("unknown", true, "retry or rephrase the request")
    };
    format_error(category, error, retry_safe, Some(recovery))
}

/// Format a structured error JSON string.
fn format_error(category: &str, message: &str, retry_safe: bool, recovery: Option<&str>) -> String {
    let recovery_json = recovery
        .map(|r| format!(",\"recovery\":\"{}\"", r.replace('"', "\\\"")))
        .unwrap_or_default();
    format!(
        "{{\"error\":{{\"category\":\"{category}\",\"message\":\"{msg}\",\"retry_safe\":{retry_safe}{recovery_json}}}}}",
        msg = message.replace('"', "\\\""),
    )
}

/// Start the MCP server on stdio.
pub async fn run_server(root: PathBuf) -> anyhow::Result<()> {
    let server = FlowctlServer::new(root);
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
