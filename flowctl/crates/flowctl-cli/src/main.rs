//! flowctl CLI entry point.
//!
//! Clap 4 derive-based CLI matching the full Python flowctl command surface.
//! All commands are registered as stubs that return "not yet implemented".

#![forbid(unsafe_code)]

mod commands;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

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
#[command(name = "flowctl", version, about = "Development orchestration engine")]
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
    // ── Admin / top-level ────────────────────────────────────────────
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

    // ── Shell completions ────────────────────────────────────────────
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },


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
            output::error_exit(&format!("{cmd_name} requires an epic ID (positional or --epic)"));
        })
    }

    match cli.command {
        // Admin / top-level
        Commands::Init => admin::cmd_init(json),
        Commands::Paths => commands::paths::cmd_paths(json),
        Commands::Detect => admin::cmd_detect(json),
        Commands::Startup => admin::cmd_startup(json),
        Commands::Status { interrupted, dag, epic, progress } => {
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
        Commands::WorkerPrompt { task, tdd, review, bootstrap: _, inline_skills } => {
            admin::cmd_worker_prompt(json, task, tdd, review, inline_skills)
        }

        Commands::Review { cmd } => admin::dispatch_review(&cmd, json),
        Commands::Dag { id } => commands::stats::cmd_dag(json, Some(id)),
        Commands::Estimate { id, epic_flag } => {
            let epic = resolve_epic(id, epic_flag, "estimate");
            commands::stats::cmd_estimate(json, &epic);
        }
        Commands::Replay { epic_id, dry_run, force } => commands::epic::cmd_replay(json, &epic_id, dry_run, force),
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
        Commands::Unlock { task, files, all } => query::cmd_unlock(json, task, files, all),
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
        Commands::Block { id, reason, reason_file } => {
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
        Commands::WriteFile { path, content, stdin, append } => {
            commands::file::cmd_write_file(json, path, content, stdin, append)
        }

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
