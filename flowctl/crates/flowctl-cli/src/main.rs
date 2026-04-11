//! flowctl CLI — minimal entry point for V4.
//!
//! Only 4 commands: serve, init, guard, version.
//! All workflow interaction happens through the MCP server (3 tools).

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use clap::Parser;

/// flowctl — goal-driven development engine.
#[derive(Parser, Debug)]
#[command(
    name = "flowctl",
    version,
    about = "Goal-driven development engine",
    long_about = "V4 engine-driven protocol. Start the MCP server with `flowctl serve`, \
        then use flow_drive/flow_submit/flow_query tools from Claude Code or Codex."
)]
struct Cli {
    /// Project root directory (overrides CWD for .flow/ resolution).
    #[arg(long = "project-dir", short = 'C', global = true)]
    project_dir: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Start MCP server on stdio.
    Serve,

    /// Initialize .flow/ directory for a project.
    Init,

    /// Run quality guard checks (lint, type, test).
    Guard {
        /// Guard depth: trivial, standard, thorough.
        #[arg(long, default_value = "standard")]
        depth: String,

        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let root = resolve_root(cli.project_dir.as_deref());

    match cli.command {
        Commands::Serve => cmd_serve(root),
        Commands::Init => cmd_init(&root),
        Commands::Guard { depth, json } => cmd_guard(&root, &depth, json),
    }
}

fn resolve_root(project_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = project_dir {
        PathBuf::from(dir)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

// ── Commands ────────────────────────────────────────────────────────

fn cmd_serve(root: PathBuf) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(async {
        if let Err(e) = flowctl_mcp::run_server(root).await {
            eprintln!("MCP server error: {e}");
            std::process::exit(1);
        }
    });
}

fn cmd_init(root: &Path) {
    let flow_dir = root.join(".flow");

    print!("Building code graph...");
    match flowctl_core::graph_store::CodeGraph::build(root) {
        Ok(graph) => {
            let graph_path = flow_dir.join("graph.bin");
            if let Err(e) = graph.save(&graph_path) {
                eprintln!(" failed to save: {e}");
            } else {
                let stats = graph.stats();
                println!(" {} symbols, {} files, {} edges", stats.symbol_count, stats.file_count, stats.edge_count);
            }
        }
        Err(e) => println!(" skipped ({e})"),
    }
}

fn cmd_guard(root: &Path, depth: &str, json: bool) {
    use flowctl_core::domain::node::GuardDepth;
    use flowctl_core::quality::guard_runner::GuardRunner;

    let guard = GuardRunner::new(root);
    let depth_enum = match depth {
        "trivial" => GuardDepth::Trivial,
        "thorough" => GuardDepth::Thorough,
        _ => GuardDepth::Standard,
    };

    let result = guard.run(depth_enum);

    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        let icon = if result.passed { "PASS" } else { "FAIL" };
        println!("[{icon}] Guard ({depth})");
        for r in &result.results {
            let status = if r.passed { "ok" } else { "FAIL" };
            println!("  [{status}] {}", r.command);
            if !r.passed {
                if !r.stdout.is_empty() {
                    for line in r.stdout.lines().take(10) {
                        println!("    {line}");
                    }
                }
                if !r.stderr.is_empty() {
                    for line in r.stderr.lines().take(10) {
                        println!("    {line}");
                    }
                }
            }
        }
    }

    if !result.passed {
        std::process::exit(1);
    }
}
