//! Worker-phase commands: dispatch, next, done.

use std::collections::HashSet;
use std::fs;

use clap::Subcommand;
use serde_json::{Value, json};

use crate::output::{error_exit, json_output};

use flowctl_core::config::read_config_bool;
use flowctl_core::id::is_task_id;
use flowctl_core::state_machine::Status;
use flowctl_core::types::TaskSize;

use super::ensure_flow_exists;

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
        /// Task size: S (small/fast), M (medium/default), L (large/thorough).
        #[arg(long, value_parser = ["S", "M", "L"])]
        size: Option<String>,
        /// Return ALL remaining phases at once (batch mode — reduces CLI round-trips).
        #[arg(long)]
        all: bool,
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
        /// Task size: S (small/fast), M (medium/default), L (large/thorough).
        #[arg(long, value_parser = ["S", "M", "L"])]
        size: Option<String>,
        /// Inline JSON receipt for artifact-backed worker gates.
        #[arg(long)]
        receipt: Option<String>,
        /// Path to a JSON receipt file for artifact-backed worker gates.
        #[arg(long)]
        receipt_file: Option<String>,
    },
}

// ── Phase definitions ──────────────────────────────────────────────

/// Phase definition: (id, title, done_condition, instructions).
struct PhaseDef {
    id: &'static str,
    title: &'static str,
    done_condition: &'static str,
    instructions: &'static str,
}

const PHASE_DEFS: &[PhaseDef] = &[
    PhaseDef {
        id: "1",
        title: "Verify Configuration",
        done_condition: "OWNED_FILES verified and configuration validated",
        instructions: "Validate OWNED_FILES list and confirm task configuration matches the epic spec.",
    },
    PhaseDef {
        id: "2",
        title: "Re-anchor",
        done_condition: "Run flowctl show <task> and verify spec was read",
        instructions: "Read the task spec via flowctl show and re-anchor on acceptance criteria before coding.",
    },
    PhaseDef {
        id: "3",
        title: "Investigation",
        done_condition: "Target files read and related patterns identified",
        instructions: "Read investigation targets from the task spec. If targets exist, follow them. If not, do a lightweight scan: read files listed in **Files:** and grep for 2-3 related patterns. If RP_CONTEXT is available, use context_builder for deep context. Skip only for trivial one-line config changes.",
    },
    PhaseDef {
        id: "4",
        title: "TDD Red-Green",
        done_condition: "Failing tests written and confirmed to fail",
        instructions: "Write failing tests that encode the acceptance criteria, then confirm they fail.",
    },
    PhaseDef {
        id: "5",
        title: "Implement",
        done_condition: "Feature implemented and code compiles",
        instructions: "Implement the feature to satisfy the spec and ensure the code compiles cleanly.",
    },
    PhaseDef {
        id: "6",
        title: "Verify & Fix",
        done_condition: "flowctl guard passes and diff reviewed",
        instructions: "Run flowctl guard (lint, type-check, tests) and review the diff for correctness.",
    },
    PhaseDef {
        id: "7",
        title: "Commit",
        done_condition: "Changes committed with conventional commit message",
        instructions: "Commit all changes with a conventional commit message referencing the task ID.",
    },
    PhaseDef {
        id: "8",
        title: "Review",
        done_condition: "SHIP verdict received from reviewer",
        instructions: "Submit the diff for review and iterate until a SHIP verdict is received.",
    },
    PhaseDef {
        id: "9",
        title: "Outputs Dump",
        done_condition: "Narrative summary written to .flow/outputs/<task-id>.md",
        instructions: "Write a narrative summary of what was built and why to .flow/outputs/<task-id>.md.",
    },
    PhaseDef {
        id: "10",
        title: "Complete",
        done_condition: "flowctl done called and task status is done",
        instructions: "Call flowctl done with summary and evidence to mark the task complete.",
    },
    PhaseDef {
        id: "11",
        title: "Memory Auto-Save",
        done_condition: "Non-obvious lessons saved to memory (if any)",
        instructions: "Save any non-obvious lessons or patterns discovered during implementation to memory.",
    },
    PhaseDef {
        id: "12",
        title: "Return",
        done_condition: "Summary returned to main conversation",
        instructions: "Return a concise summary of completed work to the main conversation.",
    },
];

