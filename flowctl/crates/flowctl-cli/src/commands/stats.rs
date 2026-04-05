//! Stats command: flowctl stats [--epic <id>] [--weekly] [--tokens] [--bottlenecks] [--dora] [--format json]
//!
//! TTY-aware: table output for terminals, JSON when piped or --json is passed.

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Subcommand;
use serde_json::json;

use crate::output::{error_exit, json_output};

/// Open DB or exit with error.
fn open_db_or_exit() -> crate::commands::db_shim::Connection {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match crate::commands::db_shim::open(&cwd) {
        Ok(conn) => conn,
        Err(e) => {
            error_exit(&format!("Cannot open database: {}", e));
        }
    }
}

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
        StatsCmd::Weekly { weeks } => cmd_weekly(json_flag, *weeks),
        StatsCmd::Tokens { epic } => cmd_tokens(json_flag, epic.as_deref()),
        StatsCmd::Bottlenecks { limit } => cmd_bottlenecks(json_flag, *limit),
        StatsCmd::Dora => cmd_dora(json_flag),
        StatsCmd::Rollup => cmd_rollup(json_flag),
        StatsCmd::Cleanup => cmd_cleanup(json_flag),
    }
}

fn cmd_summary(json_flag: bool) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let summary = match stats.summary() {
        Ok(s) => s,
        Err(e) => error_exit(&format!("Failed to query stats: {}", e)),
    };

    if should_json(json_flag) {
        json_output(json!({
            "total_epics": summary.total_epics,
            "open_epics": summary.open_epics,
            "total_tasks": summary.total_tasks,
            "done_tasks": summary.done_tasks,
            "in_progress_tasks": summary.in_progress_tasks,
            "blocked_tasks": summary.blocked_tasks,
            "total_events": summary.total_events,
            "total_tokens": summary.total_tokens,
            "total_cost_usd": summary.total_cost_usd,
        }));
    } else {
        println!("flowctl Stats Summary");
        println!("{}", "=".repeat(40));
        println!("Epics:       {} total, {} open", summary.total_epics, summary.open_epics);
        println!(
            "Tasks:       {} total, {} done, {} in progress, {} blocked",
            summary.total_tasks, summary.done_tasks, summary.in_progress_tasks, summary.blocked_tasks
        );
        println!("Events:      {}", summary.total_events);
        println!("Tokens:      {}", format_tokens(summary.total_tokens));
        println!("Cost:        ${:.4}", summary.total_cost_usd);
    }
}

fn cmd_epic(json_flag: bool, epic_id: Option<&str>) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let epics = match stats.per_epic(epic_id) {
        Ok(e) => e,
        Err(e) => error_exit(&format!("Failed to query epic stats: {}", e)),
    };

    if should_json(json_flag) {
        let data: Vec<serde_json::Value> = epics.iter().map(|e| json!({
            "epic_id": e.epic_id,
            "title": e.title,
            "status": e.status,
            "task_count": e.task_count,
            "done_count": e.done_count,
            "avg_duration_secs": e.avg_duration_secs,
            "total_tokens": e.total_tokens,
            "total_cost": e.total_cost,
        })).collect();
        json_output(json!({ "epics": data, "count": data.len() }));
    } else if epics.is_empty() {
        println!("No epic stats found.");
    } else {
        println!("{:<30} {:>6} {:>5}/{:>5} {:>10} {:>10}", "EPIC", "STATUS", "DONE", "TOTAL", "TOKENS", "COST");
        println!("{}", "-".repeat(75));
        for e in &epics {
            println!(
                "{:<30} {:>6} {:>5}/{:>5} {:>10} {:>10}",
                truncate(&e.epic_id, 30),
                e.status,
                e.done_count,
                e.task_count,
                format_tokens(e.total_tokens),
                format!("${:.4}", e.total_cost),
            );
        }
    }
}

fn cmd_weekly(json_flag: bool, weeks: u32) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let trends = match stats.weekly_trends(weeks) {
        Ok(t) => t,
        Err(e) => error_exit(&format!("Failed to query weekly trends: {}", e)),
    };

    if should_json(json_flag) {
        let data: Vec<serde_json::Value> = trends.iter().map(|t| json!({
            "week": t.week,
            "tasks_started": t.tasks_started,
            "tasks_completed": t.tasks_completed,
            "tasks_failed": t.tasks_failed,
        })).collect();
        json_output(json!({ "weekly_trends": data }));
    } else if trends.is_empty() {
        println!("No weekly trend data available.");
    } else {
        println!("{:<12} {:>8} {:>10} {:>8}", "WEEK", "STARTED", "COMPLETED", "FAILED");
        println!("{}", "-".repeat(42));
        for t in &trends {
            println!("{:<12} {:>8} {:>10} {:>8}", t.week, t.tasks_started, t.tasks_completed, t.tasks_failed);
        }
    }
}

