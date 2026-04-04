//! Guard and worker-prompt commands.

use std::fs;
use std::process::Command;

use serde_json::json;

use crate::output::{error_exit, json_output};

use flowctl_core::types::CONFIG_FILE;

use super::{deep_merge, get_default_config, get_flow_dir};

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

    let stack = config.get("stack").cloned().unwrap_or(json!({}));
    let stack_obj = stack.as_object();

    if stack_obj.is_none() || stack_obj.unwrap().is_empty() {
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

    if commands.is_empty() {
        if json_mode {
            json_output(json!({
                "results": [],
                "message": "no guard commands configured",
            }));
        } else {
            println!("No guard commands found in stack config.");
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

    for (layer_name, cmd_type, cmd) in &commands {
        if !json_mode {
            println!("\u{25b8} [{}] {}: {}", layer_name, cmd_type, cmd);
        }

        let output = Command::new("sh")
            .args(["-c", cmd])
            .current_dir(&repo_root)
            .output();

        let rc = match &output {
            Ok(o) => o.status.code().unwrap_or(1),
            Err(_) => 1,
        };

        let passed = rc == 0;
        if !passed {
            all_passed = false;
        }

        results.push(json!({
            "layer": layer_name,
            "type": cmd_type,
            "command": cmd,
            "passed": passed,
            "exit_code": rc,
        }));

        if !json_mode {
            let status = if passed { "\u{2713}" } else { "\u{2717}" };
            println!("  {} exit {}", status, rc);
        }
    }

    if json_mode {
        json_output(json!({"results": results}));
    } else {
        let passed_count = results.iter().filter(|r| r["passed"].as_bool().unwrap_or(false)).count();
        let total = results.len();
        let suffix = if all_passed { "" } else { " \u{2014} FAILED" };
        println!("\n{}/{} guards passed{}", passed_count, total, suffix);
    }

    if !all_passed {
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
