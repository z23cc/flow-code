//! Pipeline phase commands: `flowctl phase next` and `flowctl phase done`.
//!
//! These commands manage the epic-level pipeline progression stored in
//! `.state/pipeline.json`. Distinct from worker-phase (task-level phases).

use clap::Subcommand;
use serde_json::json;

use flowctl_core::pipeline::PipelinePhase;
use flowctl_core::json_store;

use crate::output::{error_exit, json_output};

use super::ensure_flow_exists;

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
    ///
    /// Some phases require evidence to advance:
    /// - plan_review: --score N (self-review score out of 30)
    /// - work: --guard-ran (confirms guard was executed between waves)
    /// - impl_review: --score N (self-review score out of 30)
    /// - close: --guard-ran (confirms final guard was executed)
    Done {
        /// Epic ID.
        #[arg(long)]
        epic: String,
        /// Phase name to mark done (must match current phase).
        #[arg(long)]
        phase: String,
        /// Self-review score (required for plan_review and impl_review).
        #[arg(long)]
        score: Option<u32>,
        /// Review evidence text (required with --score). Must contain actual Q&A content.
        #[arg(long)]
        evidence: Option<String>,
        /// Confirms guard was executed (required for work and close).
        #[arg(long)]
        guard_ran: bool,
        /// Skip evidence requirements (escape hatch for --quick path).
        #[arg(long)]
        no_gate: bool,
    },
}

/// Dispatch pipeline phase subcommands.
pub fn dispatch_pipeline_phase(cmd: &PipelinePhaseCmd, json: bool) {
    match cmd {
        PipelinePhaseCmd::Next { epic } => cmd_phase_next(json, epic),
        PipelinePhaseCmd::Done { epic, phase, score, evidence, guard_ran, no_gate } => {
            cmd_phase_done(json, epic, phase, *score, evidence.as_deref(), *guard_ran, *no_gate)
        }
    }
}

/// Read current pipeline phase from file. If no entry exists, initialize to Brainstorm.
fn get_or_init_phase(flow_dir: &std::path::Path, epic_id: &str) -> PipelinePhase {
    match json_store::pipeline_read(flow_dir, epic_id) {
        Ok(Some(phase_str)) => PipelinePhase::parse(&phase_str).unwrap_or(PipelinePhase::Brainstorm),
        _ => {
            // No entry — initialize with Brainstorm phase.
            let _ = json_store::pipeline_write(flow_dir, epic_id, "brainstorm");
            PipelinePhase::Brainstorm
        }
    }
}

/// Validate review evidence. Requires minimum content length to prevent
/// score fabrication (audit finding: AI fills "Q1:3 Q2:2..." without
/// actually running forcing questions).
fn validate_score_evidence(evidence: &str, expected_questions: usize, phase: &str) {
    // Minimum 200 chars — short scores like "Q1:3 Q2:2..." are ~40 chars.
    // Actual review content with reasoning is 500+ chars.
    let min_chars = 200;
    if evidence.len() < min_chars {
        error_exit(&format!(
            "{phase} evidence is too short ({} chars, minimum {min_chars}).\n\
             Evidence must contain actual review findings, not just scores.\n\
             Example: --evidence \"Q1(Right problem):3 evidence-backed, Q2(Do-nothing):2 risk of X, ...\"\n\
             Run the actual forcing questions and include your reasoning.",
            evidence.len()
        ));
    }

    // Count Q-entries to verify coverage
    let q_count = evidence
        .split("Q")
        .filter(|s| !s.is_empty() && s.chars().next().map_or(false, |c| c.is_ascii_digit()))
        .count();

    if q_count < expected_questions / 2 {
        error_exit(&format!(
            "{phase} evidence references only {q_count} questions (expected ~{expected_questions}).\n\
             Include findings for each question: Q1(...):score reason, Q2(...):score reason, ..."
        ));
    }

    eprintln!("Evidence: {q_count} questions referenced, {} chars", evidence.len());
}

/// Update pipeline phase in file store.
fn update_phase(flow_dir: &std::path::Path, epic_id: &str, new_phase: &PipelinePhase) {
    if let Err(e) = json_store::pipeline_write(flow_dir, epic_id, new_phase.as_str()) {
        error_exit(&format!("Failed to update pipeline phase: {e}"));
    }
}

