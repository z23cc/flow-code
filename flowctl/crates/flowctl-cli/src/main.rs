//! flowctl CLI entry point.
//!
//! Clap 4 derive-based CLI matching the full Python flowctl command surface.
//! All commands are registered as stubs that return "not yet implemented".

#![forbid(unsafe_code)]

mod commands;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};

use commands::{
    admin::{self, ConfigCmd, ReviewCmd},
    approval::ApprovalCmd,
    checklist::ChecklistCmd,
    checkpoint::CheckpointCmd,
    code_structure::CodeStructureCmd,
    codex::CodexCmd,
    dep::DepCmd,
    epic::EpicCmd,
    gap::GapCmd,
    graph::GraphCmd,
    hook::HookCmd,
    index::IndexCmd,
    log::LogCmd,
    memory::MemoryCmd,
    outputs::OutputsCmd,
    patch::PatchCmd,
    project_context::ProjectContextCmd,
    query,
    ralph::RalphCmd,
    rp::RpCmd,
    scout_cache::ScoutCacheCmd,
    skill::SkillCmd,
    stack::{InvariantsCmd, StackCmd},
    stats::StatsCmd,
    task::TaskCmd,
    workflow::{self, PipelinePhaseCmd, WorkerPhaseCmd},
};
use output::OutputOpts;

/// flowctl - development orchestration engine.
#[derive(Parser, Debug)]
#[command(
    name = "flowctl",
    version,
    about = "Development orchestration engine",
    long_about = "Agent-Primary Workflow / Orchestration CLI.\n\n\
        Primary surface: Workflow (epic/task/phase lifecycle, DAG scheduling)\n\
        Secondary surfaces: Code Intelligence, Integration, Meta/Config, File Ops",
    help_template = "\
{about-with-newline}
{usage-heading} {usage}

{all-args}

[Workflow]        epic, task, dep, phase, worker-phase, ready, next, start, done, restart, block, fail, queue, events
[Query]           show, epics, tasks, list, cat, files, lock, unlock, lock-check, heartbeat
[Admin]           init, detect, startup, status, doctor, validate, guard, pre-launch, worker-prompt, review, dag, estimate, replay, diff, recover
[Code Intel]      graph, index, find, search, code-structure, repo-map, plan-depth
[Integration]     rp, codex, hook
[Meta / Config]   config, stack, invariants, project-context, skill, scout-cache, ralph, stats
[Data / IO]       write-file, patch, export, import, checkpoint, outputs, log, memory, gap, approval, checklist
[Introspection]   schema, describe, commands, completions
"
)]
struct Cli {
    #[command(flatten)]
    output: OutputOpts,

    /// Preview mutations as JSON without applying them.
    #[arg(long, global = true)]
    dry_run: bool,

    /// Project root directory (overrides CWD for .flow/ resolution).
    /// Use this when running flowctl from a subdirectory.
    #[arg(long = "project-dir", short = 'C', global = true)]
    project_dir: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // ── Admin ────────────────────────────────────────────────────────
    /// Initialize .flow/ directory.
    Init,
    /// Check if .flow/ exists.
    Detect,
    /// Combined startup: detect + interrupted + memory + review-backend + epics (single call).
    Startup,
    /// Show .flow state and active runs.
    Status {
        /// Detect interrupted epics with undone tasks.
        #[arg(long)]
        interrupted: bool,
        /// Render ASCII DAG of task dependencies for the active epic.
        #[arg(long)]
        dag: bool,
        /// Epic ID (required with --dag or --progress).
        #[arg(long)]
        epic: Option<String>,
        /// Show wave/task progress summary.
        #[arg(long)]
        progress: bool,
    },
    /// Run comprehensive state health diagnostics.
    Doctor {
        /// Run workflow-specific health checks (backend config, tools, locks).
        #[arg(long)]
        workflow: bool,
    },
    /// Validate epic or all.
    Validate {
        /// Epic ID.
        #[arg(long)]
        epic: Option<String>,
        /// Validate all epics and tasks.
        #[arg(long)]
        all: bool,
    },
    /// Show all resolved state directory paths (three-layer resolution).
    Paths,
    /// Show resolved state directory path.
    StatePath {
        /// Task ID to show state file path for.
        #[arg(long)]
        task: Option<String>,
    },
    /// Get review backend.
    ReviewBackend {
        /// Compare review receipts (comma-separated file paths).
        #[arg(long)]
        compare: Option<String>,
        /// Auto-discover review receipts for epic.
        #[arg(long)]
        epic: Option<String>,
    },
    /// Extract structured findings from review output.
    ParseFindings {
        /// Review output file (or '-' for stdin).
        #[arg(long)]
        file: String,
        /// Epic ID (required with --register).
        #[arg(long)]
        epic: Option<String>,
        /// Auto-register critical/major findings as gaps.
        #[arg(long)]
        register: bool,
        /// Gap source label.
        #[arg(long, default_value = "manual")]
        source: String,
    },
    /// Run test/lint/typecheck guards from stack config.
    Guard {
        /// Run guards for specific layer.
        #[arg(long, default_value = "all")]
        layer: String,
    },
    /// Run pre-launch checks (6 dimensions: quality, security, performance, a11y, infra, docs).
    PreLaunch,
    /// Output trimmed worker prompt based on mode flags.
    WorkerPrompt {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Include TDD Phase 2a.
        #[arg(long)]
        tdd: bool,
        /// Include review Phase 4 (rp or codex).
        #[arg(long, value_parser = ["rp", "codex"])]
        review: Option<String>,
        /// Output minimal bootstrap prompt (~200 tokens).
        #[arg(long)]
        bootstrap: bool,
        /// Inline core skill rules into the prompt (reduces worker Phase 2 file reads).
        #[arg(long)]
        inline_skills: bool,
    },

    /// Review commands (merge findings, etc.).
    Review {
        #[command(subcommand)]
        cmd: ReviewCmd,
    },

    /// Render ASCII DAG of task dependencies.
    Dag {
        /// Epic ID.
        id: String,
    },
    /// Estimate remaining time for an epic based on historical durations.
    Estimate {
        /// Epic ID (positional).
        id: Option<String>,
        /// Epic ID (flag, kept for compatibility).
        #[arg(long = "epic")]
        epic_flag: Option<String>,
    },
    /// Replay an epic: reset all tasks to todo for re-execution.
    Replay {
        /// Epic ID.
        epic_id: String,
        /// Show what would be reset without doing it.
        #[arg(long)]
        dry_run: bool,
        /// Allow replay even if tasks are in_progress.
        #[arg(long)]
        force: bool,
    },
    /// Show git diff summary for an epic's branch.
    Diff {
        /// Epic ID.
        epic_id: String,
    },

