//! Guard and worker-prompt commands.
//!
//! The guard command runs test/lint/typecheck commands from the stack config
//! and filters their output to show only summaries, failures, and warnings.
//! This achieves ~90% token reduction for LLM consumers.

use std::fs;
use std::process::Command;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

use flowctl_core::project_context::ProjectContext;
use flowctl_core::types::CONFIG_FILE;

use super::{deep_merge, get_default_config, get_flow_dir};

// ── Output filtering ─────────────────────────────────────────────

/// Regex for parsing "test result:" summary lines from cargo test.
fn test_result_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"test result: (\w+)\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;\s+(\d+) measured;\s+(\d+) filtered out(?:;\s+finished in ([\d.]+)s)?"
        ).unwrap()
    })
}

/// Aggregated test result counters for compact display.
#[derive(Debug, Default, Clone)]
struct AggregatedTestResult {
    passed: usize,
    failed: usize,
    ignored: usize,
    filtered_out: usize,
    suites: usize,
    duration_secs: f64,
    has_duration: bool,
}

impl AggregatedTestResult {
    /// Parse a "test result:" line into counters.
    fn parse_line(line: &str) -> Option<Self> {
        let caps = test_result_re().captures(line)?;
        let status = caps.get(1)?.as_str();

        // Only aggregate "ok" results; failed results get shown differently.
        if status != "ok" {
            return None;
        }

        let passed = caps.get(2)?.as_str().parse().ok()?;
        let failed = caps.get(3)?.as_str().parse().ok()?;
        let ignored = caps.get(4)?.as_str().parse().ok()?;
        let filtered_out = caps.get(6)?.as_str().parse().ok()?;

        let (duration_secs, has_duration) = if let Some(d) = caps.get(7) {
            (d.as_str().parse().unwrap_or(0.0), true)
        } else {
            (0.0, false)
        };

        Some(Self {
            passed,
            failed,
            ignored,
            filtered_out,
            suites: 1,
            duration_secs,
            has_duration,
        })
    }

    fn merge(&mut self, other: &Self) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.ignored += other.ignored;
        self.filtered_out += other.filtered_out;
        self.suites += other.suites;
        self.duration_secs += other.duration_secs;
        self.has_duration = self.has_duration && other.has_duration;
    }

    /// One-line compact format: "42 passed, 1 ignored (3 suites, 1.23s)"
    fn format_compact(&self) -> String {
        let mut parts = vec![format!("{} passed", self.passed)];
        if self.ignored > 0 {
            parts.push(format!("{} ignored", self.ignored));
        }
        if self.filtered_out > 0 {
            parts.push(format!("{} filtered out", self.filtered_out));
        }
        let counts = parts.join(", ");

        let suite_text = if self.suites == 1 {
            "1 suite".to_string()
        } else {
            format!("{} suites", self.suites)
        };

        if self.has_duration {
            format!("{} ({}, {:.2}s)", counts, suite_text, self.duration_secs)
        } else {
            format!("{} ({})", counts, suite_text)
        }
    }
}

/// Filter cargo test output: remove compilation noise, keep failures + summary.
fn filter_cargo_test(output: &str) -> FilterResult {
    let mut failures: Vec<String> = Vec::new();
    let mut summary_lines: Vec<String> = Vec::new();
    let mut in_failure_section = false;
    let mut current_failure: Vec<String> = Vec::new();

    for line in output.lines() {
        // Skip compilation noise
        let trimmed = line.trim_start();
        if trimmed.starts_with("Compiling")
            || trimmed.starts_with("Downloading")
            || trimmed.starts_with("Downloaded")
            || trimmed.starts_with("Finished")
        {
            continue;
        }

        // Skip "running N tests" and individual passing tests
        if line.starts_with("running ") || (line.starts_with("test ") && line.ends_with("... ok"))
        {
            continue;
        }

        // Detect failures section
        if line == "failures:" {
            in_failure_section = true;
            continue;
        }

        if in_failure_section {
            if line.starts_with("test result:") {
                in_failure_section = false;
                summary_lines.push(line.to_string());
            } else if line.starts_with("    ") || line.starts_with("---- ") {
                current_failure.push(line.to_string());
            } else if line.trim().is_empty() && !current_failure.is_empty() {
                failures.push(current_failure.join("\n"));
                current_failure.clear();
            } else if !line.trim().is_empty() {
                current_failure.push(line.to_string());
            }
        }

        // Capture test result summary outside failure section
        if !in_failure_section && line.starts_with("test result:") {
            summary_lines.push(line.to_string());
        }
    }

    if !current_failure.is_empty() {
        failures.push(current_failure.join("\n"));
    }

    // All passed: try to aggregate into a single compact line
    if failures.is_empty() && !summary_lines.is_empty() {
        let mut aggregated: Option<AggregatedTestResult> = None;
        let mut all_parsed = true;

        for line in &summary_lines {
            if let Some(parsed) = AggregatedTestResult::parse_line(line) {
                if let Some(ref mut agg) = aggregated {
                    agg.merge(&parsed);
                } else {
                    aggregated = Some(parsed);
                }
            } else {
                all_parsed = false;
                break;
            }
        }

        if all_parsed {
            if let Some(agg) = aggregated {
                if agg.suites > 0 {
                    let summary = agg.format_compact();
                    return FilterResult {
                        summary,
                        errors: vec![],
                    };
                }
            }
        }

        // Fallback: join summary lines
        let summary = summary_lines.join("; ");
        return FilterResult {
            summary,
            errors: vec![],
        };
    }

    // Failures present: include them
    let error_texts: Vec<String> = failures.iter().take(10).cloned().collect();
    let summary = if !summary_lines.is_empty() {
        summary_lines.join("; ")
    } else {
        format!("{} failure(s)", failures.len())
    };

    FilterResult {
        summary,
        errors: error_texts,
    }
}

