//! flowctl CLI entry point.
//!
//! Clap 4 derive-based CLI matching the full Python flowctl command surface.
//! All commands are registered as stubs that return "not yet implemented".

mod commands;
mod diagnostics;
mod output;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use commands::{
    admin::{self, ConfigCmd},
    approval::ApprovalCmd,
    checkpoint::CheckpointCmd,
    codex::CodexCmd,
    dep::DepCmd,
    epic::EpicCmd,
    gap::GapCmd,
    hook::HookCmd,
    memory::MemoryCmd,
    outputs::OutputsCmd,
    query,
    ralph::RalphCmd,
    rp::RpCmd,
    stack::{InvariantsCmd, StackCmd},
    stats::StatsCmd,
    task::TaskCmd,
    workflow::{self, WorkerPhaseCmd},
};
use output::OutputOpts;

/// flowctl - development orchestration engine.
#[derive(Parser, Debug)]
#[command(name = "flowctl", version, about = "Development orchestration engine")]
struct Cli {
    #[command(flatten)]
    output: OutputOpts,

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
    /// Show .flow state and active runs.
    Status {
        /// Detect interrupted epics with undone tasks.
        #[arg(long)]
        interrupted: bool,
        /// Render ASCII DAG of task dependencies for the active epic.
        #[arg(long)]
        dag: bool,
        /// Epic ID (required with --dag).
        #[arg(long)]
        epic: Option<String>,
    },
    /// Run comprehensive state health diagnostics.
    Doctor,
    /// Validate epic or all.
    Validate {
        /// Epic ID.
        #[arg(long)]
        epic: Option<String>,
        /// Validate all epics and tasks.
        #[arg(long)]
        all: bool,
    },
    /// Show resolved state directory path.
    StatePath {
        /// Task ID to show state file path for.
        #[arg(long)]
        task: Option<String>,
    },
    /// Migrate runtime state from definition files to state-dir.
    MigrateState {
        /// Remove runtime fields from definition files after migration.
        #[arg(long)]
        clean: bool,
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
    },

    /// Render ASCII DAG of task dependencies.
    Dag {
        /// Epic ID.
        id: String,
    },
    /// Estimate remaining time for an epic based on historical durations.
    Estimate {
        /// Epic ID.
        #[arg(long)]
        epic: String,
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
    /// Requirement gap registry.
    Gap {
        #[command(subcommand)]
        cmd: GapCmd,
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
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
    /// Lock files for a task (Teams mode).
    Lock {
        /// Task ID that owns the files.
        #[arg(long)]
        task: String,
        /// Comma-separated file paths to lock.
        #[arg(long)]
        files: String,
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
    /// Check file lock status (Teams mode).
    LockCheck {
        /// Specific file to check.
        #[arg(long)]
        file: Option<String>,
    },

    // ── Workflow commands ─────────────────────────────────────────────
    /// List ready tasks.
    Ready {
        /// Epic ID.
        #[arg(long)]
        epic: String,
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
    /// Start task.
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

    match cli.command {
        // Admin / top-level
        Commands::Init => admin::cmd_init(json),
        Commands::Detect => admin::cmd_detect(json),
        Commands::Status { interrupted, dag, epic } => {
            if dag {
                commands::stats::cmd_dag(json, epic);
            } else {
                admin::cmd_status(json, interrupted);
            }
        }
        Commands::Doctor => admin::cmd_doctor(json),
        Commands::Validate { epic, all } => admin::cmd_validate(json, epic, all),
        Commands::StatePath { task } => admin::cmd_state_path(json, task),
        Commands::MigrateState { clean } => admin::cmd_migrate_state(json, clean),
        Commands::ReviewBackend { compare, epic } => admin::cmd_review_backend(json, compare, epic),
        Commands::ParseFindings {
            file,
            epic,
            register,
            source,
        } => admin::cmd_parse_findings(json, file, epic, register, source),
        Commands::Guard { layer } => admin::cmd_guard(json, layer),
        Commands::WorkerPrompt { task, tdd, review } => {
            admin::cmd_worker_prompt(json, task, tdd, review)
        }

        Commands::Dag { id } => commands::stats::cmd_dag(json, Some(id)),
        Commands::Estimate { epic } => commands::stats::cmd_estimate(json, &epic),
        Commands::Replay { epic_id, dry_run, force } => commands::epic::cmd_replay(json, &epic_id, dry_run, force),
        Commands::Diff { epic_id } => commands::epic::cmd_diff(json, &epic_id),

        // Nested groups
        Commands::Config { cmd } => admin::cmd_config(&cmd, json),
        Commands::Epic { cmd } => commands::epic::dispatch(&cmd, json),
        Commands::Task { cmd } => commands::task::dispatch(&cmd, json),
        Commands::Dep { cmd } => commands::dep::dispatch(&cmd, json),
        Commands::Approval { cmd } => commands::approval::dispatch(&cmd, json),
        Commands::Gap { cmd } => commands::gap::dispatch(&cmd, json),
        Commands::Memory { cmd } => commands::memory::dispatch(&cmd, json),
        Commands::Outputs { cmd } => commands::outputs::dispatch(&cmd, json),
        Commands::Checkpoint { cmd } => commands::checkpoint::dispatch(&cmd, json),
        Commands::Stack { cmd } => commands::stack::dispatch(&cmd, json),
        Commands::Invariants { cmd } => commands::stack::dispatch_invariants(&cmd, json),
        Commands::Ralph { cmd } => commands::ralph::dispatch(&cmd, json),
        Commands::Rp { cmd } => commands::rp::dispatch(&cmd, json),
        Commands::Codex { cmd } => commands::codex::dispatch(&cmd, json),
        Commands::Hook { cmd } => commands::hook::dispatch(&cmd),
        Commands::Stats { cmd } => commands::stats::dispatch(&cmd, json),
        Commands::WorkerPhase { cmd } => workflow::dispatch_worker_phase(&cmd, json),

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
        Commands::Files { epic } => query::cmd_files(json, epic),
        Commands::Lock { task, files } => query::cmd_lock(json, task, files),
        Commands::Unlock { task, files, all } => query::cmd_unlock(json, task, files, all),
        Commands::LockCheck { file } => query::cmd_lock_check(json, file),

        // Workflow
        Commands::Ready { epic } => workflow::cmd_ready(json, epic),
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

        // Data exchange
        Commands::Export { epic, format } => admin::cmd_export(json, epic, format),
        Commands::Import => admin::cmd_import(json),

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