    // ── Nested command groups ────────────────────────────────────────
    /// Config commands.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Epic commands.
    Epic {
        #[command(subcommand)]
        cmd: EpicCmd,
    },
    /// Task commands.
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// Dependency commands.
    Dep {
        #[command(subcommand)]
        cmd: DepCmd,
    },
    /// Approval commands (request/resolve blocking decisions).
    Approval {
        #[command(subcommand)]
        cmd: ApprovalCmd,
    },
    /// Structured Definition of Done checklists.
    Checklist {
        #[command(subcommand)]
        cmd: ChecklistCmd,
    },
    /// Requirement gap registry.
    Gap {
        #[command(subcommand)]
        cmd: GapCmd,
    },
    /// Decision logging for workflow traceability.
    Log {
        #[command(subcommand)]
        cmd: LogCmd,
    },
    /// Memory commands (v2: atomic entries).
    Memory {
        #[command(subcommand)]
        cmd: MemoryCmd,
    },
    /// Outputs commands (narrative handoff between tasks).
    Outputs {
        #[command(subcommand)]
        cmd: OutputsCmd,
    },
    /// Checkpoint commands.
    Checkpoint {
        #[command(subcommand)]
        cmd: CheckpointCmd,
    },
    /// Stack profile commands.
    Stack {
        #[command(subcommand)]
        cmd: StackCmd,
    },
    /// Architecture invariant registry.
    Invariants {
        #[command(subcommand)]
        cmd: InvariantsCmd,
    },
    /// Ralph run control commands.
    Ralph {
        #[command(subcommand)]
        cmd: RalphCmd,
    },
    /// Scout result cache commands (get, set, clear).
    ScoutCache {
        #[command(subcommand)]
        cmd: ScoutCacheCmd,
    },
    /// Skill registry commands (register, match).
    Skill {
        #[command(subcommand)]
        cmd: SkillCmd,
    },
    /// RepoPrompt helpers.
    Rp {
        #[command(subcommand)]
        cmd: RpCmd,
    },
    /// Codex CLI helpers.
    Codex {
        #[command(subcommand)]
        cmd: CodexCmd,
    },
    /// Claude Code hook scripts (auto-memory, ralph-guard).
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
    /// Trigram index commands (build, status, search).
    Index {
        #[command(subcommand)]
        cmd: IndexCmd,
    },
    /// Code graph commands (build, refs, impact, map).
    Graph {
        #[command(subcommand)]
        cmd: GraphCmd,
    },
    /// Stats dashboard: summary, trends, tokens, DORA metrics.
    Stats {
        #[command(subcommand)]
        cmd: StatsCmd,
    },
    /// Phase-gate sequential execution for workers.
    WorkerPhase {
        #[command(subcommand)]
        cmd: WorkerPhaseCmd,
    },
    /// Epic-level pipeline phase progression.
    Phase {
        #[command(subcommand)]
        cmd: PipelinePhaseCmd,
    },
    /// Classify request depth for adaptive plan step selection.
    PlanDepth {
        /// Request text to classify.
        #[arg(long)]
        request: String,
    },

    // ── Query commands ───────────────────────────────────────────────
    /// Show epic or task.
    Show {
        /// Epic or task ID.
        id: String,
    },
    /// List all epics.
    Epics,
    /// List tasks.
    Tasks {
        /// Filter by epic ID.
        #[arg(long)]
        epic: Option<String>,
        /// Filter by status.
        #[arg(long, value_parser = ["todo", "in_progress", "blocked", "done"])]
        status: Option<String>,
        /// Filter by domain.
        #[arg(long, value_parser = ["frontend", "backend", "architecture", "testing", "docs", "ops", "general"])]
        domain: Option<String>,
    },
    /// List all epics and tasks.
    List,
    /// Print spec markdown.
    Cat {
        /// Epic or task ID.
        id: String,
    },
    /// Show file ownership map for epic.
    Files {
        /// Epic ID (positional).
        id: Option<String>,
        /// Epic ID (flag, kept for compatibility).
        #[arg(long = "epic")]
        epic_flag: Option<String>,
    },
    /// Lock files for a task (Teams mode).
    Lock {
        /// Task ID that owns the files.
        #[arg(long)]
        task: String,
        /// Comma-separated file paths to lock.
        #[arg(long)]
        files: String,
        /// Lock mode: read, write, or directory_add (default: write).
        #[arg(long, default_value = "write", value_parser = ["read", "write", "directory_add"])]
        mode: String,
    },
    /// Unlock files for a task (Teams mode).
    Unlock {
        /// Task ID to unlock files for.
        #[arg(long)]
        task: Option<String>,
        /// Comma-separated file paths.
        #[arg(long)]
        files: Option<String>,
        /// Clear ALL file locks.
        #[arg(long)]
        all: bool,
    },
    /// Extend lock TTL for a task (Teams mode heartbeat).
    Heartbeat {
        /// Task ID to extend locks for.
        #[arg(long)]
        task: String,
    },
    /// Check file lock status (Teams mode).
    LockCheck {
        /// Specific file to check.
        #[arg(long)]
        file: Option<String>,
    },

    // ── Workflow commands ─────────────────────────────────────────────
    /// List ready tasks.
    Ready {
        /// Epic ID (positional).
        id: Option<String>,
        /// Epic ID (flag, kept for compatibility).
        #[arg(long = "epic")]
        epic_flag: Option<String>,
    },
    /// Select next plan/work unit.
    Next {
        /// JSON file with ordered epic list.
        #[arg(long)]
        epics_file: Option<String>,
        /// Require plan review before work.
        #[arg(long)]
        require_plan_review: bool,
        /// Require completion review when all tasks done.
        #[arg(long)]
        require_completion_review: bool,
    },
    /// Show multi-epic queue status.
    Queue,
    /// Start task. Rejects parallel start unless --force (confirms worktree isolation).
    Start {
        /// Task ID.
        id: String,
        /// Skip status/dependency/claim checks.
        #[arg(long)]
        force: bool,
        /// Claim note.
        #[arg(long)]
        note: Option<String>,
    },
    /// Complete task.
    Done {
        /// Task ID.
        id: String,
        /// Done summary markdown file.
        #[arg(long)]
        summary_file: Option<String>,
        /// Done summary (inline text).
        #[arg(long)]
        summary: Option<String>,
        /// Evidence JSON file path or inline JSON string.
        #[arg(long)]
        evidence_json: Option<String>,
        /// Evidence JSON (inline string, legacy).
        #[arg(long)]
        evidence: Option<String>,
        /// Skip status checks.
        #[arg(long)]
        force: bool,
    },
    /// Restart task and cascade-reset downstream dependents.
    Restart {
        /// Task ID.
        id: String,
        /// Show what would be reset without doing it.
        #[arg(long)]
        dry_run: bool,
        /// Allow restart even if tasks are in_progress.
        #[arg(long)]
        force: bool,
    },
    /// Block task with reason.
    Block {
        /// Task ID.
        id: String,
        /// Block reason (inline text).
        #[arg(long)]
        reason: Option<String>,
        /// Block reason from file (deprecated, use --reason).
        #[arg(long)]
        reason_file: Option<String>,
    },
    /// Mark task as failed (triggers upstream_failed propagation to downstream).
    Fail {
        /// Task ID.
        id: String,
        /// Reason for failure.
        #[arg(long)]
        reason: Option<String>,
        /// Skip status checks.
        #[arg(long)]
        force: bool,
    },

