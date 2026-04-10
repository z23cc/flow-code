//! Stats command: flowctl stats [--epic <id>] [--weekly] [--tokens] [--bottlenecks] [--dora] [--format json]
//!
//! TTY-aware: table output for terminals, JSON when piped or --json is passed.
//! Stats are computed from JSON file store (epics, tasks, state files).

use std::io::IsTerminal;

use clap::Subcommand;
use serde_json::json;

use super::helpers::get_flow_dir;
use crate::output::{error_exit, json_output, pretty_output};

/// Determine if output should be JSON: explicit --json flag, or stdout is not a terminal.
fn should_json(json_flag: bool) -> bool {
    json_flag || !std::io::stdout().is_terminal()
}

/// Stats subcommands.
#[derive(Subcommand, Debug)]
pub enum StatsCmd {
    /// Show overall summary.
    Summary,
    /// Show per-epic breakdown.
    Epic {
        /// Specific epic ID (optional, shows all if omitted).
        #[arg(long)]
        id: Option<String>,
    },
    /// Show weekly trends.
    Weekly {
        /// Number of weeks to show (default: 8).
        #[arg(long, default_value = "8")]
        weeks: u32,
    },
    /// Show token/cost breakdown.
    Tokens {
        /// Filter by epic ID.
        #[arg(long)]
        epic: Option<String>,
    },
    /// Show bottleneck analysis.
    Bottlenecks {
        /// Max results (default: 10).
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Show DORA metrics.
    Dora,
    /// Generate monthly rollups from daily data.
    Rollup,
    /// Run auto-cleanup (delete old events/rollups).
    Cleanup,
}

pub fn dispatch(cmd: &StatsCmd, json_flag: bool) {
    match cmd {
        StatsCmd::Summary => cmd_summary(json_flag),
        StatsCmd::Epic { id } => cmd_epic(json_flag, id.as_deref()),
        StatsCmd::Weekly { weeks: _ } => cmd_weekly(json_flag),
        StatsCmd::Tokens { epic: _ } => cmd_tokens(json_flag),
        StatsCmd::Bottlenecks { limit: _ } => cmd_bottlenecks(json_flag),
        StatsCmd::Dora => cmd_dora(json_flag),
        StatsCmd::Rollup => cmd_rollup(json_flag),
        StatsCmd::Cleanup => cmd_cleanup(json_flag),
    }
}

fn cmd_summary(json_flag: bool) {
    let flow_dir = get_flow_dir();
    let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap_or_default();
    let open_epics = epics
        .iter()
        .filter(|e| e.status.to_string() == "open")
        .count();

    let mut total_tasks = 0i64;
    let mut done_tasks = 0i64;
    let mut in_progress_tasks = 0i64;
    let mut blocked_tasks = 0i64;

    for epic in &epics {
        let tasks =
            flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic.id).unwrap_or_default();
        for task in &tasks {
            total_tasks += 1;
            match task.status {
                flowctl_core::state_machine::Status::Done => done_tasks += 1,
                flowctl_core::state_machine::Status::InProgress => in_progress_tasks += 1,
                flowctl_core::state_machine::Status::Blocked => blocked_tasks += 1,
                _ => {}
            }
        }
    }

    let total_events = flowctl_core::json_store::events_read_all(&flow_dir)
        .map(|v| v.len() as i64)
        .unwrap_or(0);

    if should_json(json_flag) {
        json_output(json!({
            "total_epics": epics.len(),
            "open_epics": open_epics,
            "total_tasks": total_tasks,
            "done_tasks": done_tasks,
            "in_progress_tasks": in_progress_tasks,
            "blocked_tasks": blocked_tasks,
            "total_events": total_events,
            "total_tokens": 0,
            "total_cost_usd": 0.0,
        }));
    } else {
        println!("flowctl Stats Summary");
        println!("{}", "=".repeat(40));
        println!("Epics:       {} total, {} open", epics.len(), open_epics);
        println!(
            "Tasks:       {} total, {} done, {} in progress, {} blocked",
            total_tasks, done_tasks, in_progress_tasks, blocked_tasks
        );
        println!("Events:      {}", total_events);
    }
}

