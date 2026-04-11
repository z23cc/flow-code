//! FROZEN — ADR-011: Bug fixes only. No new features.
//! Superseded by V3 Goal-driven engine (see docs/v3-final-architecture.md).
//!
//! Pipeline phase commands: `flowctl phase next` and `flowctl phase done`.
//!
//! These commands manage the epic-level pipeline progression stored in
//! `.state/pipeline.json`. Distinct from worker-phase (task-level phases).

use std::collections::BTreeSet;
use std::fs;

use clap::Subcommand;
use serde_json::{Value, json};

use flowctl_core::json_store;
use flowctl_core::pipeline::PipelinePhase;
use flowctl_core::types::EpicStatus;

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
    /// - brainstorm: --receipt-file with requirements artifact metadata
    /// - plan: --receipt-file with spec/task artifact metadata
    /// - plan_review: --score N (self-review score out of 30)
    /// - work: --receipt-file with integration guard receipt
    /// - impl_review: --score N (self-review score out of 30)
    /// - close: --receipt-file with final validation receipt
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
        /// Inline JSON receipt for artifact-backed gates.
        #[arg(long)]
        receipt: Option<String>,
        /// Path to a JSON receipt file for artifact-backed gates.
        #[arg(long)]
        receipt_file: Option<String>,
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
        PipelinePhaseCmd::Done {
            epic,
            phase,
            score,
            evidence,
            receipt,
            receipt_file,
            guard_ran,
            no_gate,
        } => cmd_phase_done(
            json,
            epic,
            phase,
            *score,
            evidence.as_deref(),
            receipt.as_deref(),
            receipt_file.as_deref(),
            *guard_ran,
            *no_gate,
        ),
    }
}

/// Read current pipeline phase from file. If no entry exists, initialize to Brainstorm.
fn get_or_init_phase(flow_dir: &std::path::Path, epic_id: &str) -> PipelinePhase {
    match json_store::pipeline_read(flow_dir, epic_id) {
        Ok(Some(phase_str)) => {
            PipelinePhase::parse(&phase_str).unwrap_or(PipelinePhase::Brainstorm)
        }
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

    eprintln!(
        "Evidence: {q_count} questions referenced, {} chars",
        evidence.len()
    );
}

/// Update pipeline phase in file store.
fn update_phase(flow_dir: &std::path::Path, epic_id: &str, new_phase: &PipelinePhase) {
    if let Err(e) = json_store::pipeline_write(flow_dir, epic_id, new_phase.as_str()) {
        error_exit(&format!("Failed to update pipeline phase: {e}"));
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
                "{phase} requires a readable --receipt-file. Failed to read {}: {}",
                path, e
            ))
        }))
    } else {
        receipt.map(ToOwned::to_owned)
    }?;

    let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|e| {
        error_exit(&format!(
            "{phase} receipt must be valid JSON object data: {}",
            e
        ))
    });

    if !parsed.is_object() {
        error_exit(&format!("{phase} receipt must be a JSON object"));
    }

    Some(parsed)
}

fn require_receipt(receipt: Option<Value>, phase: &str) -> Value {
    receipt.unwrap_or_else(|| {
        error_exit(&format!(
            "{phase} requires --receipt '{{...}}' or --receipt-file <path>. \
             This gate is artifact-backed and cannot be advanced by prompt text alone."
        ))
    })
}

fn receipt_bool(receipt: &Value, key: &str, phase: &str) -> bool {
    receipt
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or_else(|| error_exit(&format!("{phase} receipt missing boolean key '{key}'")))
}

fn receipt_string(receipt: &Value, key: &str, phase: &str) -> String {
    receipt
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| error_exit(&format!("{phase} receipt missing string key '{key}'")))
}

fn receipt_string_list(receipt: &Value, key: &str, phase: &str) -> Vec<String> {
    let arr = receipt
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| error_exit(&format!("{phase} receipt missing array key '{key}'")));
    let values: Vec<String> = arr
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .collect();
    if values.is_empty() {
        error_exit(&format!(
            "{phase} receipt key '{key}' must be a non-empty string array"
        ));
    }
    values
}

