//! Worker-phase commands: dispatch, next, done.

use std::collections::HashSet;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::id::is_task_id;

use super::{ensure_flow_exists, try_open_db};

/// Worker-phase subcommands.
#[derive(Subcommand, Debug)]
pub enum WorkerPhaseCmd {
    /// Return the next uncompleted phase.
    Next {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Include TDD phases.
        #[arg(long)]
        tdd: bool,
        /// Include review phase.
        #[arg(long, value_parser = ["rp", "codex"])]
        review: Option<String>,
    },
    /// Mark a phase as completed.
    Done {
        /// Task ID.
        #[arg(long)]
        task: String,
        /// Phase ID to mark done.
        #[arg(long)]
        phase: String,
        /// Include TDD phases.
        #[arg(long)]
        tdd: bool,
        /// Include review phase.
        #[arg(long, value_parser = ["rp", "codex"])]
        review: Option<String>,
    },
}

// ── Phase definitions ──────────────────────────────────────────────

/// Phase definition: (id, title, done_condition).
struct PhaseDef {
    id: &'static str,
    title: &'static str,
    done_condition: &'static str,
}

const PHASE_DEFS: &[PhaseDef] = &[
    PhaseDef { id: "0",   title: "Verify Configuration",  done_condition: "OWNED_FILES verified and configuration validated" },
    PhaseDef { id: "1",   title: "Re-anchor",             done_condition: "Run flowctl show <task> and verify spec was read" },
    PhaseDef { id: "2a",  title: "TDD Red-Green",         done_condition: "Failing tests written and confirmed to fail" },
    PhaseDef { id: "2",   title: "Implement",             done_condition: "Feature implemented and code compiles" },
    PhaseDef { id: "2.5", title: "Verify & Fix",          done_condition: "flowctl guard passes and diff reviewed" },
    PhaseDef { id: "3",   title: "Commit",                done_condition: "Changes committed with conventional commit message" },
    PhaseDef { id: "4",   title: "Review",                done_condition: "SHIP verdict received from reviewer" },
    PhaseDef { id: "5",   title: "Complete",              done_condition: "flowctl done called and task status is done" },
    PhaseDef { id: "5c",  title: "Outputs Dump",          done_condition: "Narrative summary written to .flow/outputs/<task-id>.md" },
    PhaseDef { id: "5b",  title: "Memory Auto-Save",      done_condition: "Non-obvious lessons saved to memory (if any)" },
    PhaseDef { id: "6",   title: "Return",                done_condition: "Summary returned to main conversation" },
];

/// Canonical ordering of all phases — used to merge sequences.
/// Phase 5c (outputs dump) runs BEFORE 5 (completion) so the narrative
/// handoff artifact exists before dependents unblock and begin re-anchor.
const CANONICAL_ORDER: &[&str] = &["0", "1", "2a", "2", "2.5", "3", "4", "5c", "5", "5b", "6"];

/// Default phase sequence (Worktree + Teams, always includes Phase 0).
/// Phase 5c is inserted before 5 when `outputs.enabled` is true (default).
const PHASE_SEQ_DEFAULT: &[&str] = &["0", "1", "2", "2.5", "3", "5", "5b", "6"];
const PHASE_SEQ_TDD: &[&str]    = &["0", "1", "2a", "2", "2.5", "3", "5", "5b", "6"];
const PHASE_SEQ_REVIEW: &[&str] = &["0", "1", "2", "2.5", "3", "4", "5", "5b", "6"];

fn get_phase_def(phase_id: &str) -> Option<&'static PhaseDef> {
    PHASE_DEFS.iter().find(|p| p.id == phase_id)
}

/// Read `outputs.enabled` from .flow/config.json. Default: true.
fn is_outputs_enabled() -> bool {
    use flowctl_core::types::{CONFIG_FILE, FLOW_DIR};
    let cfg_path = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join(FLOW_DIR)
        .join(CONFIG_FILE);
    if !cfg_path.exists() {
        return true;
    }
    match std::fs::read_to_string(&cfg_path) {
        Ok(content) => {
            let cfg: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            cfg.get("outputs")
                .and_then(|m| m.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        }
        Err(_) => true,
    }
}

/// Build the phase sequence based on mode flags.
fn build_phase_sequence(tdd: bool, review: bool) -> Vec<&'static str> {
    let mut phases = HashSet::new();
    for p in PHASE_SEQ_DEFAULT {
        phases.insert(*p);
    }
    if tdd {
        for p in PHASE_SEQ_TDD {
            phases.insert(*p);
        }
    }
    if review {
        for p in PHASE_SEQ_REVIEW {
            phases.insert(*p);
        }
    }
    if is_outputs_enabled() {
        phases.insert("5c");
    }
    CANONICAL_ORDER.iter().copied().filter(|p| phases.contains(p)).collect()
}