/// `flowctl phase next --epic <id> --json`
fn cmd_phase_next(json: bool, epic_id: &str) {
    let flow_dir = ensure_flow_exists();
    let current = get_or_init_phase(&flow_dir, epic_id);
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

/// `flowctl phase done --epic <id> --phase <name> [--score N] [--guard-ran] [--no-gate] --json`
fn cmd_phase_done(json: bool, epic_id: &str, phase_name: &str, score: Option<u32>, evidence: Option<&str>, guard_ran: bool, no_gate: bool) {
    let flow_dir = ensure_flow_exists();

    let requested = match PipelinePhase::parse(phase_name) {
        Some(p) => p,
        None => {
            let valid: Vec<&str> = PipelinePhase::all().iter().map(PipelinePhase::as_str).collect();
            error_exit(&format!(
                "Unknown phase '{}'. Valid phases: {}",
                phase_name,
                valid.join(", ")
            ));
        }
    };

    let current = get_or_init_phase(&flow_dir, epic_id);

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

    // ── Evidence gates (skip with --no-gate for quick path) ────────
    if !no_gate {
        match current {
            PipelinePhase::PlanReview => {
                if score.is_none() {
                    error_exit(
                        "plan_review requires --score N AND --evidence \"Q1:3 Q2:2 ...\" (scores per question).\n\
                         Run the 10 forcing questions, score each 1-3, then:\n\
                         $FLOWCTL phase done --epic ID --phase plan_review --score 25 --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\" --json"
                    );
                }
                if let Some(ev) = evidence {
                    validate_score_evidence(ev, 10, "plan_review");
                } else if score.is_some() {
                    error_exit(
                        "plan_review requires --evidence with per-question scores.\n\
                         Example: --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\""
                    );
                }
            }
            PipelinePhase::Work => {
                if !guard_ran {
                    error_exit(
                        "work phase requires --guard-ran (confirms guard was executed between waves).\n\
                         Run $FLOWCTL guard first, then:\n\
                         $FLOWCTL phase done --epic ID --phase work --guard-ran --json"
                    );
                }
            }
            PipelinePhase::ImplReview => {
                if score.is_none() {
                    error_exit(
                        "impl_review requires --score N AND --evidence \"Q1:3 Q2:2 ...\" (scores per question).\n\
                         Run the 10 forcing questions, score each 1-3, then:\n\
                         $FLOWCTL phase done --epic ID --phase impl_review --score 25 --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\" --json"
                    );
                }
                if let Some(ev) = evidence {
                    validate_score_evidence(ev, 10, "impl_review");
                } else if score.is_some() {
                    error_exit(
                        "impl_review requires --evidence with per-question scores.\n\
                         Example: --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\""
                    );
                }
            }
            PipelinePhase::Close => {
                if !guard_ran {
                    error_exit(
                        "close phase requires --guard-ran (confirms final guard was executed).\n\
                         Run $FLOWCTL guard first, then:\n\
                         $FLOWCTL phase done --epic ID --phase close --guard-ran --json"
                    );
                }
            }
            _ => {} // brainstorm and plan have no evidence gates
        }
    }

    let next_phase = current.next().expect("non-terminal phase has a next");
    update_phase(&flow_dir, epic_id, &next_phase);

    if json {
        let mut out = json!({
            "previous_phase": current.as_str(),
            "phase": next_phase.as_str(),
            "prompt": next_phase.prompt_template(),
            "all_done": next_phase.is_terminal(),
        });
        if let Some(s) = score {
            out["evidence_score"] = json!(s);
        }
        if guard_ran {
            out["guard_ran"] = json!(true);
        }
        json_output(out);
    } else {
        println!("Advanced: {} → {}", current, next_phase);
        if let Some(s) = score {
            println!("Evidence: self-review score {}/30", s);
        }
        if guard_ran {
            println!("Evidence: guard executed");
        }
        println!("Prompt: {}", next_phase.prompt_template());
        if next_phase.is_terminal() {
            println!("Status: all phases complete");
        }
    }
}