/// Canonical ordering of all phases — used to merge sequences.
/// Phase 9 (outputs dump) runs BEFORE 10 (completion) so the narrative
/// handoff artifact exists before dependents unblock and begin re-anchor.
const CANONICAL_ORDER: &[&str] = &[
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
];

/// Default phase sequence (Worktree + Teams, always includes Phase 1).
/// Phase 3 (investigation) now included by default — even S/M tasks need context.
/// Phase 9 is inserted before 10 when `outputs.enabled` is true (default).
const PHASE_SEQ_DEFAULT: &[&str] = &["1", "2", "3", "5", "6", "7", "10", "11", "12"];
const PHASE_SEQ_TDD: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "10", "11", "12"];
const PHASE_SEQ_REVIEW: &[&str] = &["1", "2", "3", "5", "6", "7", "8", "10", "11", "12"];

/// Size-based phase sequences.
/// S: fast path — includes lightweight investigation (phase 3), skips outputs/memory.
const PHASE_SEQ_SMALL: &[&str] = &["1", "2", "3", "5", "6", "7", "10", "12"];
/// L: thorough path — all 11 defined phases (adds TDD, review, outputs, memory).
const PHASE_SEQ_LARGE: &[&str] = &[
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
];

fn get_phase_def(phase_id: &str) -> Option<&'static PhaseDef> {
    PHASE_DEFS.iter().find(|p| p.id == phase_id)
}

/// Build the phase sequence based on mode flags and task size.
///
/// Size controls the base sequence:
///   - S (small): fast path — 8 phases, includes investigation, skips outputs/memory
///   - M (medium, default): standard 9-phase sequence with investigation
///   - L (large): thorough — all 12 defined phases
///
/// Additional flags (--tdd, --review) merge extra phases into the base.
/// Config overrides (outputs.enabled, memory.enabled) apply on top.
fn build_phase_sequence(tdd: bool, review: bool, size: TaskSize) -> Vec<&'static str> {
    let mut phases = HashSet::new();

    // Select base sequence from size
    let base = match size {
        TaskSize::Small => PHASE_SEQ_SMALL,
        TaskSize::Medium => PHASE_SEQ_DEFAULT,
        TaskSize::Large => PHASE_SEQ_LARGE,
    };

    for p in base {
        // Phase 11 (memory) is always conditional on config, not base sequence
        if *p == "11" {
            continue;
        }
        phases.insert(*p);
    }

    // Merge TDD phases
    if tdd {
        for p in PHASE_SEQ_TDD {
            if *p == "11" {
                continue;
            }
            phases.insert(*p);
        }
    }

    // Merge review phase
    if review {
        for p in PHASE_SEQ_REVIEW {
            if *p == "11" {
                continue;
            }
            phases.insert(*p);
        }
    }

    // Config-driven phases
    if read_config_bool("outputs.enabled", true) {
        phases.insert("9");
    }
    if read_config_bool("memory.enabled", true) {
        phases.insert("11");
    }

    CANONICAL_ORDER
        .iter()
        .copied()
        .filter(|p| phases.contains(p))
        .collect()
}

/// Map unambiguously legacy phase IDs to sequential integers.
/// Only IDs that cannot be confused with new sequential IDs are migrated.
/// Pure integers 1-12 are left as-is since they may already be new IDs.
fn migrate_phase_id(id: &str) -> String {
    match id {
        "0" => "1".to_string(),
        "1.5" => "3".to_string(),
        "2a" => "4".to_string(),
        "2.5" => "6".to_string(),
        "5c" => "9".to_string(),
        "5b" => "11".to_string(),
        other => other.to_string(),
    }
}

/// Load completed phases from file store, migrating legacy IDs.
fn load_completed_phases(flow_dir: &std::path::Path, task_id: &str) -> Vec<String> {
    flowctl_core::json_store::phases_completed(flow_dir, task_id)
        .unwrap_or_default()
        .into_iter()
        .map(|id| migrate_phase_id(&id))
        .collect()
}