fn cmd_epic(json_flag: bool, epic_filter: Option<&str>) {
    let flow_dir = get_flow_dir();
    let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap_or_default();
    let filtered: Vec<_> = if let Some(eid) = epic_filter {
        epics.into_iter().filter(|e| e.id == eid).collect()
    } else {
        epics
    };

    let mut data: Vec<serde_json::Value> = Vec::new();
    for epic in &filtered {
        let tasks =
            flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic.id).unwrap_or_default();
        let done_count = tasks
            .iter()
            .filter(|t| t.status == flowctl_core::state_machine::Status::Done)
            .count();
        data.push(json!({
            "epic_id": epic.id,
            "title": epic.title,
            "status": epic.status.to_string(),
            "task_count": tasks.len(),
            "done_count": done_count,
            "avg_duration_secs": 0,
            "total_tokens": 0,
            "total_cost": 0.0,
        }));
    }

    if should_json(json_flag) {
        json_output(json!({ "epics": data, "count": data.len() }));
    } else if data.is_empty() {
        println!("No epic stats found.");
    } else {
        println!(
            "{:<30} {:>6} {:>5}/{:>5}",
            "EPIC", "STATUS", "DONE", "TOTAL"
        );
        println!("{}", "-".repeat(55));
        for e in &data {
            println!(
                "{:<30} {:>6} {:>5}/{:>5}",
                truncate(e["epic_id"].as_str().unwrap_or(""), 30),
                e["status"].as_str().unwrap_or(""),
                e["done_count"],
                e["task_count"],
            );
        }
    }
}

fn cmd_weekly(json_flag: bool) {
    if should_json(json_flag) {
        json_output(
            json!({ "weekly_trends": [], "message": "Weekly trends not available (file-based storage)" }),
        );
    } else {
        println!("Weekly trends not available with file-based storage.");
    }
}

fn cmd_tokens(json_flag: bool) {
    if should_json(json_flag) {
        json_output(
            json!({ "token_usage": [], "message": "Token tracking not available (file-based storage)" }),
        );
    } else {
        println!("Token usage tracking not available with file-based storage.");
    }
}

fn cmd_bottlenecks(json_flag: bool) {
    if should_json(json_flag) {
        json_output(
            json!({ "bottlenecks": [], "message": "Bottleneck analysis not available (file-based storage)" }),
        );
    } else {
        println!("Bottleneck analysis not available with file-based storage.");
    }
}

fn cmd_dora(json_flag: bool) {
    if should_json(json_flag) {
        json_output(json!({
            "lead_time_hours": null,
            "throughput_per_week": 0.0,
            "change_failure_rate": 0.0,
            "time_to_restore_hours": null,
            "message": "DORA metrics not available (file-based storage)",
        }));
    } else {
        println!("DORA metrics not available with file-based storage.");
    }
}

fn cmd_rollup(json_flag: bool) {
    if should_json(json_flag) {
        json_output(
            json!({ "months_updated": 0, "message": "Rollups not applicable (file-based storage)" }),
        );
    } else {
        println!("Rollups not applicable with file-based storage.");
    }
}

fn cmd_cleanup(json_flag: bool) {
    if should_json(json_flag) {
        json_output(
            json!({ "deleted": 0, "message": "Cleanup not applicable (file-based storage)" }),
        );
    } else {
        println!("Cleanup not applicable with file-based storage.");
    }
}

// ── DAG rendering ────────────────────────────────────────────────────