fn read_existing_file(path: &str, label: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| error_exit(&format!("{label} file '{}' is unreadable: {}", path, e)))
}

fn require_markdown_sections(content: &str, label: &str, sections: &[&str]) {
    let missing: Vec<&str> = sections
        .iter()
        .copied()
        .filter(|section| !content.contains(section))
        .collect();
    if !missing.is_empty() {
        error_exit(&format!(
            "{label} is missing required sections: {}",
            missing.join(", ")
        ));
    }
    let empty: Vec<&str> = sections
        .iter()
        .copied()
        .filter(|section| {
            let start = content
                .find(section)
                .map(|idx| idx + section.len())
                .unwrap_or(0);
            let rest = &content[start..];
            let next_heading = rest.find("\n## ").unwrap_or(rest.len());
            rest[..next_heading].trim().is_empty()
        })
        .collect();
    if !empty.is_empty() {
        error_exit(&format!(
            "{label} has empty required sections: {}",
            empty.join(", ")
        ));
    }
}

fn task_spec_ready(spec: &str) -> bool {
    spec.contains("## Description")
        && spec.contains("## Acceptance")
        && spec.contains("**Files:**")
        && spec.contains("- [")
}

fn validate_review_receipt(receipt: Value, phase: &str, score: u32) -> Value {
    let backend = receipt_string(&receipt, "backend", phase);
    let verdict = receipt_string(&receipt, "verdict", phase);
    let findings_path = receipt_string(&receipt, "findings_path", phase);
    let findings = read_existing_file(&findings_path, &format!("{phase} findings"));
    if findings.trim().is_empty() {
        error_exit(&format!(
            "{phase} findings file '{}' is empty",
            findings_path
        ));
    }
    if !matches!(verdict.as_str(), "SHIP" | "NEEDS_WORK" | "MAJOR_RETHINK") {
        error_exit(&format!(
            "{phase} receipt verdict must be one of SHIP, NEEDS_WORK, MAJOR_RETHINK"
        ));
    }
    if backend != "rp" && backend != "codex" && backend != "none" && backend != "export" {
        error_exit(&format!(
            "{phase} receipt backend must be one of rp, codex, export, none"
        ));
    }
    if let Some(receipt_score) = receipt.get("score").and_then(Value::as_u64) {
        if receipt_score != score as u64 {
            error_exit(&format!(
                "{phase} receipt score ({receipt_score}) does not match --score ({score})"
            ));
        }
    }
    receipt
}

/// `flowctl phase next --epic <id> --json`
fn cmd_phase_next(json: bool, epic_id: &str) {
    let flow_dir = ensure_flow_exists();
    let current = get_or_init_phase(&flow_dir, epic_id);
    let all_done = current.is_terminal();

    if json {
        if all_done {
            json_output(json!({
                "phase": null,
                "prompt": current.prompt_template(),
                "all_done": true,
            }));
        } else {
            json_output(json!({
                "phase": current.as_str(),
                "prompt": current.prompt_template(),
                "all_done": false,
            }));
        }
    } else {
        if all_done {
            println!("Status: all phases complete");
        } else {
            println!("Phase: {}", current);
            println!("Prompt: {}", current.prompt_template());
        }
    }
}

