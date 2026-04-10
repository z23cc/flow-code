//! Epic command type definitions.

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum EpicCmd {
    /// Create a new epic.
    Create {
        /// Epic title.
        #[arg(long, required_unless_present = "input_json")]
        title: Option<String>,
        /// Branch name.
        #[arg(long)]
        branch: Option<String>,
        /// JSON payload input (inline, @file, or - for stdin).
        /// Fields: {"title": "...", "branch": "..."}
        #[arg(long)]
        input_json: Option<String>,
    },
    /// Set epic spec from file or inline text.
    Plan {
        /// Epic ID.
        id: String,
        /// Markdown file (use '-' for stdin).
        #[arg(long)]
        file: Option<String>,
        /// Inline spec text (alternative to --file).
        #[arg(long)]
        spec: Option<String>,
    },
    /// Set plan review status.
    Review {
        /// Epic ID.
        id: String,
        /// Review status: ship, needs_work, unknown.
        #[arg(value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set completion review status.
    Completion {
        /// Epic ID.
        id: String,
        /// Review status: ship, needs_work, unknown.
        #[arg(value_parser = ["ship", "needs_work", "unknown"])]
        status: String,
    },
    /// Set epic branch name.
    Branch {
        /// Epic ID.
        id: String,
        /// Branch name.
        name: String,
    },
    /// Rename epic title.
    Title {
        /// Epic ID.
        id: String,
        /// New title.
        #[arg(long)]
        title: String,
    },
    /// Close an epic.
    Close {
        /// Epic ID.
        id: String,
        /// Bypass gap registry gate.
        #[arg(long)]
        skip_gap_check: bool,
    },
    /// Reopen a closed epic.
    Reopen {
        /// Epic ID.
        id: String,
    },
    /// Archive closed epic to .flow/.archive/.
    Archive {
        /// Epic ID.
        id: String,
        /// Archive even if not closed.
        #[arg(long)]
        force: bool,
    },
    /// Archive all closed epics at once.
    Clean,
    /// Audit epic task-coverage vs original spec (advisory only).
    ///
    /// Assembles the epic spec, task list, and prior audit context into a
    /// payload consumed by `agents/epic-auditor.md`. Writes the assembled
    /// payload to `.flow/reviews/epic-audit-<id>-<timestamp>.json`. Advisory
    /// only — never mutates epic/tasks/gaps.
    Audit {
        /// Epic ID.
        id: String,
        /// Force a new audit even if a recent (<24h) receipt exists.
        #[arg(long)]
        force: bool,
    },
    /// Add epic-level dependency.
    AddDep {
        /// Epic ID.
        epic: String,
        /// Epic ID to depend on.
        depends_on: String,
    },
    /// Remove epic-level dependency.
    RmDep {
        /// Epic ID.
        epic: String,
        /// Epic ID to remove from deps.
        depends_on: String,
    },
    /// Set default backend specs.
    SetBackend {
        /// Epic ID.
        id: String,
        /// Default impl backend spec.
        #[arg(long = "impl")]
        impl_spec: Option<String>,
        /// Default review backend spec.
        #[arg(long)]
        review: Option<String>,
        /// Default sync backend spec.
        #[arg(long)]
        sync: Option<String>,
    },
    /// Set or clear auto_execute_pending marker.
    AutoExec {
        /// Epic ID.
        id: String,
        /// Mark auto-execute as pending.
        #[arg(long)]
        pending: bool,
        /// Clear auto-execute pending marker.
        #[arg(long)]
        done: bool,
    },
}