    /// Show event store history for an epic (all streams).
    Events {
        /// Epic ID (positional).
        id: Option<String>,
        /// Epic ID (flag, kept for compatibility).
        #[arg(long = "epic")]
        epic_flag: Option<String>,
    },

    // ── File I/O ─────────────────────────────────────────────────────
    /// Write content to a file (pipeline helper, bypasses permission prompts).
    WriteFile {
        /// Target file path.
        #[arg(long)]
        path: String,
        /// Content to write (inline). Use --stdin for piped input.
        #[arg(long)]
        content: Option<String>,
        /// Read content from stdin.
        #[arg(long)]
        stdin: bool,
        /// Append instead of overwrite.
        #[arg(long)]
        append: bool,
    },

    // ── Patch ─────────────────────────────────────────────────────────
    /// Fuzzy diff, patch application, and search-replace.
    Patch {
        #[command(subcommand)]
        cmd: PatchCmd,
    },

    // ── Project context ──────────────────────────────────────────────
    /// Project context commands (parse .flow/project-context.md).
    ProjectContext {
        #[command(subcommand)]
        cmd: ProjectContextCmd,
    },

    // ── Data exchange ────────────────────────────────────────────────
    /// Export epics/tasks from DB to Markdown files.
    Export {
        /// Epic ID to export (or omit for all).
        #[arg(long)]
        epic: Option<String>,
        /// Output format.
        #[arg(long, default_value = "md")]
        format: String,
    },
    /// Import epics/tasks from Markdown files into DB (alias for reindex).
    Import,

    // ── Search ────────────────────────────────────────────────────────
    /// Fuzzy file search with frecency boosting and git status filtering.
    Search {
        /// Fuzzy query string.
        query: String,
        /// Filter by git status: modified, staged, untracked.
        #[arg(long, value_parser = ["modified", "staged", "untracked"])]
        git: Option<String>,
        /// Maximum number of results.
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    // ── Code structure & repo map ──────────────────────────────────
    /// Extract code structure (symbols, signatures) from source files.
    CodeStructure {
        #[command(subcommand)]
        cmd: CodeStructureCmd,
    },
    /// Generate a ranked repo map (top symbols by importance).
    RepoMap {
        /// Token budget for the output (default: unlimited). Use --budget to limit.
        #[arg(long, default_value = "0")]
        budget: usize,
        /// Root directory to scan (default: current directory).
        #[arg(long, default_value = ".")]
        path: String,
    },

    // ── Intent-level commands ────────────────────────────────────────
    /// Smart code search (auto-routes to best backend).
    Find {
        /// Search query.
        query: String,
        /// Maximum number of results.
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Smart code edit (exact match with fuzzy fallback).
    Edit {
        /// File to edit.
        #[arg(long)]
        file: String,
        /// Text to find and replace.
        #[arg(long)]
        old: String,
        /// Replacement text.
        #[arg(long)]
        new: String,
    },

    // ── Recovery ──────────────────────────────────────────────────────
    /// Recover task completion status from git history.
    Recover {
        /// Epic ID to recover tasks for.
        #[arg(long)]
        epic: String,
        /// Preview without making changes.
        #[arg(long)]
        dry_run: bool,
    },

    // ── V3 MCP Server ───────────────────────────────────────────────
    /// Start the MCP server (stdio transport). Exposes 16 V3 tools.
    Serve,

    /// Migrate old .flow/ data (epics→goals, tasks→plan nodes, memory→learnings).
    Migrate {
        /// Target version (currently only "v3").
        #[arg(default_value = "v3")]
        target: String,
        /// Dry-run: show what would be migrated without writing.
        #[arg(long)]
        dry_run: bool,
    },

    /// V3 PolicyEngine: check file access against lock state.
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },

    /// V3 session management.
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    // ── V3 Goal commands ───────────────────────────────────────────
    /// V3 goal lifecycle (open/status/close).
    Goal {
        #[command(subcommand)]
        action: GoalAction,
    },

    // ── V3 Plan commands ───────────────────────────────────────────
    /// V3 plan management (build/next).
    #[command(name = "plan-v3")]
    PlanV3 {
        #[command(subcommand)]
        action: PlanV3Action,
    },

    // ── V3 Node commands ───────────────────────────────────────────
    /// V3 node lifecycle (start/finish/fail).
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },

    // ── V3 Knowledge commands ──────────────────────────────────────
    /// V3 knowledge management (search/record/compound/refresh).
    Knowledge {
        #[command(subcommand)]
        action: KnowledgeAction,
    },

    // ── Shell completions ────────────────────────────────────────────
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },

    // ── Introspection (Agent-Primary discovery surface) ─────────────
    /// List all commands with descriptions (machine-readable command catalog).
    #[command(name = "commands")]
    CommandsList,
    /// Describe a command's accepted flags, types, defaults, and required status.
    Describe {
        /// Command name (e.g., "epic create", "task create", "unlock").
        command: Vec<String>,
    },
    /// Show JSON schema for a command's output structure.
    Schema {
        /// Command name.
        command: Vec<String>,
    },
}

// ── V3 subcommand enums ─────────────────────────────────────────────