/// `flowctl phase done --epic <id> --phase <name> [...] --json`
fn cmd_phase_done(
    json: bool,
    epic_id: &str,
    phase_name: &str,
    score: Option<u32>,
    evidence: Option<&str>,
    receipt: Option<&str>,
    receipt_file: Option<&str>,
    guard_ran: bool,
    no_gate: bool,
) {
    let flow_dir = ensure_flow_exists();

    let requested = match PipelinePhase::parse(phase_name) {
        Some(p) => p,
        None => {
            let valid: Vec<&str> = PipelinePhase::all()
                .iter()
                .map(PipelinePhase::as_str)
                .collect();
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
        error_exit("Pipeline is already complete. No further advancement.");
    }

    let receipt_value = read_json_receipt(receipt, receipt_file, requested.as_str());

    // ── Evidence gates (skip with --no-gate for quick path) ────────
    if !no_gate {
        match current {
            PipelinePhase::Brainstorm => {
                let receipt = require_receipt(receipt_value.clone(), "brainstorm");
                let requirements_path = receipt_string(&receipt, "requirements_path", "brainstorm");
                let requirements =
                    read_existing_file(&requirements_path, "brainstorm requirements");
                require_markdown_sections(
                    &requirements,
                    "brainstorm requirements",
                    &[
                        "## Problem",
                        "## Users",
                        "## Chosen Approach",
                        "## Requirements",
                        "## Self-Interview Trace",
                        "## Approach Comparison",
                    ],
                );
                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save brainstorm receipt: {e}"));
                }
            }
            PipelinePhase::Plan => {
                let receipt = require_receipt(receipt_value.clone(), "plan");
                let spec_path = receipt
                    .get("spec_path")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        flow_dir
                            .join("specs")
                            .join(format!("{epic_id}.md"))
                            .to_string_lossy()
                            .to_string()
                    });
                let spec = read_existing_file(&spec_path, "plan epic spec");
                if spec.trim().is_empty() {
                    error_exit("plan epic spec is empty");
                }

                let receipt_task_ids: BTreeSet<String> =
                    receipt_string_list(&receipt, "task_ids", "plan")
                        .into_iter()
                        .collect();
                let tasks = json_store::task_list_by_epic(&flow_dir, epic_id)
                    .unwrap_or_else(|e| error_exit(&format!("Failed to read epic tasks: {e}")));
                if tasks.is_empty() {
                    error_exit("plan gate failed: no tasks were created for the epic");
                }

                let actual_task_ids: BTreeSet<String> =
                    tasks.iter().map(|task| task.id.clone()).collect();
                if receipt_task_ids != actual_task_ids {
                    error_exit(&format!(
                        "plan receipt task_ids do not match current epic tasks. receipt={} actual={}",
                        receipt_task_ids.into_iter().collect::<Vec<_>>().join(", "),
                        actual_task_ids.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                }

                for task_id in tasks.iter().map(|task| task.id.clone()) {
                    let task_spec =
                        json_store::task_spec_read(&flow_dir, &task_id).unwrap_or_else(|e| {
                            error_exit(&format!("Failed to read task spec for {task_id}: {e}"))
                        });
                    if !task_spec_ready(&task_spec) {
                        error_exit(&format!(
                            "plan gate failed: task spec {} is not implementation-ready \
                             (requires ## Description, ## Acceptance with at least one checkbox item, and **Files:**)",
                            task_id
                        ));
                    }
                }

                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save plan receipt: {e}"));
                }
            }
            PipelinePhase::PlanReview => {
                let Some(score) = score else {
                    error_exit(
                        "plan_review requires --score N AND --evidence \"Q1:3 Q2:2 ...\" (scores per question).\n\
                         Run the 10 forcing questions, score each 1-3, then:\n\
                         $FLOWCTL phase done --epic ID --phase plan_review --score 25 --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\" --receipt-file plan-review-receipt.json --json",
                    );
                };
                if let Some(ev) = evidence {
                    validate_score_evidence(ev, 10, "plan_review");
                } else {
                    error_exit(
                        "plan_review requires --evidence with per-question scores.\n\
                         Example: --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\"",
                    );
                }
                let receipt = validate_review_receipt(
                    require_receipt(receipt_value.clone(), "plan_review"),
                    "plan_review",
                    score,
                );
                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save plan_review receipt: {e}"));
                }
            }
            PipelinePhase::Work => {
                let receipt = require_receipt(receipt_value.clone(), "work");
                if !guard_ran || !receipt_bool(&receipt, "guard_passed", "work") {
                    error_exit(
                        "work phase requires a real guard receipt. \
                         Pass --guard-ran and a receipt with {\"guard_passed\":true}.",
                    );
                }
                if !receipt_bool(&receipt, "invariants_passed", "work") {
                    error_exit("work phase cannot advance: invariants_passed must be true");
                }
                let tasks = json_store::task_list_by_epic(&flow_dir, epic_id)
                    .unwrap_or_else(|e| error_exit(&format!("Failed to read epic tasks: {e}")));
                if tasks.is_empty() {
                    error_exit("work phase cannot advance: epic has no tasks");
                }
                let unfinished: Vec<String> = tasks
                    .iter()
                    .filter(|task| !task.status.is_satisfied())
                    .map(|task| format!("{} ({})", task.id, task.status))
                    .collect();
                if !unfinished.is_empty() {
                    error_exit(&format!(
                        "work phase cannot advance while tasks remain unfinished: {}",
                        unfinished.join(", ")
                    ));
                }
                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save work receipt: {e}"));
                }
            }
            PipelinePhase::ImplReview => {
                let Some(score) = score else {
                    error_exit(
                        "impl_review requires --score N AND --evidence \"Q1:3 Q2:2 ...\" (scores per question).\n\
                         Run the 10 forcing questions, score each 1-3, then:\n\
                         $FLOWCTL phase done --epic ID --phase impl_review --score 25 --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\" --receipt-file impl-review-receipt.json --json",
                    );
                };
                if let Some(ev) = evidence {
                    validate_score_evidence(ev, 10, "impl_review");
                } else {
                    error_exit(
                        "impl_review requires --evidence with per-question scores.\n\
                         Example: --evidence \"Q1:3 Q2:2 Q3:3 Q4:2 Q5:3 Q6:2 Q7:3 Q8:2 Q9:3 Q10:2\"",
                    );
                }
                let receipt = validate_review_receipt(
                    require_receipt(receipt_value.clone(), "impl_review"),
                    "impl_review",
                    score,
                );
                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save impl_review receipt: {e}"));
                }
            }
            PipelinePhase::Close => {
                let receipt = require_receipt(receipt_value.clone(), "close");
                if !guard_ran || !receipt_bool(&receipt, "guard_passed", "close") {
                    error_exit(
                        "close phase requires a real final guard receipt. \
                         Pass --guard-ran and a receipt with {\"guard_passed\":true}.",
                    );
                }
                if !receipt_bool(&receipt, "pre_launch_passed", "close") {
                    error_exit("close phase cannot advance: pre_launch_passed must be true");
                }
                if !receipt_bool(&receipt, "validate_passed", "close") {
                    error_exit("close phase cannot advance: validate_passed must be true");
                }
                let epic = json_store::epic_read(&flow_dir, epic_id)
                    .unwrap_or_else(|e| error_exit(&format!("Failed to read epic {epic_id}: {e}")));
                if epic.status != EpicStatus::Done {
                    error_exit(
                        "close phase cannot advance until the epic has been explicitly closed via \
                         `flowctl epic close <epic-id>`.",
                    );
                }
                if let Err(e) = json_store::pipeline_phase_receipt_write(
                    &flow_dir,
                    epic_id,
                    current.as_str(),
                    &receipt,
                ) {
                    error_exit(&format!("Failed to save close receipt: {e}"));
                }
            }
            PipelinePhase::Completed => error_exit("completed is not an actionable phase"),
        }
    }

    let next_phase = current.next().expect("non-terminal phase has a next");
    update_phase(&flow_dir, epic_id, &next_phase);

    if json {
        let mut out = json!({
            "previous_phase": current.as_str(),
            "phase": if next_phase.is_terminal() { Value::Null } else { Value::String(next_phase.as_str().to_string()) },
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
        if next_phase.is_terminal() {
            println!("Advanced: {} → completed", current);
        } else {
            println!("Advanced: {} → {}", current, next_phase);
        }
        if let Some(s) = score {
            println!("Evidence: self-review score {}/30", s);
        }
        if guard_ran {
            println!("Evidence: guard executed");
        }
        if next_phase.is_terminal() {
            println!("Status: all phases complete");
        } else {
            println!("Prompt: {}", next_phase.prompt_template());
        }
    }
}