/// Mark a phase as done in file store.
fn save_phase_done(flow_dir: &std::path::Path, task_id: &str, phase: &str) {
    if let Err(e) = flowctl_core::json_store::phase_mark_done(flow_dir, task_id, phase) {
        eprintln!("Warning: failed to save phase progress: {}", e);
    }
}

fn read_json_receipt(
    receipt: Option<&str>,
    receipt_file: Option<&str>,
    phase: &str,
) -> Option<Value> {
    let raw = if let Some(path) = receipt_file {
        Some(fs::read_to_string(path).unwrap_or_else(|e| {
            error_exit(&format!(
                "phase {phase} requires a readable --receipt-file. Failed to read {}: {}",
                path, e
            ))
        }))
    } else {
        receipt.map(ToOwned::to_owned)
    }?;

    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
        error_exit(&format!(
            "phase {phase} receipt must be valid JSON object data: {}",
            e
        ))
    });

    if !parsed.is_object() {
        error_exit(&format!("phase {phase} receipt must be a JSON object"));
    }

    Some(parsed)
}

fn require_receipt(receipt: Option<Value>, phase: &str) -> Value {
    receipt.unwrap_or_else(|| {
        error_exit(&format!(
            "phase {phase} requires --receipt '{{...}}' or --receipt-file <path>. \
             Worker phases are artifact-backed and cannot advance on prompt text alone."
        ))
    })
}

fn receipt_bool(receipt: &Value, key: &str, phase: &str) -> bool {
    receipt
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            error_exit(&format!(
                "phase {phase} receipt missing boolean key '{key}'"
            ))
        })
}

fn receipt_string(receipt: &Value, key: &str, phase: &str) -> String {
    receipt
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| error_exit(&format!("phase {phase} receipt missing string key '{key}'")))
}

fn receipt_string_list(receipt: &Value, key: &str, phase: &str) -> Vec<String> {
    let arr = receipt
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| error_exit(&format!("phase {phase} receipt missing array key '{key}'")));
    let values: Vec<String> = arr
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if values.is_empty() {
        error_exit(&format!(
            "phase {phase} receipt key '{key}' must be a non-empty string array"
        ));
    }
    values
}

fn read_existing_file(path: &str, label: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| error_exit(&format!("{label} '{}' is unreadable: {}", path, e)))
}

fn require_non_empty_file(path: &str, label: &str) {
    let content = read_existing_file(path, label);
    if content.trim().is_empty() {
        error_exit(&format!("{label} '{}' is empty", path));
    }
}

fn require_task_spec(flow_dir: &std::path::Path, task_id: &str) -> String {
    let spec = flowctl_core::json_store::task_spec_read(flow_dir, task_id).unwrap_or_else(|e| {
        error_exit(&format!(
            "phase gate failed: unable to read task spec for {}: {}",
            task_id, e
        ))
    });
    if spec.trim().is_empty() {
        error_exit(&format!(
            "phase gate failed: task {} has no task spec. \
             Write the task spec before advancing worker phases.",
            task_id
        ));
    }
    spec
}