fn cmd_tokens(json_flag: bool, epic_id: Option<&str>) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let tokens = match stats.token_breakdown(epic_id) {
        Ok(t) => t,
        Err(e) => error_exit(&format!("Failed to query token usage: {}", e)),
    };

    if should_json(json_flag) {
        let data: Vec<serde_json::Value> = tokens.iter().map(|t| json!({
            "epic_id": t.epic_id,
            "model": t.model,
            "input_tokens": t.input_tokens,
            "output_tokens": t.output_tokens,
            "cache_read": t.cache_read,
            "cache_write": t.cache_write,
            "estimated_cost": t.estimated_cost,
        })).collect();
        json_output(json!({ "token_usage": data }));
    } else if tokens.is_empty() {
        println!("No token usage data.");
    } else {
        println!("{:<25} {:<20} {:>10} {:>10} {:>10}", "EPIC", "MODEL", "INPUT", "OUTPUT", "COST");
        println!("{}", "-".repeat(80));
        for t in &tokens {
            println!(
                "{:<25} {:<20} {:>10} {:>10} {:>10}",
                truncate(&t.epic_id, 25),
                truncate(&t.model, 20),
                format_tokens(t.input_tokens),
                format_tokens(t.output_tokens),
                format!("${:.4}", t.estimated_cost),
            );
        }
    }
}

fn cmd_bottlenecks(json_flag: bool, limit: usize) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let bottlenecks = match stats.bottlenecks(limit) {
        Ok(b) => b,
        Err(e) => error_exit(&format!("Failed to query bottlenecks: {}", e)),
    };

    if should_json(json_flag) {
        let data: Vec<serde_json::Value> = bottlenecks.iter().map(|b| json!({
            "task_id": b.task_id,
            "epic_id": b.epic_id,
            "title": b.title,
            "duration_secs": b.duration_secs,
            "status": b.status,
            "blocked_reason": b.blocked_reason,
        })).collect();
        json_output(json!({ "bottlenecks": data }));
    } else if bottlenecks.is_empty() {
        println!("No bottleneck data.");
    } else {
        println!("{:<25} {:<10} {:>10} TITLE", "TASK", "STATUS", "DURATION");
        println!("{}", "-".repeat(70));
        for b in &bottlenecks {
            let duration = b.duration_secs
                .map(format_duration)
                .unwrap_or_else(|| "-".to_string());
            let suffix = b.blocked_reason.as_ref()
                .map(|r| format!(" [blocked: {}]", truncate(r, 30)))
                .unwrap_or_default();
            println!(
                "{:<25} {:<10} {:>10} {}{}",
                truncate(&b.task_id, 25),
                b.status,
                duration,
                truncate(&b.title, 30),
                suffix,
            );
        }
    }
}

fn cmd_dora(json_flag: bool) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    let dora = match stats.dora_metrics() {
        Ok(d) => d,
        Err(e) => error_exit(&format!("Failed to compute DORA metrics: {}", e)),
    };

    if should_json(json_flag) {
        json_output(json!({
            "lead_time_hours": dora.lead_time_hours,
            "throughput_per_week": dora.throughput_per_week,
            "change_failure_rate": dora.change_failure_rate,
            "time_to_restore_hours": dora.time_to_restore_hours,
        }));
    } else {
        println!("DORA Metrics (last 30 days)");
        println!("{}", "=".repeat(40));
        println!(
            "Lead Time:           {}",
            dora.lead_time_hours
                .map(|h| format!("{:.1}h", h))
                .unwrap_or_else(|| "N/A".to_string())
        );
        println!("Throughput:          {:.1} tasks/week", dora.throughput_per_week);
        println!("Change Failure Rate: {:.1}%", dora.change_failure_rate * 100.0);
        println!(
            "Time to Restore:     {}",
            dora.time_to_restore_hours
                .map(|h| format!("{:.1}h", h))
                .unwrap_or_else(|| "N/A".to_string())
        );
    }
}

