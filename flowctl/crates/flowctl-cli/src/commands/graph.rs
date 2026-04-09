//! `flowctl graph` commands: build, update, status, refs, impact, map.
//!
//! Manages a persistent code graph stored at `.flow/graph.bin`.

use clap::Subcommand;
use serde_json::json;

use flowctl_core::graph_store::CodeGraph;

use crate::output::{error_exit, json_output, pretty_output};

// ── CLI definition ─────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum GraphCmd {
    /// Build the code graph from scratch.
    Build,
    /// Incrementally update changed files (uses git diff HEAD~1).
    Update,
    /// Show graph statistics.
    Status,
    /// Find all references to a symbol.
    Refs {
        /// Symbol name to find references for.
        symbol: String,
    },
    /// Analyze impact of changing a file.
    Impact {
        /// File path to analyze.
        path: String,
    },
    /// Output ranked repo map from cached graph.
    Map {
        /// Token budget (0 = unlimited).
        #[arg(long, default_value = "0")]
        budget: usize,
    },
}

// ── Dispatch ───────────────────────────────────────────────────────

pub fn dispatch(cmd: &GraphCmd, json: bool) {
    match cmd {
        GraphCmd::Build => cmd_build(json),
        GraphCmd::Update => cmd_update(json),
        GraphCmd::Status => cmd_status(json),
        GraphCmd::Refs { symbol } => cmd_refs(json, symbol),
        GraphCmd::Impact { path } => cmd_impact(json, path),
        GraphCmd::Map { budget } => cmd_map(json, *budget),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn graph_path() -> std::path::PathBuf {
    let flow_dir = super::helpers::get_flow_dir();
    flow_dir.join("graph.bin")
}

fn project_root() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|e| {
        error_exit(&format!("Cannot get current dir: {e}"));
    })
}

fn load_graph() -> CodeGraph {
    let path = graph_path();
    if !path.exists() {
        error_exit("No graph found. Run `flowctl graph build` first.");
    }
    match CodeGraph::load(&path) {
        Ok(g) => g,
        Err(e) => error_exit(&format!("Failed to load graph: {e}")),
    }
}

// ── Build ──────────────────────────────────────────────────────────

fn cmd_build(json: bool) {
    let root = project_root();
    let start = std::time::Instant::now();

    let graph = match CodeGraph::build(&root) {
        Ok(g) => g,
        Err(e) => error_exit(&format!("Failed to build graph: {e}")),
    };

    let path = graph_path();
    if let Err(e) = graph.save(&path) {
        error_exit(&format!("Failed to save graph: {e}"));
    }

    let stats = graph.stats();
    let elapsed_ms = start.elapsed().as_millis();

    if json {
        json_output(json!({
            "action": "build",
            "symbol_count": stats.symbol_count,
            "file_count": stats.file_count,
            "edge_count": stats.edge_count,
            "elapsed_ms": elapsed_ms,
            "path": path.to_string_lossy(),
        }));
    } else {
        pretty_output(
            "graph",
            &format!(
                "Graph built: {} symbols, {} files, {} edges in {}ms\nSaved to {}",
                stats.symbol_count,
                stats.file_count,
                stats.edge_count,
                elapsed_ms,
                path.display()
            ),
        );
    }
}

// ── Update ─────────────────────────────────────────────────────────

fn cmd_update(json: bool) {
    let root = project_root();
    let start = std::time::Instant::now();

    // Get changed files from git.
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD~1"])
        .current_dir(&root)
        .output();

    let changed_files: Vec<String> = match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| {
                    let p = root.join(l);
                    p.display().to_string()
                })
                .collect()
        }
        _ => {
            error_exit("Failed to run git diff. Is this a git repository with at least 2 commits?");
        }
    };

    if changed_files.is_empty() {
        if json {
            json_output(json!({
                "action": "update",
                "changed_files": 0,
                "message": "No changed files detected",
            }));
        } else {
            pretty_output("graph", "No changed files detected.");
        }
        return;
    }

    let mut graph = load_graph();

    if let Err(e) = graph.update(&root, &changed_files) {
        error_exit(&format!("Failed to update graph: {e}"));
    }

    let path = graph_path();
    if let Err(e) = graph.save(&path) {
        error_exit(&format!("Failed to save graph: {e}"));
    }

    let stats = graph.stats();
    let elapsed_ms = start.elapsed().as_millis();

    if json {
        json_output(json!({
            "action": "update",
            "changed_files": changed_files.len(),
            "symbol_count": stats.symbol_count,
            "file_count": stats.file_count,
            "edge_count": stats.edge_count,
            "elapsed_ms": elapsed_ms,
        }));
    } else {
        pretty_output(
            "graph",
            &format!(
                "Graph updated: {} changed files, {} symbols, {} files, {} edges in {}ms",
                changed_files.len(),
                stats.symbol_count,
                stats.file_count,
                stats.edge_count,
                elapsed_ms,
            ),
        );
    }
}