#[derive(Subcommand, Debug)]
enum GoalAction {
    /// Open a new goal from a request.
    Open {
        /// User request describing the goal.
        request: String,
        /// Intent: execute, plan, or brainstorm.
        #[arg(long, default_value = "execute")]
        intent: String,
    },
    /// Get goal status.
    Status {
        /// Goal ID.
        goal_id: String,
    },
    /// Close a goal (mark done, trigger knowledge compounding).
    Close {
        /// Goal ID.
        goal_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum PlanV3Action {
    /// Build an execution graph for a goal.
    Build {
        /// Goal ID.
        goal_id: String,
    },
    /// Get currently ready nodes.
    Next {
        /// Goal ID.
        goal_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum NodeAction {
    /// Start working on a node.
    Start {
        /// Goal ID.
        goal_id: String,
        /// Node ID.
        node_id: String,
    },
    /// Mark a node as done.
    Finish {
        /// Goal ID.
        goal_id: String,
        /// Node ID.
        node_id: String,
        /// Summary of work done.
        #[arg(long)]
        summary: String,
    },
    /// Report a node failure.
    Fail {
        /// Goal ID.
        goal_id: String,
        /// Node ID.
        node_id: String,
        /// Error description.
        #[arg(long)]
        error: String,
    },
}

#[derive(Subcommand, Debug)]
enum KnowledgeAction {
    /// Search across all knowledge layers.
    Search {
        /// Search query.
        query: String,
        /// Max results.
        #[arg(long, default_value = "5")]
        limit: usize,
    },
    /// Record a learning.
    Record {
        /// Goal ID.
        goal_id: String,
        /// Content.
        content: String,
        /// Kind: success, failure, discovery, pitfall.
        #[arg(long, default_value = "discovery")]
        kind: String,
    },
    /// Compound learnings into patterns.
    Compound {
        /// Goal ID.
        goal_id: String,
    },
    /// Refresh stale patterns.
    Refresh,
}

#[derive(Subcommand, Debug)]
enum PolicyAction {
    /// Check if a tool call is allowed by policy (used by PreToolUse hook).
    CheckHook {
        /// Tool name (Edit, Write, Bash).
        #[arg(long)]
        tool: String,
        /// File path being accessed.
        #[arg(long)]
        file: Option<String>,
        /// Current node ID (if in worker context).
        #[arg(long)]
        node: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SessionAction {
    /// Save a session snapshot (active goals, nodes, locks).
    Snapshot,
    /// List active sessions.
    List,
}

fn main() {
    // Exit cleanly on broken pipe (e.g., `flowctl ... | head -1`)
    // instead of panicking with "failed printing to stdout".
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.to_string();
        if msg.contains("Broken pipe") || msg.contains("os error 32") {
            std::process::exit(0);
        }
        default_hook(info);
    }));

    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .context_lines(3)
                .build(),
        )
    }))
    .ok();

    let cli = Cli::parse();
    output::init_compact(cli.output.compact);
    let json = cli.output.json;
    let dry_run = cli.dry_run;

    // Apply --project-dir: change CWD before any command runs.
    // This fixes the #1 recurring audit failure: CWD drift after cd to subdirectories.
    if let Some(ref dir) = cli.project_dir {
        if let Err(e) = std::env::set_current_dir(dir) {
            output::error_exit(&format!("Cannot cd to project dir '{}': {}", dir, e));
        }
    }

    /// Resolve dual-mode argument: positional `id` takes precedence over `--epic` flag.
    fn resolve_epic(id: Option<String>, epic_flag: Option<String>, cmd_name: &str) -> String {
        id.or(epic_flag).unwrap_or_else(|| {
            output::error_exit(&format!(
                "{cmd_name} requires an epic ID (positional or --epic)"
            ));
        })
    }

    match cli.command {
        // Admin / top-level
        Commands::Init => admin::cmd_init(json),
        Commands::Paths => commands::paths::cmd_paths(json),
        Commands::Detect => admin::cmd_detect(json),
        Commands::Startup => admin::cmd_startup(json),
        Commands::Status {
            interrupted,
            dag,
            epic,
            progress,
        } => {
            if dag {
                commands::stats::cmd_dag(json, epic);
            } else if progress {
                admin::cmd_progress(json, epic);
            } else {
                admin::cmd_status(json, interrupted);
            }
        }
        Commands::Doctor { workflow } => admin::cmd_doctor(json, workflow),
        Commands::Validate { epic, all } => admin::cmd_validate(json, epic, all),
        Commands::StatePath { task } => admin::cmd_state_path(json, task),
        Commands::ReviewBackend { compare, epic } => admin::cmd_review_backend(json, compare, epic),
        Commands::ParseFindings {
            file,
            epic,
            register,
            source,
        } => admin::cmd_parse_findings(json, file, epic, register, source),
        Commands::Guard { layer } => admin::cmd_guard(json, layer),
        Commands::PreLaunch => commands::pre_launch::cmd_pre_launch(json),
        Commands::WorkerPrompt {
            task,
            tdd,
            review,
            bootstrap: _,
            inline_skills,
        } => admin::cmd_worker_prompt(json, task, tdd, review, inline_skills),

        Commands::Review { cmd } => admin::dispatch_review(&cmd, json),
        Commands::Dag { id } => commands::stats::cmd_dag(json, Some(id)),
        Commands::Estimate { id, epic_flag } => {
            let epic = resolve_epic(id, epic_flag, "estimate");
            commands::stats::cmd_estimate(json, &epic);
        }
        Commands::Replay {
            epic_id,
            dry_run,
            force,
        } => commands::epic::cmd_replay(json, &epic_id, dry_run, force),
        Commands::Diff { epic_id } => commands::epic::cmd_diff(json, &epic_id),

        // Nested groups
        Commands::Config { cmd } => admin::cmd_config(&cmd, json),
        Commands::Epic { cmd } => commands::epic::dispatch(&cmd, json, dry_run),
        Commands::Task { cmd } => commands::task::dispatch(&cmd, json, dry_run),
        Commands::Dep { cmd } => commands::dep::dispatch(&cmd, json, dry_run),
        Commands::Approval { cmd } => commands::approval::dispatch(&cmd, json),
        Commands::Checklist { cmd } => commands::checklist::dispatch(&cmd, json),
        Commands::Gap { cmd } => commands::gap::dispatch(&cmd, json),
        Commands::Log { cmd } => commands::log::dispatch(&cmd, json),
        Commands::Memory { cmd } => commands::memory::dispatch(&cmd, json),
        Commands::Outputs { cmd } => commands::outputs::dispatch(&cmd, json),
        Commands::Checkpoint { cmd } => commands::checkpoint::dispatch(&cmd, json),
        Commands::Stack { cmd } => commands::stack::dispatch(&cmd, json),
        Commands::Invariants { cmd } => commands::stack::dispatch_invariants(&cmd, json),
        Commands::Ralph { cmd } => commands::ralph::dispatch(&cmd, json),
        Commands::ScoutCache { cmd } => commands::scout_cache::dispatch(&cmd, json),
        Commands::Skill { cmd } => commands::skill::dispatch(&cmd, json),
        Commands::Rp { cmd } => commands::rp::dispatch(&cmd, json),
        Commands::Codex { cmd } => commands::codex::dispatch(&cmd, json),
        Commands::Hook { cmd } => commands::hook::dispatch(&cmd),
        Commands::Index { cmd } => commands::index::dispatch(&cmd, json),
        Commands::Graph { cmd } => commands::graph::dispatch(&cmd, json),
        Commands::Stats { cmd } => commands::stats::dispatch(&cmd, json),
        Commands::WorkerPhase { cmd } => workflow::dispatch_worker_phase(&cmd, json),
        Commands::Phase { cmd } => workflow::dispatch_pipeline_phase(&cmd, json),
        Commands::PlanDepth { request } => commands::plan_depth::cmd_plan_depth(json, &request),

        // Query
        Commands::Show { id } => query::cmd_show(json, id),
        Commands::Epics => query::cmd_epics(json),
        Commands::Tasks {
            epic,
            status,
            domain,
        } => query::cmd_tasks(json, epic, status, domain),
        Commands::List => query::cmd_list(json),
        Commands::Cat { id } => query::cmd_cat(id),
        Commands::Files { id, epic_flag } => {
            let epic = resolve_epic(id, epic_flag, "files");
            query::cmd_files(json, epic);
        }
        Commands::Lock { task, files, mode } => query::cmd_lock(json, task, files, mode),
        Commands::Unlock { task, files, all } => query::cmd_unlock(json, task, files, all, dry_run),
        Commands::LockCheck { file } => query::cmd_lock_check(json, file),
        Commands::Heartbeat { task } => query::cmd_heartbeat(json, task),

        // Workflow
        Commands::Ready { id, epic_flag } => {
            let epic = resolve_epic(id, epic_flag, "ready");
            workflow::cmd_ready(json, epic);
        }
        Commands::Next {
            epics_file,
            require_plan_review,
            require_completion_review,
        } => workflow::cmd_next(
            json,
            epics_file,
            require_plan_review,
            require_completion_review,
        ),
        Commands::Queue => workflow::cmd_queue(json),
        Commands::Start { id, force, note } => workflow::cmd_start(json, id, force, note),
        Commands::Done {
            id,
            summary_file,
            summary,
            evidence_json,
            evidence,
            force,
        } => workflow::cmd_done(
            json,
            id,
            summary_file,
            summary,
            evidence_json,
            evidence,
            force,
        ),
        Commands::Restart { id, dry_run, force } => workflow::cmd_restart(json, id, dry_run, force),
        Commands::Block {
            id,
            reason,
            reason_file,
        } => {
            let reason_text = if let Some(r) = reason {
                r
            } else if let Some(f) = reason_file {
                std::fs::read_to_string(&f).unwrap_or_else(|e| {
                    output::error_exit(&format!("Cannot read reason file: {e}"));
                })
            } else {
                output::error_exit("Either --reason or --reason-file is required");
            };
            workflow::cmd_block(json, id, reason_text)
        }
        Commands::Fail { id, reason, force } => workflow::cmd_fail(json, id, reason, force),
        Commands::Events { id, epic_flag } => {
            let epic = resolve_epic(id, epic_flag, "events");
            workflow::cmd_events(json, epic);
        }

        // File I/O
        Commands::WriteFile {
            path,
            content,
            stdin,
            append,
        } => commands::file::cmd_write_file(json, path, content, stdin, append),

        // Patch
        Commands::Patch { cmd } => commands::patch::dispatch(&cmd, json),

        // Project context
        Commands::ProjectContext { cmd } => commands::project_context::dispatch(&cmd, json),

        // Search
        Commands::Search { query, git, limit } => {
            commands::search::cmd_search(json, query, git, limit)
        }

        // Data exchange
        Commands::Export { epic, format } => admin::cmd_export(json, epic, format),
        Commands::Import => admin::cmd_import(json),

        // Code structure & repo map
        Commands::CodeStructure { cmd } => commands::code_structure::dispatch(&cmd, json),
        Commands::RepoMap { budget, path } => commands::repo_map::cmd_repo_map(json, budget, &path),

        // Intent-level commands
        Commands::Find { query, limit } => {
            commands::find::cmd_find(json, &query, limit);
        }
        Commands::Edit { file, old, new } => {
            commands::edit::cmd_edit(json, &file, &old, &new);
        }

        // Recovery
        Commands::Recover { epic, dry_run } => {
            commands::recover::cmd_recover(json, &epic, dry_run);
        }

        // Shell completions
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "flowctl", &mut std::io::stdout());
        }

        // Introspection
        Commands::CommandsList => cmd_commands_list(json),
        Commands::Describe { command } => cmd_describe(json, &command),
        Commands::Schema { command } => cmd_schema(json, &command),

        // ── V3 Commands ─────────────────────────────────────────────
        Commands::Serve => cmd_serve(),
        Commands::Migrate { target, dry_run } => cmd_migrate(&target, dry_run),
        Commands::Goal { action } => cmd_goal(json, action),
        Commands::PlanV3 { action } => cmd_plan_v3(json, action),
        Commands::Node { action } => cmd_node(json, action),
        Commands::Knowledge { action } => cmd_knowledge(json, action),
        Commands::Policy { action } => cmd_policy(action),
        Commands::Session { action } => cmd_session(action),
    }
}