fn cmd_rollup(json_flag: bool) {
    let conn = open_db_or_exit();
    let stats = crate::commands::db_shim::StatsQuery::new(&conn);

    match stats.generate_monthly_rollups() {
        Ok(count) => {
            if should_json(json_flag) {
                json_output(json!({ "months_updated": count }));
            } else {
                println!("Updated {} monthly rollup(s).", count);
            }
        }
        Err(e) => error_exit(&format!("Failed to generate rollups: {}", e)),
    }
}

fn cmd_cleanup(json_flag: bool) {
    let conn = open_db_or_exit();

    match crate::commands::db_shim::cleanup(&conn) {
        Ok(count) => {
            if should_json(json_flag) {
                json_output(json!({ "deleted": count }));
            } else {
                println!("Cleaned up {} old record(s).", count);
            }
        }
        Err(e) => error_exit(&format!("Cleanup failed: {}", e)),
    }
}

// ── DAG rendering ────────────────────────────────────────────────────

pub fn cmd_dag(json_flag: bool, epic_id: Option<String>) {
    let conn = open_db_or_exit();
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);

    // Find epic: use provided ID or find the first open epic
    let epic_id = match epic_id {
        Some(id) => id,
        None => {
            let epic_repo = crate::commands::db_shim::EpicRepo::new(&conn);
            match epic_repo.list(Some("open")) {
                Ok(epics) if !epics.is_empty() => epics[0].id.clone(),
                _ => error_exit("No open epic found. Use --epic <id> to specify."),
            }
        }
    };

    let tasks = match task_repo.list_by_epic(&epic_id) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => error_exit(&format!("No tasks found for epic {}", epic_id)),
        Err(e) => error_exit(&format!("Failed to load tasks: {}", e)),
    };

    let dag = match flowctl_core::TaskDag::from_tasks(&tasks) {
        Ok(d) => d,
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
    println!("DAG for {}", epic_id);
    println!();

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
            // Short ID: take just the task number suffix
            let short_id = task.id.rsplit('.').next().unwrap_or(&task.id);
            let label = format!(".{} [{}]", short_id, status_icon);
            let indent = "  ".repeat(layer);
            let connector = if layer > 0 { "\u{2514}\u{2500}\u{2500} " } else { "" };
            println!("{}{}\u{250c}\u{2500}{}\u{2500}\u{2510}", indent, connector, "\u{2500}".repeat(label.len()), );
            println!("{}{}\u{2502} {} \u{2502}", indent, if layer > 0 { "    " } else { "" }, label);
            println!("{}{}\u{2514}\u{2500}{}\u{2500}\u{2518}", indent, if layer > 0 { "    " } else { "" }, "\u{2500}".repeat(label.len()));
        }

        // Draw arrows between layers
        if layer < max_layer {
            let next_layer_nodes: Vec<&flowctl_core::types::Task> = tasks
                .iter()
                .filter(|t| layer_of.get(&t.id) == Some(&(layer + 1)))
                .collect();
            if !next_layer_nodes.is_empty() {
                let indent = "  ".repeat(layer + 1);
                println!("{}\u{2502}", indent);
                println!("{}\u{2193}", indent);
            }
        }
    }
}

// ── Estimate command ─────────────────────────────────────────────────

pub fn cmd_estimate(json_flag: bool, epic_id: &str) {
    let conn = open_db_or_exit();
    let task_repo = crate::commands::db_shim::TaskRepo::new(&conn);
    let runtime_repo = crate::commands::db_shim::RuntimeRepo::new(&conn);

    let tasks = match task_repo.list_by_epic(epic_id) {
        Ok(t) => t,
        Err(e) => error_exit(&format!("Failed to load tasks: {}", e)),
    };

    if tasks.is_empty() {
        error_exit(&format!("No tasks found for epic {}", epic_id));
    }

    // Collect durations from completed tasks
    let mut completed_durations: Vec<u64> = Vec::new();
    let mut incomplete_count = 0u32;

    for task in &tasks {
        if task.status == flowctl_core::Status::Done {
            if let Ok(Some(rt)) = runtime_repo.get(&task.id) {
                if let Some(dur) = rt.duration_secs {
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

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_duration(secs: i64) -> String {
    if secs >= 3600 {
        format!("{:.1}h", secs as f64 / 3600.0)
    } else if secs >= 60 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