/// Load completed phases from SQLite.
fn load_completed_phases(task_id: &str) -> Vec<String> {
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::PhaseProgressRepo::new(&conn);
        repo.get_completed(task_id).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Mark a phase as done in SQLite.
fn save_phase_done(task_id: &str, phase: &str) {
    if let Some(conn) = try_open_db() {
        let repo = crate::commands::db_shim::PhaseProgressRepo::new(&conn);
        if let Err(e) = repo.mark_done(task_id, phase) {
            eprintln!("Warning: failed to save phase progress: {}", e);
        }
    }
}

// ── Worker-phase dispatch ─────────────────────────────────────────

pub fn dispatch_worker_phase(cmd: &WorkerPhaseCmd, json_mode: bool) {
    match cmd {
        WorkerPhaseCmd::Next { task, tdd, review } => {
            cmd_worker_phase_next(json_mode, task, *tdd, review.as_deref());
        }
        WorkerPhaseCmd::Done { task, phase, tdd, review } => {
            cmd_worker_phase_done(json_mode, task, phase, *tdd, review.as_deref());
        }
    }
}

fn cmd_worker_phase_next(json_mode: bool, task_id: &str, tdd: bool, review: Option<&str>) {
    let _flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some());
    let completed = load_completed_phases(task_id);
    let completed_set: HashSet<&str> =
        completed.iter().map(|s| s.as_str()).collect();

    // Find first uncompleted phase
    let next_phase = seq.iter().find(|p| !completed_set.contains(**p)).copied();

    match next_phase {
        None => {
            if json_mode {
                json_output(json!({
                    "phase": null,
                    "all_done": true,
                    "sequence": seq,
                }));
            } else {
                println!("All phases completed.");
            }
        }
        Some(phase_id) => {
            let def = get_phase_def(phase_id);
            let title = def.map(|d| d.title).unwrap_or("Unknown");
            let done_condition = def.map(|d| d.done_condition).unwrap_or("");

            let sorted_completed: Vec<&str> = seq.iter()
                .copied()
                .filter(|p| completed_set.contains(*p))
                .collect();

            if json_mode {
                json_output(json!({
                    "phase": phase_id,
                    "title": title,
                    "done_condition": done_condition,
                    "content": "",
                    "completed_phases": sorted_completed,
                    "sequence": seq,
                    "all_done": false,
                }));
            } else {
                println!("Next phase: {} - {}", phase_id, title);
                println!("Done when: {}", done_condition);
                if !sorted_completed.is_empty() {
                    println!("Completed: {}", sorted_completed.join(", "));
                }
            }
        }
    }
}

fn cmd_worker_phase_done(
    json_mode: bool,
    task_id: &str,
    phase: &str,
    tdd: bool,
    review: Option<&str>,
) {
    let _flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some());

    // Validate phase exists in sequence
    if !seq.contains(&phase) {
        error_exit(&format!(
            "Phase '{}' is not in the current sequence: {}. \
             Check your mode flags (--tdd, --review).",
            phase,
            seq.join(", ")
        ));
    }

    let completed = load_completed_phases(task_id);
    let completed_set: HashSet<&str> =
        completed.iter().map(|s| s.as_str()).collect();

    // Find expected next phase (first uncompleted)
    let expected = seq.iter().find(|p| !completed_set.contains(**p)).copied();

    match expected {
        None => {
            error_exit("All phases are already completed. Nothing to mark done.");
        }
        Some(exp) if exp != phase => {
            error_exit(&format!(
                "Expected phase {}, got phase {}. Cannot skip phases.",
                exp, phase
            ));
        }
        _ => {}
    }

    // Mark phase done
    save_phase_done(task_id, phase);

    // Reload to get updated state
    let updated_completed = load_completed_phases(task_id);
    let updated_set: HashSet<&str> =
        updated_completed.iter().map(|s| s.as_str()).collect();
    let next_phase = seq.iter().find(|p| !updated_set.contains(**p)).copied();
    let all_done = next_phase.is_none();

    if json_mode {
        let mut result = json!({
            "completed_phase": phase,
            "completed_phases": updated_completed,
            "all_done": all_done,
        });
        if let Some(np) = next_phase {
            let def = get_phase_def(np);
            result["next_phase"] = json!({
                "phase": np,
                "title": def.map(|d| d.title).unwrap_or("Unknown"),
                "done_condition": def.map(|d| d.done_condition).unwrap_or(""),
            });
        }
        json_output(result);
    } else {
        println!("Phase {} marked done.", phase);
        if let Some(np) = next_phase {
            let def = get_phase_def(np);
            let title = def.map(|d| d.title).unwrap_or("Unknown");
            println!("Next: {} - {}", np, title);
        } else {
            println!("All phases completed.");
        }
    }
}