/// List all commands with descriptions (machine-readable command catalog).
fn cmd_commands_list(json_mode: bool) {
    let cmd = Cli::command();
    let mut entries = Vec::new();

    for sub in cmd.get_subcommands() {
        let name = sub.get_name().to_string();
        let about = sub
            .get_about()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let has_subcommands = sub.get_subcommands().next().is_some();

        if has_subcommands {
            for nested in sub.get_subcommands() {
                let nested_name = format!("{} {}", name, nested.get_name());
                let nested_about = nested
                    .get_about()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                entries.push(serde_json::json!({
                    "command": nested_name,
                    "description": nested_about,
                    "group": classify_command(&name),
                }));
            }
        } else {
            entries.push(serde_json::json!({
                "command": name,
                "description": about,
                "group": classify_command(&name),
            }));
        }
    }

    if json_mode {
        output::json_output(serde_json::json!(entries));
    } else {
        for entry in &entries {
            println!(
                "{:<30} [{}] {}",
                entry["command"].as_str().unwrap_or(""),
                entry["group"].as_str().unwrap_or(""),
                entry["description"].as_str().unwrap_or(""),
            );
        }
    }
}

/// Describe a command's flags, types, defaults, and required status.
fn cmd_describe(json_mode: bool, command_path: &[String]) {
    let root = Cli::command();
    let target = find_subcommand(&root, command_path);

    match target {
        Some(cmd) => {
            let mut args = Vec::new();
            for arg in cmd.get_arguments() {
                if arg.get_id() == "help" || arg.get_id() == "version" {
                    continue;
                }
                let long = arg.get_long().map(|s| format!("--{s}"));
                let short = arg.get_short().map(|c| format!("-{c}"));
                let required = arg.is_required_set();
                let help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
                let default = arg
                    .get_default_values()
                    .iter()
                    .map(|v| v.to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                let possible = arg
                    .get_possible_values()
                    .iter()
                    .map(|v| v.get_name().to_string())
                    .collect::<Vec<_>>();

                args.push(serde_json::json!({
                    "name": arg.get_id().as_str(),
                    "long": long,
                    "short": short,
                    "required": required,
                    "help": help,
                    "default": if default.is_empty() { serde_json::Value::Null } else { serde_json::json!(default) },
                    "possible_values": if possible.is_empty() { serde_json::Value::Null } else { serde_json::json!(possible) },
                }));
            }

            if json_mode {
                output::json_output(serde_json::json!({
                    "command": command_path.join(" "),
                    "description": cmd.get_about().map(|a| a.to_string()).unwrap_or_default(),
                    "arguments": args,
                }));
            } else {
                println!(
                    "{}  — {}",
                    command_path.join(" "),
                    cmd.get_about().map(|a| a.to_string()).unwrap_or_default()
                );
                println!();
                for a in &args {
                    let flag = a["long"]
                        .as_str()
                        .unwrap_or(a["name"].as_str().unwrap_or(""));
                    let req = if a["required"].as_bool().unwrap_or(false) {
                        " (required)"
                    } else {
                        ""
                    };
                    println!("  {:<25} {}{}", flag, a["help"].as_str().unwrap_or(""), req);
                }
            }
        }
        None => {
            output::error_exit(&format!(
                "Unknown command '{}'. Run 'flowctl commands' to see all commands.",
                command_path.join(" ")
            ));
        }
    }
}

/// Show basic JSON schema for a command's output.
fn cmd_schema(json_mode: bool, command_path: &[String]) {
    let root = Cli::command();
    let target = find_subcommand(&root, command_path);

    match target {
        Some(cmd) => {
            // Output the standard envelope schema plus command-specific hints
            let schema = serde_json::json!({
                "command": command_path.join(" "),
                "output_envelope": {
                    "api_version": { "type": "integer", "description": "Output stability contract version" },
                    "success": { "type": "boolean", "description": "Whether the command succeeded" },
                },
                "error_envelope": {
                    "api_version": { "type": "integer" },
                    "success": { "const": false },
                    "error": { "type": "string" },
                },
                "exit_codes": {
                    "0": "success",
                    "1": "error",
                    "2": "blocked",
                },
                "description": cmd.get_about().map(|a| a.to_string()).unwrap_or_default(),
                "supports_json": true,
                "supports_compact": true,
                "supports_dry_run": cmd.get_arguments().any(|a| a.get_id() == "dry_run"),
            });

            if json_mode {
                output::json_output(schema);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&schema).unwrap()
                );
            }
        }
        None => {
            output::error_exit(&format!(
                "Unknown command '{}'. Run 'flowctl commands' to see all commands.",
                command_path.join(" ")
            ));
        }
    }
}