fn validate_phase_receipt(
    flow_dir: &std::path::Path,
    task_id: &str,
    phase: &str,
    receipt: Option<Value>,
) -> Value {
    match phase {
        "1" => {
            let receipt = require_receipt(receipt, phase);
            let owned_files = receipt_string_list(&receipt, "owned_files", phase);
            if !receipt_bool(&receipt, "config_valid", phase) {
                error_exit("phase 1 gate failed: config_valid must be true");
            }
            let task = flowctl_core::json_store::task_read(flow_dir, task_id).unwrap_or_else(|e| {
                error_exit(&format!(
                    "phase 1 gate failed: unable to read task {}: {}",
                    task_id, e
                ))
            });
            if !task.files.is_empty() {
                let owned_set: HashSet<&str> = owned_files.iter().map(String::as_str).collect();
                let missing: Vec<String> = task
                    .files
                    .iter()
                    .filter(|path| !owned_set.contains(path.as_str()))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    error_exit(&format!(
                        "phase 1 gate failed: receipt owned_files is missing declared task files: {}",
                        missing.join(", ")
                    ));
                }
            }
            receipt
        }
        "2" => {
            let receipt = require_receipt(receipt, phase);
            let task_spec = require_task_spec(flow_dir, task_id);
            if !task_spec.contains("## Acceptance") {
                error_exit(&format!(
                    "phase 2 gate failed: task spec for {} is missing ## Acceptance",
                    task_id
                ));
            }
            let acceptance_points = receipt_string_list(&receipt, "acceptance_points", phase);
            if acceptance_points.len() < 1 {
                error_exit("phase 2 gate failed: acceptance_points must not be empty");
            }
            receipt
        }
        "3" => {
            let receipt = require_receipt(receipt, phase);
            require_task_spec(flow_dir, task_id);
            receipt_string_list(&receipt, "files_read", phase);
            receipt
        }
        "4" => {
            let receipt = require_receipt(receipt, phase);
            receipt_string_list(&receipt, "failing_tests", phase);
            if !receipt_bool(&receipt, "confirmed_failing", phase) {
                error_exit("phase 4 gate failed: confirmed_failing must be true");
            }
            receipt
        }
        "5" => {
            let receipt = require_receipt(receipt, phase);
            receipt_string_list(&receipt, "files_changed", phase);
            if !receipt_bool(&receipt, "implemented", phase) {
                error_exit("phase 5 gate failed: implemented must be true");
            }
            receipt
        }
        "6" => {
            let receipt = require_receipt(receipt, phase);
            if !receipt_bool(&receipt, "guard_passed", phase) {
                error_exit("phase 6 gate failed: guard_passed must be true");
            }
            if !receipt_bool(&receipt, "diff_reviewed", phase) {
                error_exit("phase 6 gate failed: diff_reviewed must be true");
            }
            receipt
        }
        "7" => {
            let receipt = require_receipt(receipt, phase);
            let commit = receipt_string(&receipt, "commit", phase);
            if commit.len() < 7 {
                error_exit("phase 7 gate failed: commit must look like a real commit hash");
            }
            receipt
        }
        "8" => {
            let receipt = require_receipt(receipt, phase);
            let verdict = receipt_string(&receipt, "verdict", phase);
            if verdict.to_uppercase() != "SHIP" {
                error_exit("phase 8 gate failed: verdict must be SHIP");
            }
            receipt
        }
        "9" => {
            let receipt = require_receipt(receipt, phase);
            let output_path = receipt_string(&receipt, "output_path", phase);
            require_non_empty_file(&output_path, "phase 9 output");
            receipt
        }
        "10" => {
            let task = flowctl_core::json_store::task_read(flow_dir, task_id).unwrap_or_else(|e| {
                error_exit(&format!(
                    "phase 10 gate failed: unable to read task {}: {}",
                    task_id, e
                ))
            });
            if task.status != Status::Done {
                error_exit(&format!(
                    "phase 10 gate failed: task {} is not done. \
                     Run `flowctl done {}` with summary and evidence before completing phase 10.",
                    task_id, task_id
                ));
            }
            let state =
                flowctl_core::json_store::state_read(flow_dir, task_id).unwrap_or_else(|e| {
                    error_exit(&format!(
                        "phase 10 gate failed: unable to read runtime state for {}: {}",
                        task_id, e
                    ))
                });
            if state.evidence.is_none() {
                error_exit(&format!(
                    "phase 10 gate failed: task {} is done but has no evidence recorded",
                    task_id
                ));
            }
            json!({
                "task_done": true,
                "evidence_present": true,
            })
        }
        "11" => {
            let receipt = require_receipt(receipt, phase);
            if !receipt_bool(&receipt, "checked", phase) {
                error_exit("phase 11 gate failed: checked must be true");
            }
            receipt
                .get("saved")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| error_exit("phase 11 receipt missing boolean key 'saved'"));
            receipt
        }
        "12" => {
            let receipt = require_receipt(receipt, phase);
            let summary = receipt_string(&receipt, "summary", phase);
            if summary.len() < 10 {
                error_exit("phase 12 gate failed: summary is too short");
            }
            receipt
        }
        _ => error_exit(&format!("Unknown worker phase '{}'", phase)),
    }
}