// ── Status ─────────────────────────────────────────────────────────

fn cmd_status(json: bool) {
    let path = graph_path();

    if !path.exists() {
        if json {
            json_output(json!({
                "exists": false,
                "hint": "Run `flowctl graph build` to create the graph",
            }));
        } else {
            pretty_output("graph", "No graph found. Run `flowctl graph build` to create one.");
        }
        return;
    }

    let graph = load_graph();
    let stats = graph.stats();
    let disk_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    if json {
        json_output(json!({
            "exists": true,
            "symbol_count": stats.symbol_count,
            "file_count": stats.file_count,
            "edge_count": stats.edge_count,
            "disk_size_bytes": disk_size,
            "built_at_epoch_ms": stats.built_at_epoch_ms,
            "path": path.to_string_lossy(),
        }));
    } else {
        pretty_output(
            "graph",
            &format!(
                "Graph: {} symbols, {} files, {} edges\nOn-disk: {} bytes\nBuilt at: {}\nPath: {}",
                stats.symbol_count,
                stats.file_count,
                stats.edge_count,
                disk_size,
                stats.built_at_epoch_ms,
                path.display()
            ),
        );
    }
}

// ── Refs ───────────────────────────────────────────────────────────

fn cmd_refs(json: bool, symbol: &str) {
    let graph = load_graph();
    let refs = graph.find_refs(symbol);

    if json {
        let entries: Vec<serde_json::Value> = refs
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "kind": s.kind,
                    "file": s.file,
                    "line": s.line,
                    "signature": s.signature,
                })
            })
            .collect();
        json_output(json!({
            "symbol": symbol,
            "ref_count": entries.len(),
            "refs": entries,
        }));
    } else if refs.is_empty() {
        pretty_output("graph", &format!("No references found for \"{symbol}\""));
    } else {
        let mut out = format!("{} references to \"{}\":\n", refs.len(), symbol);
        for r in &refs {
            out.push_str(&format!("  {}:{} {} ({})\n", r.file, r.line, r.name, r.kind));
        }
        pretty_output("graph", &out);
    }
}

// ── Impact ─────────────────────────────────────────────────────────

fn cmd_impact(json: bool, path: &str) {
    let graph = load_graph();
    let root = project_root();

    // Resolve to absolute path for matching.
    let abs_path = if std::path::Path::new(path).is_absolute() {
        path.to_string()
    } else {
        root.join(path).display().to_string()
    };

    let impact = graph.find_impact(&abs_path);

    if json {
        json_output(json!({
            "file": path,
            "impact_count": impact.len(),
            "impacted_files": impact,
        }));
    } else if impact.is_empty() {
        pretty_output("graph", &format!("No impact detected for \"{path}\""));
    } else {
        let mut out = format!("{} files impacted by changes to \"{}\":\n", impact.len(), path);
        for f in &impact {
            out.push_str(&format!("  {f}\n"));
        }
        pretty_output("graph", &out);
    }
}

// ── Map ────────────────────────────────────────────────────────────

fn cmd_map(json: bool, budget: usize) {
    let graph = load_graph();
    let map = graph.repo_map(budget);

    if json {
        json_output(json!({
            "map": map,
            "symbol_count": graph.stats().symbol_count,
            "budget": budget,
        }));
    } else {
        pretty_output("graph", &map);
    }
}