/// Find a subcommand by path (e.g., ["epic", "create"]).
fn find_subcommand<'a>(
    root: &'a clap::Command,
    path: &[String],
) -> Option<&'a clap::Command> {
    let mut current = root;
    for part in path {
        current = current
            .get_subcommands()
            .find(|s| s.get_name() == part.as_str())?;
    }
    Some(current)
}

/// Classify a command into its surface group.
fn classify_command(name: &str) -> &'static str {
    match name {
        "epic" | "task" | "dep" | "phase" | "worker-phase" | "ready" | "next" | "start"
        | "done" | "restart" | "block" | "fail" | "queue" | "events" => "Workflow",

        "show" | "epics" | "tasks" | "list" | "cat" | "files" | "lock" | "unlock"
        | "lock-check" | "heartbeat" => "Query",

        "init" | "detect" | "startup" | "status" | "doctor" | "validate" | "guard"
        | "pre-launch" | "worker-prompt" | "review" | "dag" | "estimate" | "replay" | "diff"
        | "recover" | "plan-depth" => "Admin",

        "graph" | "index" | "find" | "search" | "code-structure" | "repo-map" => "Code Intel",

        "rp" | "codex" | "hook" => "Integration",

        "config" | "stack" | "invariants" | "project-context" | "skill" | "scout-cache"
        | "ralph" | "stats" => "Meta / Config",

        "write-file" | "patch" | "export" | "import" | "checkpoint" | "outputs" | "log"
        | "memory" | "gap" | "approval" | "checklist" | "edit" => "Data / IO",

        "schema" | "describe" | "commands" | "completions" => "Introspection",

        _ => "Other",
    }
}

// ── V3 command handlers ────────────────────────────────────────────

fn cmd_serve() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    rt.block_on(async {
        if let Err(e) = flowctl_mcp::run_server(root).await {
            eprintln!("MCP server error: {e}");
            std::process::exit(1);
        }
    });
}

