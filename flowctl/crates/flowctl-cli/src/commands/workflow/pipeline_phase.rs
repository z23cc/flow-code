//! Pipeline phase commands: `flowctl phase next` and `flowctl phase done`.
//!
//! These commands manage the epic-level pipeline progression stored in the
//! `pipeline_progress` table. Distinct from worker-phase (task-level phases).

use clap::Subcommand;
use serde_json::json;

use flowctl_core::pipeline::PipelinePhase;

use crate::output::{error_exit, json_output};

use super::require_db;

/// Pipeline phase subcommands.
#[derive(Subcommand, Debug)]
pub enum PipelinePhaseCmd {
    /// Show current pipeline phase for an epic.
    Next {
        /// Epic ID.
        #[arg(long)]
        epic: String,
    },
    /// Mark current phase as done and advance to next.
    Done {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Phase name to mark done (must match current phase).
        #[arg(long)]
        phase: String,
    },
}

/// Dispatch pipeline phase subcommands.
pub fn dispatch_pipeline_phase(cmd: &PipelinePhaseCmd, json: bool) {
    match cmd {
        PipelinePhaseCmd::Next { epic } => cmd_phase_next(json, epic),
        PipelinePhaseCmd::Done { epic, phase } => cmd_phase_done(json, epic, phase),
    }
}

/// Read current pipeline phase from DB. If no row exists, initialize to Plan.
fn get_or_init_phase(epic_id: &str) -> PipelinePhase {
    let conn = require_db();
    let raw = conn.inner_conn();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let mut rows = raw
            .query(
                "SELECT phase FROM pipeline_progress WHERE epic_id = ?1",
                libsql::params![epic_id],
            )
            .await
            .unwrap_or_else(|e| {
                error_exit(&format!("DB query failed: {e}"));
            });

        if let Some(row) = rows.next().await.unwrap_or(None) {
            let phase_str: String = row.get(0).unwrap_or_else(|_| "plan".to_string());
            PipelinePhase::parse(&phase_str).unwrap_or(PipelinePhase::Plan)
        } else {
            // No row — initialize with Plan phase.
            let now = chrono::Utc::now().to_rfc3339();
            raw.execute(
                "INSERT INTO pipeline_progress (epic_id, phase, started_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                libsql::params![epic_id, "plan", now.clone(), now],
            )
            .await
            .unwrap_or_else(|e| {
                error_exit(&format!("DB insert failed: {e}"));
            });
            PipelinePhase::Plan
        }
    })
}

/// Update pipeline phase in DB.
fn update_phase(epic_id: &str, new_phase: &PipelinePhase) {
    let conn = require_db();
    let raw = conn.inner_conn();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let now = chrono::Utc::now().to_rfc3339();
        raw.execute(
            "UPDATE pipeline_progress SET phase = ?1, updated_at = ?2 WHERE epic_id = ?3",
            libsql::params![new_phase.as_str(), now, epic_id],
        )
        .await
        .unwrap_or_else(|e| {
            error_exit(&format!("DB update failed: {e}"));
        });
    });
}

/// `flowctl phase next --epic <id> --json`
fn cmd_phase_next(json: bool, epic_id: &str) {
    let current = get_or_init_phase(epic_id);
    let all_done = current.is_terminal();

    if json {
        json_output(json!({
            "phase": current.as_str(),
            "prompt": current.prompt_template(),
            "all_done": all_done,
        }));
    } else {
        println!("Phase: {}", current);
        println!("Prompt: {}", current.prompt_template());
        if all_done {
            println!("Status: all phases complete");
        }
    }
}

/// `flowctl phase done --epic <id> --phase <name> --json`
fn cmd_phase_done(json: bool, epic_id: &str, phase_name: &str) {
    let requested = match PipelinePhase::parse(phase_name) {
        Some(p) => p,
        None => {
            let valid: Vec<&str> = PipelinePhase::all().iter().map(|p| p.as_str()).collect();
            error_exit(&format!(
                "Unknown phase '{}'. Valid phases: {}",
                phase_name,
                valid.join(", ")
            ));
        }
    };

    let current = get_or_init_phase(epic_id);

    if requested != current {
        error_exit(&format!(
            "Phase mismatch: current phase is '{}', but '{}' was requested. \
             Phases must be completed in order.",
            current, requested
        ));
    }

    if current.is_terminal() {
        error_exit("Pipeline is already at the terminal phase (close). No further advancement.");
    }

    let next_phase = current.next().expect("non-terminal phase has a next");
    update_phase(epic_id, &next_phase);

    if json {
        json_output(json!({
            "previous_phase": current.as_str(),
            "phase": next_phase.as_str(),
            "prompt": next_phase.prompt_template(),
            "all_done": next_phase.is_terminal(),
        }));
    } else {
        println!("Advanced: {} → {}", current, next_phase);
        println!("Prompt: {}", next_phase.prompt_template());
        if next_phase.is_terminal() {
            println!("Status: all phases complete");
        }
    }
}