/// Filter cargo clippy / lint output: group warnings, keep errors.
fn filter_lint_output(output: &str) -> FilterResult {
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut error_details: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim_start();

        // Skip compilation noise
        if trimmed.starts_with("Compiling")
            || trimmed.starts_with("Checking")
            || trimmed.starts_with("Downloading")
            || trimmed.starts_with("Downloaded")
            || trimmed.starts_with("Finished")
        {
            continue;
        }

        // Count errors and warnings
        if line.starts_with("error:") || line.starts_with("error[") {
            // Skip meta-errors
            if line.contains("aborting due to") || line.contains("could not compile") {
                continue;
            }
            error_count += 1;
            let detail = if line.len() > 160 {
                format!("{}...", &line[..157])
            } else {
                line.to_string()
            };
            error_details.push(detail);
        } else if line.starts_with("warning:") || line.starts_with("warning[") {
            // Skip summary lines like "warning: `crate` generated N warnings"
            if line.contains("generated") && line.contains("warning") {
                continue;
            }
            warning_count += 1;
        }
    }

    if error_count == 0 && warning_count == 0 {
        return FilterResult {
            summary: "no issues".to_string(),
            errors: vec![],
        };
    }

    let summary = format!("{} errors, {} warnings", error_count, warning_count);
    FilterResult {
        summary,
        errors: error_details.into_iter().take(5).collect(),
    }
}

/// Minimal filter for typecheck output (similar to lint).
fn filter_typecheck_output(output: &str) -> FilterResult {
    // Typecheck uses the same error/warning pattern as lint
    filter_lint_output(output)
}

/// Result of filtering a guard command's output.
struct FilterResult {
    /// One-line summary (e.g., "42 passed, 0 ignored (3 suites, 1.23s)")
    summary: String,
    /// Error details to show (truncated list).
    errors: Vec<String>,
}

/// Filter guard command output based on command type.
fn filter_guard_output(cmd_type: &str, stdout: &str, stderr: &str) -> FilterResult {
    // Combine stdout + stderr (cargo outputs to stderr for build info)
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    match cmd_type {
        "test" => filter_cargo_test(&combined),
        "lint" => filter_lint_output(&combined),
        "typecheck" => filter_typecheck_output(&combined),
        _ => {
            // Unknown type: show last few meaningful lines
            let meaningful: Vec<&str> = combined
                .lines()
                .filter(|l| !l.trim().is_empty())
                .collect();
            let last_lines: Vec<&str> = meaningful.iter().rev().take(5).rev().copied().collect();
            FilterResult {
                summary: last_lines.join("; "),
                errors: vec![],
            }
        }
    }
}

// ── Tool availability check ──────────────────────────────────────

/// Check if the program required by a guard command is available in PATH.
fn check_tool_available(cmd: &str) -> bool {
    let program = cmd.split_whitespace().next().unwrap_or("");
    // Handle common wrappers: npx/bunx/pnpx are the actual binary to check
    let actual = match program {
        "npx" | "bunx" | "pnpx" => program,
        _ => program,
    };
    which::which(actual).is_ok()
}

// ── Guard command ──────────────────────────────────────────────────