fn cmd_migrate(target: &str, dry_run: bool) {
    if target != "v3" {
        output::error_exit(&format!("Unknown migration target: {target}. Only 'v3' is supported."));
    }

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_dir = root.join(".flow");
    if !flow_dir.exists() {
        output::error_exit(".flow/ directory not found. Nothing to migrate.");
    }

    let epics_dir = flow_dir.join("epics");
    let goals_dir = flow_dir.join("goals");
    let knowledge_dir = flow_dir.join("knowledge");

    let mut migrated_goals = 0u32;
    let mut migrated_learnings = 0u32;

    // Migrate epics → goals
    if epics_dir.exists() {
        for entry in std::fs::read_dir(&epics_dir).unwrap_or_else(|e| {
            output::error_exit(&format!("Failed to read epics dir: {e}"));
        }) {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(epic) = serde_json::from_str::<serde_json::Value>(&data) {
                        let epic_id = epic.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let title = epic.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let goal_id = format!("g-{epic_id}");

                        if dry_run {
                            println!("[dry-run] Would migrate epic {epic_id} → goal {goal_id}");
                        } else {
                            let goal_dir = goals_dir.join(&goal_id);
                            std::fs::create_dir_all(&goal_dir).ok();
                            let goal = serde_json::json!({
                                "id": goal_id,
                                "request": title,
                                "intent": "execute",
                                "planning_mode": "graph",
                                "success_model": "criteria",
                                "status": epic.get("status").and_then(|v| v.as_str()).unwrap_or("open"),
                                "current_plan_rev": 0,
                                "acceptance_criteria": [],
                                "constraints": [],
                                "known_facts": [],
                                "open_questions": [],
                                "created_at": epic.get("created_at").unwrap_or(&serde_json::json!("")),
                                "updated_at": epic.get("updated_at").unwrap_or(&serde_json::json!("")),
                            });
                            let json = serde_json::to_string_pretty(&goal).unwrap();
                            std::fs::write(goal_dir.join("goal.json"), &json).ok();
                        }
                        migrated_goals += 1;
                    }
                }
            }
        }
    }

    // Migrate tasks → plan nodes within each goal directory
    let tasks_dir = flow_dir.join("tasks");
    let mut migrated_tasks = 0u32;
    if tasks_dir.exists() {
        for entry in std::fs::read_dir(&tasks_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(task) = serde_json::from_str::<serde_json::Value>(&data) {
                        let epic_id = task.get("epic").and_then(|v| v.as_str()).unwrap_or("");
                        let task_id = task.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let title = task.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let goal_id = format!("g-{epic_id}");

                        if dry_run {
                            println!("[dry-run] Would migrate task {task_id} → node in goal {goal_id}");
                        } else {
                            // Create plan node inside goal's plans directory
                            let plans_dir = goals_dir.join(&goal_id).join("plans");
                            if plans_dir.parent().is_some_and(|p| p.exists()) {
                                std::fs::create_dir_all(&plans_dir).ok();
                                let plan_path = plans_dir.join("0001.json");
                                if !plan_path.exists() {
                                    let plan = serde_json::json!({
                                        "goal_id": goal_id,
                                        "rev": 1,
                                        "nodes": [{
                                            "id": task_id,
                                            "objective": title,
                                            "constraints": [],
                                            "owned_files": task.get("files").unwrap_or(&serde_json::json!([])),
                                            "risk": {"estimated_scope":"small","needs_deeper_qa":false,"touches_interfaces":false,"risk_rationale":"migrated","guard_depth":"standard"},
                                            "status": task.get("status").and_then(|v| v.as_str()).unwrap_or("ready"),
                                            "injected_patterns": [],
                                        }],
                                        "edges": [],
                                        "rationale": "migrated from v1 tasks",
                                        "trigger": {"kind":"initial"},
                                        "created_at": task.get("created_at").unwrap_or(&serde_json::json!("")),
                                    });
                                    let json = serde_json::to_string_pretty(&plan).unwrap();
                                    std::fs::write(&plan_path, &json).ok();
                                }
                            }
                        }
                        migrated_tasks += 1;
                    }
                }
            }
        }
    }

    // Migrate memory → knowledge/learnings
    let memory_file = flow_dir.join("memory").join("entries.jsonl");
    if memory_file.exists() {
        if let Ok(data) = std::fs::read_to_string(&memory_file) {
            if !dry_run {
                let learnings_dir = knowledge_dir.join("learnings");
                std::fs::create_dir_all(&learnings_dir).ok();
            }
            for line in data.lines() {
                if line.trim().is_empty() { continue; }
                if let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) {
                    let content = entry.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if content.is_empty() { continue; }

                    if dry_run {
                        println!("[dry-run] Would migrate memory entry → learning");
                    } else {
                        let learning = serde_json::json!({
                            "id": format!("l-migrated-{migrated_learnings}"),
                            "goal_id": "migrated",
                            "kind": "discovery",
                            "content": content,
                            "tags": ["migrated"],
                            "created_at": chrono::Utc::now(),
                            "verified": false,
                            "use_count": 0,
                        });
                        let learnings_dir = knowledge_dir.join("learnings");
                        let json = serde_json::to_string_pretty(&learning).unwrap();
                        std::fs::write(
                            learnings_dir.join(format!("l-migrated-{migrated_learnings}.json")),
                            &json,
                        ).ok();
                    }
                    migrated_learnings += 1;
                }
            }
        }
    }

    // Archive originals
    if !dry_run && (migrated_goals > 0 || migrated_learnings > 0) {
        let archive_dir = flow_dir.join(".archive").join("v1");
        std::fs::create_dir_all(&archive_dir).ok();
        if epics_dir.exists() {
            let dest = archive_dir.join("epics");
            if !dest.exists() {
                std::fs::rename(&epics_dir, &dest).ok();
            }
        }
    }

    output::json_output(serde_json::json!({
        "target": target,
        "dry_run": dry_run,
        "migrated_goals": migrated_goals,
        "migrated_tasks": migrated_tasks,
        "migrated_learnings": migrated_learnings,
    }));
}