// ── Worker-phase dispatch ─────────────────────────────────────────

fn parse_size(size: Option<&str>) -> TaskSize {
    match size {
        Some(s) => s.parse::<TaskSize>().unwrap_or(TaskSize::Medium),
        None => TaskSize::Medium,
    }
}

pub fn dispatch_worker_phase(cmd: &WorkerPhaseCmd, json_mode: bool) {
    match cmd {
        WorkerPhaseCmd::Next {
            task,
            tdd,
            review,
            size,
            all,
        } => {
            if *all {
                cmd_worker_phase_next_all(
                    json_mode,
                    task,
                    *tdd,
                    review.as_deref(),
                    parse_size(size.as_deref()),
                );
            } else {
                cmd_worker_phase_next(
                    json_mode,
                    task,
                    *tdd,
                    review.as_deref(),
                    parse_size(size.as_deref()),
                );
            }
        }
        WorkerPhaseCmd::Done {
            task,
            phase,
            tdd,
            review,
            size,
            receipt,
            receipt_file,
        } => {
            cmd_worker_phase_done(
                json_mode,
                task,
                phase,
                *tdd,
                review.as_deref(),
                parse_size(size.as_deref()),
                receipt.as_deref(),
                receipt_file.as_deref(),
            );
        }
    }
}

fn cmd_worker_phase_next(
    json_mode: bool,
    task_id: &str,
    tdd: bool,
    review: Option<&str>,
    size: TaskSize,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some(), size);
    let completed = load_completed_phases(&flow_dir, task_id);
    let completed_set: HashSet<&str> = completed.iter().map(std::string::String::as_str).collect();

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
            let instructions = def.map(|d| d.instructions).unwrap_or("");

            let sorted_completed: Vec<&str> = seq
                .iter()
                .copied()
                .filter(|p| completed_set.contains(*p))
                .collect();

            if json_mode {
                json_output(json!({
                    "phase": phase_id,
                    "title": title,
                    "done_condition": done_condition,
                    "instructions": instructions,
                    "content": "",
                    "completed_phases": sorted_completed,
                    "sequence": seq,
                    "all_done": false,
                }));
            } else {
                println!("Next phase: {} - {}", phase_id, title);
                println!("Done when: {}", done_condition);
                println!("Instructions: {}", instructions);
                if !sorted_completed.is_empty() {
                    println!("Completed: {}", sorted_completed.join(", "));
                }
            }
        }
    }
}