pub fn cmd_guard(json_mode: bool, layer: String) {
    let flow_dir = get_flow_dir();
    if !flow_dir.exists() {
        error_exit(".flow/ does not exist. Run 'flowctl init' first.");
    }

    // Load stack config
    let config_path = flow_dir.join(CONFIG_FILE);
    let config = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                let raw =
                    serde_json::from_str::<serde_json::Value>(&content).unwrap_or(json!({}));
                deep_merge(&get_default_config(), &raw)
            }
            Err(_) => get_default_config(),
        }
    } else {
        get_default_config()
    };

    // ── Priority chain: project-context → stack config → auto-detection ──
    let pc = ProjectContext::load_resolved();
    let pc_commands = pc.as_ref().map(|ctx| {
        let gc = &ctx.guard_commands;
        let mut cmds: Vec<(String, String, String)> = Vec::new();
        let layer_name = "project".to_string();
        if let Some(ref cmd) = gc.test {
            if !cmd.is_empty() && (layer == "all" || layer == "project") {
                cmds.push((layer_name.clone(), "test".to_string(), cmd.clone()));
            }
        }
        if let Some(ref cmd) = gc.lint {
            if !cmd.is_empty() && (layer == "all" || layer == "project") {
                cmds.push((layer_name.clone(), "lint".to_string(), cmd.clone()));
            }
        }
        if let Some(ref cmd) = gc.typecheck {
            if !cmd.is_empty() && (layer == "all" || layer == "project") {
                cmds.push((layer_name.clone(), "typecheck".to_string(), cmd.clone()));
            }
        }
        if let Some(ref cmd) = gc.format_check {
            if !cmd.is_empty() && (layer == "all" || layer == "project") {
                cmds.push((layer_name.clone(), "format_check".to_string(), cmd.clone()));
            }
        }
        cmds
    });

    // Use project-context commands if any are defined; otherwise fall back to stack config.
    let use_project_context = pc_commands.as_ref().is_some_and(|c| !c.is_empty());

    let stack = config.get("stack").cloned().unwrap_or(json!({}));
    let stack_obj = stack.as_object();

    if !use_project_context && (stack_obj.is_none() || stack_obj.unwrap().is_empty()) {
        if json_mode {
            json_output(json!({
                "results": [],
                "message": "no stack detected, nothing to run",
            }));
        } else {
            println!("No stack detected. Nothing to run.");
        }
        return;
    }

    let cmd_types = ["test", "lint", "typecheck"];
    let mut commands: Vec<(String, String, String)> = Vec::new(); // (layer_name, type, cmd)

    if use_project_context {
        commands = pc_commands.unwrap();
    } else {
        for (layer_name, layer_conf) in stack_obj.unwrap() {
            if layer != "all" && layer_name != &layer {
                continue;
            }
            if let Some(layer_obj) = layer_conf.as_object() {
                for ct in &cmd_types {
                    if let Some(cmd_val) = layer_obj.get(*ct) {
                        if let Some(cmd_str) = cmd_val.as_str() {
                            if !cmd_str.is_empty() {
                                commands.push((
                                    layer_name.clone(),
                                    ct.to_string(),
                                    cmd_str.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if commands.is_empty() {
        if json_mode {
            json_output(json!({
                "results": [],
                "message": "no guard commands configured, nothing to run",
            }));
        } else {
            println!("No guard commands configured — nothing to run.");
        }
        return;
    }

    // Find repo root for running commands
    let repo_root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(
                    String::from_utf8_lossy(&o.stdout)
                        .trim()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .unwrap_or_else(|| ".".to_string());

    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut all_passed = true;
    let mut pass_count: usize = 0;
    let mut skip_count: usize = 0;
    let mut fail_count: usize = 0;
    let mut pretty_buf = String::new();

    for (layer_name, cmd_type, cmd) in &commands {
        // Check if the command's program is available before running
        if !check_tool_available(cmd) {
            let program = cmd.split_whitespace().next().unwrap_or("unknown");
            skip_count += 1;
            results.push(json!({
                "name": format!("{}/{}", layer_name, cmd_type),
                "status": "skipped",
                "summary": format!("{} not found in PATH", program),
            }));
            if !json_mode {
                pretty_buf.push_str(&format!(
                    "\u{2014} [{}] {}: skipped ({} not found in PATH)\n",
                    layer_name, cmd_type, program
                ));
            }
            continue;
        }

        let output = Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&repo_root)
            .output();

        let (rc, stdout_str, stderr_str) = match &output {
            Ok(o) => (
                o.status.code().unwrap_or(1),
                String::from_utf8_lossy(&o.stdout).to_string(),
                String::from_utf8_lossy(&o.stderr).to_string(),
            ),
            Err(_) => (1, String::new(), String::new()),
        };

        let passed = rc == 0;
        if passed {
            pass_count += 1;
        } else {
            all_passed = false;
            fail_count += 1;
        }

        let filtered = filter_guard_output(cmd_type, &stdout_str, &stderr_str);
        let status_str = if passed { "pass" } else { "fail" };

        let mut guard_entry = json!({
            "name": format!("{}/{}", layer_name, cmd_type),
            "status": status_str,
            "summary": filtered.summary,
        });
        if !filtered.errors.is_empty() {
            guard_entry["errors"] = json!(filtered.errors);
        }
        results.push(guard_entry);

        if !json_mode {
            let icon = if passed { "\u{2713}" } else { "\u{2717}" };
            pretty_buf.push_str(&format!(
                "{} [{}] {}: {}\n",
                icon, layer_name, cmd_type, filtered.summary
            ));
            // Show errors inline for failed guards
            for err in &filtered.errors {
                for err_line in err.lines().take(3) {
                    pretty_buf.push_str(&format!("    {}\n", err_line));
                }
            }
        }
    }

    if json_mode {
        json_output(json!({
            "guards": results,
            "passed": pass_count,
            "failed": fail_count,
            "skipped": skip_count,
        }));
    } else {
        let total = commands.len();
        let mut suffix_parts: Vec<String> = Vec::new();
        if fail_count > 0 {
            suffix_parts.push(format!("{} failed", fail_count));
        }
        if skip_count > 0 {
            suffix_parts.push(format!("{} skipped", skip_count));
        }
        let suffix = if suffix_parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", suffix_parts.join(", "))
        };
        let fail_marker = if !all_passed { " \u{2014} FAILED" } else { "" };
        pretty_buf.push_str(&format!(
            "\n{}/{} guards passed{}{}",
            pass_count, total, suffix, fail_marker
        ));
        pretty_output("guard", &pretty_buf);
    }

    // Only exit non-zero if there are actual failures; skipped checks do NOT block
    if fail_count > 0 {
        std::process::exit(1);
    }
}

// ── Worker-prompt command ──────────────────────────────────────────

pub fn cmd_worker_prompt(json_mode: bool, task: String, tdd: bool, review: Option<String>) {
    // Determine epic from task ID
    let epic_id = if flowctl_core::id::is_task_id(&task) {
        flowctl_core::id::epic_id_from_task(&task).unwrap_or_else(|_| task.clone())
    } else {
        task.clone()
    };

    // Build phase sequence
    let has_review = review.is_some();
    let phases: Vec<&str> = if tdd && has_review {
        flowctl_core::types::PHASE_SEQ_TDD
            .iter()
            .chain(flowctl_core::types::PHASE_SEQ_REVIEW.iter())
            .copied()
            .collect::<std::collections::BTreeSet<&str>>()
            .into_iter()
            .collect()
    } else if tdd {
        flowctl_core::types::PHASE_SEQ_TDD.to_vec()
    } else if has_review {
        flowctl_core::types::PHASE_SEQ_REVIEW.to_vec()
    } else {
        flowctl_core::types::PHASE_SEQ_DEFAULT.to_vec()
    };

    // Build a minimal bootstrap prompt
    let review_line = review
        .as_ref()
        .map(|r| format!("REVIEW_MODE: {}", r))
        .unwrap_or_else(|| "REVIEW_MODE: none".to_string());
    let tdd_line = if tdd { "TDD_MODE: true" } else { "TDD_MODE: false" };

    let phase_list: Vec<String> = phases
        .iter()
        .filter_map(|pid| {
            flowctl_core::types::PHASE_DEFS
                .iter()
                .find(|(id, _, _)| id == pid)
                .map(|(id, title, _)| format!("Phase {}: {}", id, title))
        })
        .collect();

    let prompt_text = format!(
        "TASK_ID: {task}\nEPIC_ID: {epic_id}\n{tdd_line}\n{review_line}\nTEAM_MODE: true\n\nPhase sequence:\n{phases}\n\nExecute phases in order. Use flowctl worker-phase next/done to track progress.",
        task = task,
        epic_id = epic_id,
        tdd_line = tdd_line,
        review_line = review_line,
        phases = phase_list.join("\n"),
    );

    let estimated_tokens = prompt_text.len() / 4;

    if json_mode {
        json_output(json!({
            "prompt": prompt_text,
            "mode": "bootstrap",
            "estimated_tokens": estimated_tokens,
        }));
    } else {
        println!("{}", prompt_text);
    }
}