fn cmd_goal(_json_mode: bool, action: GoalAction) {
    use flowctl_core::domain::goal::GoalIntent;
    use flowctl_core::engine::goal_engine::GoalEngine;
    use flowctl_core::knowledge::Learner;

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_root = root.join(".flow");
    let engine = GoalEngine::new(&flow_root);

    match action {
        GoalAction::Open { request, intent } => {
            let gi = match intent.as_str() {
                "plan" => GoalIntent::Plan,
                "brainstorm" => GoalIntent::Brainstorm,
                _ => GoalIntent::Execute,
            };
            match engine.open(&request, gi) {
                Ok(goal) => output::json_output(serde_json::to_value(&goal).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
        GoalAction::Status { goal_id } => {
            match engine.status(&goal_id) {
                Ok(goal) => output::json_output(serde_json::to_value(&goal).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
        GoalAction::Close { goal_id } => {
            let learner = Learner::new(&flow_root);
            match engine.close(&goal_id) {
                Ok(goal) => {
                    let _ = learner.compound(&goal_id);
                    output::json_output(serde_json::to_value(&goal).unwrap());
                }
                Err(e) => output::error_exit(&e),
            }
        }
    }
}

fn cmd_plan_v3(_json_mode: bool, action: PlanV3Action) {
    use flowctl_core::engine::planner::Planner;
    use flowctl_core::engine::scheduler::Scheduler;
    use flowctl_core::knowledge::Learner;

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_root = root.join(".flow");

    match action {
        PlanV3Action::Build { goal_id } => {
            let planner = Planner::new(&flow_root);
            // Return current latest plan or error if none
            match planner.get_latest(&goal_id) {
                Ok(plan) => {
                    let levels = plan.compute_levels();
                    let mut val = serde_json::to_value(&plan).unwrap();
                    if let serde_json::Value::Object(ref mut m) = val {
                        m.insert("levels".into(), serde_json::to_value(&levels).unwrap());
                    }
                    output::json_output(val);
                }
                Err(e) => output::error_exit(&format!("no plan for {goal_id}: {e}")),
            }
        }
        PlanV3Action::Next { goal_id } => {
            let scheduler = Scheduler::new(&flow_root);
            let learner = Learner::new(&flow_root);
            match scheduler.ready_nodes(&goal_id) {
                Ok(mut nodes) => {
                    for node in &mut nodes {
                        if let Ok(patterns) = learner.inject_for_node(&node.objective, 3) {
                            node.injected_patterns = patterns.iter().map(|p| format!("{}: {}", p.name, p.approach)).collect();
                        }
                    }
                    output::json_output(serde_json::to_value(&nodes).unwrap());
                }
                Err(e) => output::error_exit(&e),
            }
        }
    }
}

fn cmd_node(_json_mode: bool, action: NodeAction) {
    use flowctl_core::engine::scheduler::Scheduler;
    use flowctl_core::engine::escalation::EscalationEngine;

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_root = root.join(".flow");
    let scheduler = Scheduler::new(&flow_root);

    match action {
        NodeAction::Start { goal_id, node_id } => {
            match scheduler.start_node(&goal_id, &node_id) {
                Ok(plan) => {
                    let node = plan.nodes.iter().find(|n| n.id == node_id);
                    output::json_output(serde_json::to_value(&node).unwrap());
                }
                Err(e) => output::error_exit(&e),
            }
        }
        NodeAction::Finish { goal_id, node_id, summary } => {
            match scheduler.finish_node(&goal_id, &node_id) {
                Ok(newly_ready) => {
                    let val = serde_json::json!({
                        "node_id": node_id,
                        "status": "done",
                        "summary": summary,
                        "newly_ready": newly_ready.iter().map(|n| &n.id).collect::<Vec<_>>(),
                    });
                    output::json_output(val);
                }
                Err(e) => output::error_exit(&e),
            }
        }
        NodeAction::Fail { goal_id, node_id, error } => {
            let _ = scheduler.fail_node(&goal_id, &node_id);
            let escalation = EscalationEngine::new(&flow_root);
            match escalation.handle_failure(&goal_id, &node_id, &error) {
                Ok(action) => output::json_output(serde_json::to_value(&action).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
    }
}

fn cmd_knowledge(_json_mode: bool, action: KnowledgeAction) {
    use flowctl_core::knowledge::{Learner, LearningKind};

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_root = root.join(".flow");
    let learner = Learner::new(&flow_root);

    match action {
        KnowledgeAction::Search { query, limit } => {
            match learner.search(&query, limit) {
                Ok(result) => output::json_output(serde_json::to_value(&result).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
        KnowledgeAction::Record { goal_id, content, kind } => {
            let lk = match kind.as_str() {
                "failure" => LearningKind::Failure,
                "discovery" => LearningKind::Discovery,
                "pitfall" => LearningKind::Pitfall,
                _ => LearningKind::Success,
            };
            match learner.record(&goal_id, None, lk, &content, vec![]) {
                Ok(learning) => output::json_output(serde_json::to_value(&learning).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
        KnowledgeAction::Compound { goal_id } => {
            match learner.compound(&goal_id) {
                Ok(patterns) => output::json_output(serde_json::to_value(&patterns).unwrap()),
                Err(e) => output::error_exit(&e),
            }
        }
        KnowledgeAction::Refresh => {
            match learner.refresh_stale() {
                Ok(count) => output::json_output(serde_json::json!({"decayed_count": count})),
                Err(e) => output::error_exit(&e),
            }
        }
    }
}

fn cmd_policy(action: PolicyAction) {
    use flowctl_core::quality::{PolicyEngine, policy::{PolicyContext, FileLock, PolicyDecision}};

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_dir = root.join(".flow");
    let engine = PolicyEngine::new();

    match action {
        PolicyAction::CheckHook { tool, file, node } => {
            let locks = flowctl_core::json_store::locks_read(&flow_dir).unwrap_or_default();
            let active_locks: Vec<FileLock> = locks.iter().map(|l| FileLock {
                file_path: l.file_path.clone(),
                node_id: l.task_id.clone(),
                mode: l.mode.clone(),
            }).collect();
            let ctx = PolicyContext {
                active_locks,
                guard_ran: false,
                current_node: node,
            };
            let decision = engine.check_hook(&tool, file.as_deref(), &ctx);
            match decision {
                PolicyDecision::Allow => {
                    output::json_output(serde_json::json!({"decision": "allow"}));
                }
                PolicyDecision::Warn(msg) => {
                    output::json_output(serde_json::json!({"decision": "warn", "message": msg}));
                }
                PolicyDecision::Block(msg) => {
                    output::json_output(serde_json::json!({"decision": "block", "message": msg}));
                    std::process::exit(2);
                }
            }
        }
    }
}

fn cmd_session(action: SessionAction) {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let flow_dir = root.join(".flow");

    match action {
        SessionAction::Snapshot => {
            // Save current state snapshot: active goals, locks, active nodes
            let goal_store = flowctl_core::storage::GoalStore::new(&flow_dir);
            let goals = goal_store.list().unwrap_or_default();
            let locks = flowctl_core::json_store::locks_read(&flow_dir).unwrap_or_default();

            // Write to runtime/sessions.json
            let runtime_dir = flow_dir.join("runtime");
            std::fs::create_dir_all(&runtime_dir).ok();
            let snapshot = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "active_goals": goals,
                "active_locks": locks.len(),
            });
            let json = serde_json::to_string_pretty(&snapshot).unwrap();
            std::fs::write(runtime_dir.join("sessions.json"), &json).ok();

            output::json_output(serde_json::json!({
                "status": "snapshot_saved",
                "goals": goals.len(),
                "locks": locks.len(),
            }));
        }
        SessionAction::List => {
            let runtime_dir = flow_dir.join("runtime");
            let path = runtime_dir.join("sessions.json");
            if path.exists() {
                let data = std::fs::read_to_string(&path).unwrap_or_default();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                    output::json_output(val);
                } else {
                    output::json_output(serde_json::json!({"sessions": []}));
                }
            } else {
                output::json_output(serde_json::json!({"sessions": []}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_cli() {
        // Clap's built-in verification that all derive attributes are valid.
        Cli::command().debug_assert();
    }
}