/// Return ALL remaining phases at once (batch mode — reduces 24 CLI calls to 1).
fn cmd_worker_phase_next_all(
    json_mode: bool,
    task_id: &str,
    tdd: bool,
    review: Option<&str>,
    size: TaskSize,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some(), size);
    let completed = load_completed_phases(&flow_dir, task_id);
    let completed_set: HashSet<&str> = completed.iter().map(std::string::String::as_str).collect();

    let remaining: Vec<serde_json::Value> = seq
        .iter()
        .filter(|p| !completed_set.contains(**p))
        .map(|phase_id| {
            let def = get_phase_def(phase_id);
            json!({
                "phase": phase_id,
                "title": def.map(|d| d.title).unwrap_or("Unknown"),
                "done_condition": def.map(|d| d.done_condition).unwrap_or(""),
                "instructions": def.map(|d| d.instructions).unwrap_or(""),
            })
        })
        .collect();

    if json_mode {
        json_output(json!({
            "phases": remaining,
            "total_remaining": remaining.len(),
            "completed_phases": completed,
            "sequence": seq,
            "all_done": remaining.is_empty(),
        }));
    } else {
        if remaining.is_empty() {
            println!("All phases completed.");
        } else {
            println!("{} phases remaining:", remaining.len());
            for p in &remaining {
                println!("  {} - {}", p["phase"], p["title"]);
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
    size: TaskSize,
    receipt: Option<&str>,
    receipt_file: Option<&str>,
) {
    let flow_dir = ensure_flow_exists();

    if !is_task_id(task_id) {
        error_exit(&format!(
            "Invalid task ID: {}. Expected format: fn-N.M or fn-N-slug.M",
            task_id
        ));
    }

    let seq = build_phase_sequence(tdd, review.is_some(), size);

    // Validate phase exists in sequence
    if !seq.contains(&phase) {
        error_exit(&format!(
            "Phase '{}' is not in the current sequence: {}. \
             Check your mode flags (--tdd, --review).",
            phase,
            seq.join(", ")
        ));
    }

    let completed = load_completed_phases(&flow_dir, task_id);
    let completed_set: HashSet<&str> = completed.iter().map(std::string::String::as_str).collect();

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

    let validated_receipt = validate_phase_receipt(
        &flow_dir,
        task_id,
        phase,
        read_json_receipt(receipt, receipt_file, phase),
    );
    if let Err(e) = flowctl_core::json_store::worker_phase_receipt_write(
        &flow_dir,
        task_id,
        phase,
        &validated_receipt,
    ) {
        error_exit(&format!(
            "Failed to persist worker phase receipt for {} phase {}: {}",
            task_id, phase, e
        ));
    }

    // Mark phase done
    save_phase_done(&flow_dir, task_id, phase);

    // Reload to get updated state
    let updated_completed = load_completed_phases(&flow_dir, task_id);
    let updated_set: HashSet<&str> = updated_completed
        .iter()
        .map(std::string::String::as_str)
        .collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_sequence_size_s() {
        let seq = build_phase_sequence(false, false, TaskSize::Small);
        // S base includes investigation (phase 3) since all tasks need context
        for p in &["1", "2", "3", "5", "6", "7", "10", "12"] {
            assert!(seq.contains(p), "S sequence missing phase {p}");
        }
        // S should NOT include TDD or review without flags
        assert!(!seq.contains(&"4"), "S should not include TDD without flag");
        assert!(
            !seq.contains(&"8"),
            "S should not include review without flag"
        );
    }

    #[test]
    fn test_build_sequence_size_l() {
        let seq = build_phase_sequence(false, false, TaskSize::Large);
        // L base includes TDD, review, outputs, investigation — all non-conditional phases
        for p in &["1", "2", "3", "4", "5", "6", "7", "8", "10", "12"] {
            assert!(seq.contains(p), "L sequence missing phase {p}");
        }
        // L is strictly a superset of S
        let seq_s = build_phase_sequence(false, false, TaskSize::Small);
        for p in &seq_s {
            assert!(seq.contains(p), "L should be superset of S, missing {p}");
        }
    }

    #[test]
    fn test_size_s_with_tdd() {
        let seq = build_phase_sequence(true, false, TaskSize::Small);
        assert!(seq.contains(&"4"), "S+TDD should include phase 4");
        assert!(seq.contains(&"3"), "S+TDD should include investigation");
        assert!(seq.contains(&"5"));
        assert!(seq.contains(&"6"));
    }

    #[test]
    fn test_backward_compat_no_size() {
        let seq = build_phase_sequence(false, false, TaskSize::Medium);
        // Medium always includes investigation + core phases
        for p in &["1", "2", "3", "5", "6", "7", "10", "12"] {
            assert!(seq.contains(p), "M sequence missing phase {p}");
        }
        // No TDD, no review
        assert!(!seq.contains(&"4"), "M should not include TDD without flag");
        assert!(
            !seq.contains(&"8"),
            "M should not include review without flag"
        );
    }

    #[test]
    fn test_phase_def_instructions_not_empty() {
        for def in PHASE_DEFS {
            assert!(
                !def.instructions.is_empty(),
                "Phase {} has empty instructions",
                def.id
            );
        }
    }

    #[test]
    fn test_size_ordering() {
        let s = build_phase_sequence(false, false, TaskSize::Small);
        let m = build_phase_sequence(false, false, TaskSize::Medium);
        let l = build_phase_sequence(false, false, TaskSize::Large);
        // S <= M <= L in phase count
        assert!(
            s.len() <= m.len(),
            "S ({}) should have <= phases than M ({})",
            s.len(),
            m.len()
        );
        assert!(
            m.len() <= l.len(),
            "M ({}) should have <= phases than L ({})",
            m.len(),
            l.len()
        );
    }
}