pub fn cmd_dag(json_flag: bool, epic_id: Option<String>) {
    let flow_dir = get_flow_dir();

    let epic_id = match epic_id {
        Some(id) => id,
        None => {
            let epics = flowctl_core::json_store::epic_list(&flow_dir).unwrap_or_default();
            match epics.iter().find(|e| e.status.to_string() == "open") {
                Some(e) => e.id.clone(),
                None => error_exit("No open epic found. Use --epic <id> to specify."),
            }
        }
    };

    let tasks =
        flowctl_core::json_store::task_list_by_epic(&flow_dir, &epic_id).unwrap_or_default();
    if tasks.is_empty() {
        error_exit(&format!("No tasks found for epic {}", epic_id));
    }

    let dag = match flowctl_core::TaskDag::from_tasks(&tasks) {
        Ok(d) => {
            if let Some(cycle) = d.detect_cycles() {
                error_exit(&format!("Cycle detected in DAG: {}", cycle.join(" -> ")));
            }
            d
        }
        Err(e) => error_exit(&format!("Failed to build DAG: {}", e)),
    };

    // Assign layers via longest-path from sources
    let topo = dag.topological_sort_ids();
    let mut layer_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for id in &topo {
        let deps = dag.dependencies(id);
        let my_layer = if deps.is_empty() {
            0
        } else {
            deps.iter()
                .filter_map(|d| layer_of.get(d))
                .max()
                .map(|m| m + 1)
                .unwrap_or(0)
        };
        layer_of.insert(id.clone(), my_layer);
    }

    let max_layer = layer_of.values().copied().max().unwrap_or(0);

    if should_json(json_flag) {
        let layers: Vec<serde_json::Value> = (0..=max_layer)
            .map(|layer| {
                let nodes: Vec<serde_json::Value> = tasks
                    .iter()
                    .filter(|t| layer_of.get(&t.id) == Some(&layer))
                    .map(|t| {
                        json!({
                            "id": t.id,
                            "status": t.status.to_string(),
                            "deps": dag.dependencies(&t.id),
                        })
                    })
                    .collect();
                json!({"layer": layer, "nodes": nodes})
            })
            .collect();
        json_output(json!({"epic": epic_id, "layers": layers}));
        return;
    }

    // ASCII rendering
    use std::fmt::Write as _;
    let mut buf = String::new();
    writeln!(buf, "DAG for {}", epic_id).ok();
    writeln!(buf).ok();

    for layer in 0..=max_layer {
        let mut nodes_in_layer: Vec<&flowctl_core::types::Task> = tasks
            .iter()
            .filter(|t| layer_of.get(&t.id) == Some(&layer))
            .collect();
        nodes_in_layer.sort_by(|a, b| a.id.cmp(&b.id));

        for task in &nodes_in_layer {
            let status_icon = match task.status {
                flowctl_core::Status::Done => "done",
                flowctl_core::Status::InProgress => " >> ",
                flowctl_core::Status::Blocked => "blck",
                flowctl_core::Status::Todo => "todo",
                _ => " ?? ",
            };
            let short_id = task.id.rsplit('.').next().unwrap_or(&task.id);
            let label = format!(".{} [{}]", short_id, status_icon);
            let indent = "  ".repeat(layer);
            let connector = if layer > 0 {
                "\u{2514}\u{2500}\u{2500} "
            } else {
                ""
            };
            writeln!(
                buf,
                "{}{}\u{250c}\u{2500}{}\u{2500}\u{2510}",
                indent,
                connector,
                "\u{2500}".repeat(label.len())
            )
            .ok();
            writeln!(
                buf,
                "{}{}\u{2502} {} \u{2502}",
                indent,
                if layer > 0 { "    " } else { "" },
                label
            )
            .ok();
            writeln!(
                buf,
                "{}{}\u{2514}\u{2500}{}\u{2500}\u{2518}",
                indent,
                if layer > 0 { "    " } else { "" },
                "\u{2500}".repeat(label.len())
            )
            .ok();
        }

        if layer < max_layer {
            let next_layer_nodes: Vec<&flowctl_core::types::Task> = tasks
                .iter()
                .filter(|t| layer_of.get(&t.id) == Some(&(layer + 1)))
                .collect();
            if !next_layer_nodes.is_empty() {
                let indent = "  ".repeat(layer + 1);
                writeln!(buf, "{}\u{2502}", indent).ok();
                writeln!(buf, "{}\u{2193}", indent).ok();
            }
        }
    }
    pretty_output("dag", &buf);
}

// ── Estimate command ─────────────────────────────────────────────────

pub fn cmd_estimate(json_flag: bool, epic_id: &str) {
    let flow_dir = get_flow_dir();
    let tasks = flowctl_core::json_store::task_list_by_epic(&flow_dir, epic_id).unwrap_or_default();

    if tasks.is_empty() {
        error_exit(&format!("No tasks found for epic {}", epic_id));
    }

    let mut completed_durations: Vec<u64> = Vec::new();
    let mut incomplete_count = 0u32;

    for task in &tasks {
        if task.status == flowctl_core::Status::Done {
            if let Ok(state) = flowctl_core::json_store::state_read(&flow_dir, &task.id) {
                if let Some(dur) = state.duration_seconds {
                    completed_durations.push(dur);
                }
            }
        } else if task.status != flowctl_core::state_machine::Status::Skipped {
            incomplete_count += 1;
        }
    }

    let avg_secs = if completed_durations.is_empty() {
        0u64
    } else {
        completed_durations.iter().sum::<u64>() / completed_durations.len() as u64
    };

    let estimated_remaining_secs = avg_secs * incomplete_count as u64;
    let done_count = completed_durations.len();

    if should_json(json_flag) {
        json_output(json!({
            "epic": epic_id,
            "total_tasks": tasks.len(),
            "done_tasks": done_count,
            "incomplete_tasks": incomplete_count,
            "avg_duration_secs": avg_secs,
            "estimated_remaining_secs": estimated_remaining_secs,
        }));
    } else {
        let mins = estimated_remaining_secs / 60;
        let secs = estimated_remaining_secs % 60;
        println!(
            "Estimated remaining: {}m {}s ({} tasks, avg {}s/task)",
            mins, secs, incomplete_count, avg_secs
        );
        println!(
            "  Done: {}/{}, Remaining: {}",
            done_count,
            tasks.len(),
            incomplete_count
        );
    }
}

// ── Formatting helpers ────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
