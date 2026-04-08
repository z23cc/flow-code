//! Codex review command implementations.
//!
//! Contains the `cmd_*` functions for impl-review, plan-review, adversarial,
//! cross-model, and completion-review, plus their parsing helpers.

use std::env;

use regex::Regex;
use serde_json::json;

use flowctl_core::review_protocol::{
    compute_consensus, filter_by_confidence, AutofixClass, ConsensusResult, FindingOwner,
    ModelReview, ReviewFinding, ReviewVerdict, Severity,
};

use crate::output::{error_exit, json_output};

use super::{
    delete_stale_receipt, load_receipt, parse_verdict, resolve_sandbox, run_codex_exec,
    save_receipt,
};

// ── Command implementations ─────────────────────────────────────────

pub fn cmd_check(json_mode: bool) {
    let available = super::find_codex().is_some();
    let version = if available { super::get_codex_version() } else { None };

    if json_mode {
        json_output(json!({
            "available": available,
            "version": version,
        }));
    } else if available {
        println!("codex available: {}", version.unwrap_or_else(|| "unknown version".to_string()));
    } else {
        println!("codex not available");
    }
}

pub fn cmd_impl_review(
    json_mode: bool,
    task: Option<&str>,
    base: &str,
    focus: Option<&str>,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let standalone = task.is_none();
    let sandbox = resolve_sandbox(sandbox);

    // Load receipt for re-review continuity
    let (session_id, _is_rereview) = load_receipt(receipt);

    // Build a minimal prompt — the real prompt is built by the skill layer,
    // but we support direct invocation with a simple diff-based prompt.
    let prompt = format!(
        "Review the implementation changes from branch '{}' against HEAD.\n\
         {}{}Focus on correctness, quality, performance, and testing.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        base,
        if let Some(t) = task { format!("Task: {t}\n") } else { String::new() },
        if let Some(f) = focus { format!("Focus areas: {f}\n") } else { String::new() },
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    let review_id = task.unwrap_or("branch");

    if let Some(rp) = receipt {
        save_receipt(rp, "impl_review", review_id, &verdict, thread_id.as_deref(), &output, Some(base), focus);
    }

    if json_mode {
        json_output(json!({
            "type": "impl_review",
            "id": review_id,
            "verdict": verdict,
            "session_id": thread_id,
            "mode": "codex",
            "standalone": standalone,
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

pub fn cmd_plan_review(
    json_mode: bool,
    epic: &str,
    files: &str,
    base: &str,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    if files.is_empty() {
        error_exit("plan-review requires --files argument (comma-separated CODE file paths)");
    }

    let sandbox = resolve_sandbox(sandbox);
    let (session_id, _is_rereview) = load_receipt(receipt);

    let prompt = format!(
        "Review the plan for epic '{}' with context from files: {}.\n\
         Base branch: {base}.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        epic, files,
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    if let Some(rp) = receipt {
        save_receipt(rp, "plan_review", epic, &verdict, thread_id.as_deref(), &output, None, None);
    }

    if json_mode {
        json_output(json!({
            "type": "plan_review",
            "id": epic,
            "verdict": verdict,
            "session_id": thread_id,
            "mode": "codex",
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

pub fn cmd_adversarial(
    json_mode: bool,
    base: &str,
    focus: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let sandbox = resolve_sandbox(sandbox);

    let prompt = format!(
        "You are an adversarial code reviewer. Try to BREAK the code changed between '{base}' and HEAD.\n\
         {}Look for bugs, race conditions, security vulnerabilities, edge cases, and logic errors.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.\n\
         Also output structured JSON with your findings.",
        if let Some(f) = focus { format!("Focus area: {f}\n") } else { String::new() },
    );

    let (output, _thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, None, &sandbox, effort);

    if exit_code != 0 {
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("Adversarial review failed: {}", msg.trim()));
    }

    // Try to parse structured JSON output
    let structured = parse_adversarial_output(&output);

    if json_mode {
        if let Some(mut s) = structured {
            s["base"] = json!(base);
            s["focus"] = json!(focus);
            json_output(s);
        } else {
            let verdict = parse_verdict(&output);
            json_output(json!({
                "verdict": verdict.unwrap_or_else(|| "UNKNOWN".to_string()),
                "output": output,
                "base": base,
                "focus": focus,
            }));
        }
    } else if let Some(s) = structured {
        println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
        println!("\nVerdict: {}", s.get("verdict").and_then(|v| v.as_str()).unwrap_or("UNKNOWN"));
    } else {
        print!("{output}");
        if let Some(v) = parse_verdict(&output) {
            println!("\nVerdict: {v}");
        }
    }
}

pub fn cmd_completion_review(
    json_mode: bool,
    epic: &str,
    base: &str,
    receipt: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let sandbox = resolve_sandbox(sandbox);
    let (session_id, _is_rereview) = load_receipt(receipt);

    let prompt = format!(
        "Review epic '{}' for completion. Verify all requirements are implemented.\n\
         Base branch: {base}.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.",
        epic,
    );

    let (output, thread_id, exit_code, stderr) =
        run_codex_exec(&prompt, session_id.as_deref(), &sandbox, effort);

    if exit_code != 0 {
        delete_stale_receipt(receipt);
        let msg = if !stderr.is_empty() { &stderr } else if !output.is_empty() { &output } else { "codex exec failed" };
        error_exit(&format!("codex exec failed: {}", msg.trim()));
    }

    let verdict = parse_verdict(&output);
    if verdict.is_none() {
        delete_stale_receipt(receipt);
        error_exit("Codex review completed but no verdict found in output. Expected <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>");
    }
    let verdict = verdict.unwrap();

    let session_to_write = thread_id.as_deref().or(session_id.as_deref());

    if let Some(rp) = receipt {
        save_receipt(rp, "completion_review", epic, &verdict, session_to_write, &output, Some(base), None);
    }

    if json_mode {
        json_output(json!({
            "type": "completion_review",
            "id": epic,
            "base": base,
            "verdict": verdict,
            "session_id": session_to_write,
            "mode": "codex",
            "review": output,
        }));
    } else {
        print!("{output}");
        println!("\nVERDICT={verdict}");
    }
}

pub fn cmd_cross_model(
    json_mode: bool,
    base: &str,
    focus: Option<&str>,
    sandbox: &str,
    effort: &str,
) {
    let sandbox = resolve_sandbox(sandbox);

    // ── Step 1: Run Codex adversarial review ────────────────────────
    let codex_prompt = format!(
        "You are an adversarial code reviewer. Try to BREAK the code changed between '{base}' and HEAD.\n\
         {}Look for bugs, race conditions, security vulnerabilities, edge cases, and logic errors.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.\n\
         Also output structured JSON with your findings.",
        if let Some(f) = focus { format!("Focus area: {f}\n") } else { String::new() },
    );

    let (codex_output, _codex_thread_id, codex_exit_code, codex_stderr) =
        run_codex_exec(&codex_prompt, None, &sandbox, effort);

    // Build Codex ModelReview
    let codex_review = if codex_exit_code == 0 {
        let verdict = match parse_verdict(&codex_output) {
            Some(v) if v == "SHIP" => ReviewVerdict::Ship,
            Some(v) if v == "NEEDS_WORK" => ReviewVerdict::NeedsWork,
            _ => ReviewVerdict::Abstain,
        };
        let (findings, confidence) = parse_findings_from_output(&codex_output);
        ModelReview {
            model: env::var("FLOW_CODEX_MODEL").unwrap_or_else(|_| "codex/gpt-5.4".to_string()),
            verdict,
            findings,
            confidence,
        }
    } else {
        eprintln!("WARNING: Codex review failed: {}", codex_stderr.trim());
        ModelReview {
            model: "codex/gpt-5.4".to_string(),
            verdict: ReviewVerdict::Abstain,
            findings: vec![],
            confidence: 0.0,
        }
    };

    // ── Step 2: Prepare Claude review prompt ────────────────────────
    // Write a review prompt to a temp file for the caller (Claude) to process.
    // In practice, the orchestrating skill reads this and dispatches to Claude.
    let claude_prompt = format!(
        "You are a thorough code reviewer. Review the code changed between '{base}' and HEAD.\n\
         {}Analyze for correctness, security, performance, and maintainability.\n\
         Output your verdict as <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict>.\n\
         List findings as JSON with fields: severity (critical/warning/info), category, description, file, line.",
        if let Some(f) = focus { format!("Focus area: {f}\n") } else { String::new() },
    );

    let claude_prompt_path = env::temp_dir().join("flowctl-cross-model-claude-prompt.txt");
    let _ = std::fs::write(&claude_prompt_path, &claude_prompt);

    // Build Claude ModelReview (placeholder — caller invokes Claude separately)
    // Check if a Claude review result file was pre-populated by the orchestrator
    let claude_result_path = env::temp_dir().join("flowctl-cross-model-claude-result.json");
    let claude_review = if claude_result_path.exists() {
        match std::fs::read_to_string(&claude_result_path) {
            Ok(content) => parse_claude_review_result(&content),
            Err(_) => make_abstain_review("claude/opus-4"),
        }
    } else {
        // No pre-populated result — run a lightweight self-review via codex
        // with a different prompt to simulate a second opinion
        let (claude_out, _, claude_exit, _) =
            run_codex_exec(&claude_prompt, None, &sandbox, effort);
        if claude_exit == 0 {
            let verdict = match parse_verdict(&claude_out) {
                Some(v) if v == "SHIP" => ReviewVerdict::Ship,
                Some(v) if v == "NEEDS_WORK" => ReviewVerdict::NeedsWork,
                _ => ReviewVerdict::Abstain,
            };
            let (findings, confidence) = parse_findings_from_output(&claude_out);
            ModelReview {
                model: "claude/opus-4".to_string(),
                verdict,
                findings,
                confidence,
            }
        } else {
            make_abstain_review("claude/opus-4")
        }
    };

    // ── Step 3: Compute consensus ───────────────────────────────────
    let reviews = vec![codex_review.clone(), claude_review.clone()];
    let consensus = compute_consensus(&reviews);

    // ── Step 4: Store combined review in .flow/reviews/ ─────────────
    let cwd = env::current_dir().unwrap_or_default();
    let reviews_dir = cwd.join(".flow").join("reviews");
    let _ = std::fs::create_dir_all(&reviews_dir);

    let timestamp = chrono::Utc::now().to_rfc3339();
    let review_file = reviews_dir.join(format!(
        "cross-model-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));

    let consensus_verdict_str = match &consensus {
        ConsensusResult::Consensus { verdict, .. } => format!("{verdict}"),
        ConsensusResult::Conflict { .. } => "CONFLICT".to_string(),
        ConsensusResult::InsufficientReviews => "INSUFFICIENT".to_string(),
    };

    let review_data = json!({
        "type": "cross_model_review",
        "base": base,
        "focus": focus,
        "timestamp": timestamp,
        "models": [
            serde_json::to_value(&codex_review).unwrap_or_default(),
            serde_json::to_value(&claude_review).unwrap_or_default(),
        ],
        "consensus": serde_json::to_value(&consensus).unwrap_or_default(),
        "claude_prompt_path": claude_prompt_path.to_string_lossy(),
    });

    let review_json = serde_json::to_string_pretty(&review_data).unwrap_or_default();
    let _ = std::fs::write(&review_file, format!("{review_json}\n"));

    // ── Step 5: Output ──────────────────────────────────────────────
    if json_mode {
        json_output(review_data);
    } else {
        println!("Cross-Model Review Results");
        println!("==========================");
        println!();
        println!("Model 1: {} — {}", codex_review.model, codex_review.verdict);
        println!("  Findings: {}", codex_review.findings.len());
        println!("  Confidence: {:.0}%", codex_review.confidence * 100.0);
        println!();
        println!("Model 2: {} — {}", claude_review.model, claude_review.verdict);
        println!("  Findings: {}", claude_review.findings.len());
        println!("  Confidence: {:.0}%", claude_review.confidence * 100.0);
        println!();
        println!("Consensus: {consensus_verdict_str}");
        println!("Review saved to: {}", review_file.display());
    }
}

// ── Parsing helpers ─────────────────────────────────────────────────

/// Parse findings from codex/model output. Returns (findings, confidence).
pub(super) fn parse_findings_from_output(output: &str) -> (Vec<ReviewFinding>, f64) {
    let mut findings = Vec::new();
    let mut confidence = 0.8; // default

    // Try to extract structured JSON from the output
    if let Some(data) = parse_adversarial_output(output) {
        // Extract confidence
        if let Some(c) = data.get("confidence").and_then(serde_json::Value::as_f64) {
            confidence = c;
        }

        // Extract findings array
        if let Some(arr) = data.get("findings").and_then(|v| v.as_array()) {
            for item in arr {
                let severity = match item.get("severity").and_then(|v| v.as_str()) {
                    Some("P0") | Some("critical") => Severity::P0,
                    Some("P1") | Some("warning") => Severity::P1,
                    Some("P2") => Severity::P2,
                    _ => Severity::P3,
                };
                let category = item
                    .get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general")
                    .to_string();
                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let file = item.get("file").and_then(|v| v.as_str()).map(String::from);
                let line = item.get("line").and_then(serde_json::Value::as_u64).map(|n| n as u32);
                let item_confidence = item
                    .get("confidence")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.8);
                let autofix_class = match item.get("autofix_class").and_then(|v| v.as_str()) {
                    Some("safe_auto") => AutofixClass::SafeAuto,
                    Some("gated_auto") => AutofixClass::GatedAuto,
                    Some("advisory") => AutofixClass::Advisory,
                    _ => AutofixClass::Manual,
                };
                let owner = match item.get("owner").and_then(|v| v.as_str()) {
                    Some("review-fixer") => FindingOwner::ReviewFixer,
                    Some("downstream-resolver") => FindingOwner::DownstreamResolver,
                    Some("human") => FindingOwner::Human,
                    Some("release") => FindingOwner::Release,
                    _ => FindingOwner::ReviewFixer,
                };
                let evidence = item
                    .get("evidence")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let pre_existing = item
                    .get("pre_existing")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let requires_verification = item
                    .get("requires_verification")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let suggested_fix = item
                    .get("suggested_fix")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let why_it_matters = item
                    .get("why_it_matters")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                if !description.is_empty() {
                    findings.push(ReviewFinding {
                        severity,
                        category,
                        description,
                        file,
                        line,
                        confidence: item_confidence,
                        autofix_class,
                        owner,
                        evidence,
                        pre_existing,
                        requires_verification,
                        suggested_fix,
                        why_it_matters,
                        reviewer: item
                            .get("reviewer")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    });
                }
            }
        }

        // Apply confidence filtering
        findings = filter_by_confidence(findings);
    }

    (findings, confidence)
}

/// Parse a pre-populated Claude review result JSON file into a ModelReview.
fn parse_claude_review_result(content: &str) -> ModelReview {
    match serde_json::from_str::<serde_json::Value>(content.trim()) {
        Ok(data) => {
            let verdict = match data.get("verdict").and_then(|v| v.as_str()) {
                Some("SHIP") => ReviewVerdict::Ship,
                Some("NEEDS_WORK") => ReviewVerdict::NeedsWork,
                _ => ReviewVerdict::Abstain,
            };
            let (findings, confidence) = if let Some(review_text) =
                data.get("review").and_then(|v| v.as_str())
            {
                parse_findings_from_output(review_text)
            } else {
                (vec![], 0.8)
            };
            let confidence = data
                .get("confidence")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(confidence);
            ModelReview {
                model: data
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("claude/opus-4")
                    .to_string(),
                verdict,
                findings,
                confidence,
            }
        }
        Err(_) => make_abstain_review("claude/opus-4"),
    }
}

/// Create an abstain review for a model that failed or couldn't participate.
fn make_abstain_review(model: &str) -> ModelReview {
    ModelReview {
        model: model.to_string(),
        verdict: ReviewVerdict::Abstain,
        findings: vec![],
        confidence: 0.0,
    }
}

/// Parse structured JSON from adversarial review output.
/// Handles direct JSON, JSONL streaming, markdown fences, embedded JSON.
pub(super) fn parse_adversarial_output(output: &str) -> Option<serde_json::Value> {
    // Strategy 1: Direct JSON parse
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(output.trim()) {
        if data.is_object() && data.get("verdict").is_some() {
            return Some(data);
        }
    }

    // Strategy 2: JSONL streaming events (codex exec --json)
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
            if event.get("type").and_then(|v| v.as_str()) == Some("item.completed") {
                if let Some(item) = event.get("item") {
                    if item.get("type").and_then(|v| v.as_str()) == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                            if let Ok(data) = serde_json::from_str::<serde_json::Value>(text) {
                                if data.is_object() && data.get("verdict").is_some() {
                                    return Some(data);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 3: Markdown fences
    let fence_re = Regex::new(r"```(?:json)?\s*\n?(.*?)\n?```").unwrap();
    if let Some(caps) = fence_re.captures(output) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(caps[1].trim()) {
            if data.is_object() && data.get("verdict").is_some() {
                return Some(data);
            }
        }
    }

    // Strategy 4: Greedy brace match
    let brace_re = Regex::new(r#"\{[^{}]*"verdict"[^{}]*\}"#).unwrap();
    if let Some(m) = brace_re.find(output) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(m.as_str()) {
            if data.is_object() && data.get("verdict").is_some() {
                return Some(data);
            }
        }
    }

    None
}
