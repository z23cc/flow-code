//! `flowctl project-context` — display parsed project context metadata.

use clap::Subcommand;
use serde_json::json;

use crate::commands::helpers::get_flow_dir;
use crate::output::{error_exit, json_output};
use flowctl_core::project_context::ProjectContext;

#[derive(Subcommand, Debug)]
pub enum ProjectContextCmd {
    /// Show the full parsed project context.
    Show,
    /// Infer the domain for a file path based on file conventions.
    Domain {
        /// File path to classify.
        path: String,
    },
}

pub fn dispatch(cmd: &ProjectContextCmd, json: bool) {
    match cmd {
        ProjectContextCmd::Show => cmd_show(json),
        ProjectContextCmd::Domain { path } => cmd_domain(json, path),
    }
}

fn cmd_show(json_flag: bool) {
    let flow_dir = get_flow_dir();
    let ctx = match ProjectContext::load(&flow_dir) {
        Some(c) => c,
        None => {
            if json_flag {
                json_output(json!({
                    "found": false,
                    "message": "No project-context.md found in .flow/"
                }));
                return;
            } else {
                error_exit("No project-context.md found in .flow/");
            }
        }
    };

    if json_flag {
        json_output(json!({
            "found": true,
            "technology_stack": ctx.technology_stack,
            "guard_commands": {
                "test": ctx.guard_commands.test,
                "lint": ctx.guard_commands.lint,
                "typecheck": ctx.guard_commands.typecheck,
                "format_check": ctx.guard_commands.format_check,
            },
            "critical_rules": ctx.critical_rules,
            "file_conventions": ctx.file_conventions,
            "architecture_decisions": ctx.architecture_decisions,
            "non_goals": ctx.non_goals,
        }));
    } else {
        println!("=== Project Context ===\n");

        if !ctx.technology_stack.is_empty() {
            println!("Technology Stack:");
            for item in &ctx.technology_stack {
                println!("  - {item}");
            }
            println!();
        }

        let gc = &ctx.guard_commands;
        println!("Guard Commands:");
        println!("  test:         {}", gc.test.as_deref().unwrap_or("(not set)"));
        println!("  lint:         {}", gc.lint.as_deref().unwrap_or("(not set)"));
        println!("  typecheck:    {}", gc.typecheck.as_deref().unwrap_or("(not set)"));
        println!("  format_check: {}", gc.format_check.as_deref().unwrap_or("(not set)"));
        println!();

        if !ctx.critical_rules.is_empty() {
            println!("Critical Rules:");
            for item in &ctx.critical_rules {
                println!("  - {item}");
            }
            println!();
        }

        if !ctx.file_conventions.is_empty() {
            println!("File Conventions:");
            for (domain, patterns) in &ctx.file_conventions {
                println!("  {domain}: {}", patterns.join(", "));
            }
            println!();
        }

        if !ctx.architecture_decisions.is_empty() {
            println!("Architecture Decisions:");
            for item in &ctx.architecture_decisions {
                println!("  - {item}");
            }
            println!();
        }

        if !ctx.non_goals.is_empty() {
            println!("Non-Goals:");
            for item in &ctx.non_goals {
                println!("  - {item}");
            }
        }
    }
}

fn cmd_domain(json_flag: bool, path: &str) {
    let flow_dir = get_flow_dir();
    let ctx = match ProjectContext::load(&flow_dir) {
        Some(c) => c,
        None => {
            if json_flag {
                json_output(json!({
                    "path": path,
                    "domain": null,
                    "error": "No project-context.md found"
                }));
                return;
            } else {
                error_exit("No project-context.md found in .flow/");
            }
        }
    };

    let domain = ctx.infer_domain(path);

    if json_flag {
        json_output(json!({
            "path": path,
            "domain": domain,
        }));
    } else {
        match domain {
            Some(d) => println!("{path} -> {d}"),
            None => println!("{path} -> (no matching domain)"),
        }
    }
}
