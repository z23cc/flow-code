//! Index commands: build, status, search.
//!
//! Manages a trigram inverted index for fast text search across the project.
//! The index is stored at `.flow/index/ngram.bin` (bincode binary format).

use clap::Subcommand;
use serde_json::json;

use flowctl_core::ngram_index::NgramIndex;

use crate::output::{error_exit, json_output, pretty_output};

// ── CLI definition ─────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum IndexCmd {
    /// Build or rebuild the full trigram index.
    Build,
    /// Show index statistics.
    Status,
    /// Search files using the trigram index (literal substring).
    Search {
        /// Text to search for.
        query: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Search files using the trigram index with a regex pattern.
    Regex {
        /// Regex pattern to search for.
        pattern: String,
        /// Maximum number of results (default: 20).
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

// ── Dispatch ───────────────────────────────────────────────────────

pub fn dispatch(cmd: &IndexCmd, json: bool) {
    match cmd {
        IndexCmd::Build => cmd_index_build(json),
        IndexCmd::Status => cmd_index_status(json),
        IndexCmd::Search { query, limit } => cmd_index_search(json, query, *limit),
        IndexCmd::Regex { pattern, limit } => cmd_index_regex(json, pattern, *limit),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Resolve the index file path: `.flow/index/ngram.bin`.
fn index_path() -> std::path::PathBuf {
    let flow_dir = super::helpers::get_flow_dir();
    // Try .bin first (new bincode format), fall back to .json (legacy)
    let bin_path = flow_dir.join("index").join("ngram.bin");
    if bin_path.exists() {
        return bin_path;
    }
    let json_path = flow_dir.join("index").join("ngram.json");
    if json_path.exists() {
        return json_path; // load() handles both formats
    }
    bin_path // default to .bin for new indexes
}

/// Path for saving new indexes (always .bin).
fn save_index_path() -> std::path::PathBuf {
    let flow_dir = super::helpers::get_flow_dir();
    flow_dir.join("index").join("ngram.bin")
}

/// Resolve the project root (parent of `.flow/`).
fn project_root() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|e| {
        error_exit(&format!("Cannot get current dir: {e}"));
    })
}

// ── Build ──────────────────────────────────────────────────────────

fn cmd_index_build(json: bool) {
    let root = project_root();
    let start = std::time::Instant::now();

    let idx = match NgramIndex::build(&root) {
        Ok(idx) => idx,
        Err(e) => error_exit(&format!("Failed to build index: {e}")),
    };

    let path = save_index_path();
    if let Err(e) = idx.save(&path) {
        error_exit(&format!("Failed to save index: {e}"));
    }

    let stats = idx.stats();
    let elapsed_ms = start.elapsed().as_millis();

    if json {
        json_output(json!({
            "action": "build",
            "file_count": stats.file_count,
            "trigram_count": stats.trigram_count,
            "index_size_bytes": stats.index_size_bytes,
            "elapsed_ms": elapsed_ms,
            "path": path.to_string_lossy(),
        }));
    } else {
        pretty_output(
            "index",
            &format!(
                "Index built: {} files, {} trigrams ({} bytes) in {}ms\nSaved to {}",
                stats.file_count,
                stats.trigram_count,
                stats.index_size_bytes,
                elapsed_ms,
                path.display()
            ),
        );
    }
}

// ── Status ─────────────────────────────────────────────────────────

fn cmd_index_status(json: bool) {
    let path = index_path();

    if !path.exists() {
        if json {
            json_output(json!({
                "exists": false,
                "hint": "Run `flowctl index build` to create the index",
            }));
        } else {
            pretty_output("index", "No index found. Run `flowctl index build` to create one.");
        }
        return;
    }

    let idx = match NgramIndex::load(&path) {
        Ok(idx) => idx,
        Err(e) => error_exit(&format!("Failed to load index: {e}")),
    };

    let stats = idx.stats();

    // Also get file size on disk
    let disk_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    if json {
        json_output(json!({
            "exists": true,
            "file_count": stats.file_count,
            "trigram_count": stats.trigram_count,
            "index_size_bytes": stats.index_size_bytes,
            "disk_size_bytes": disk_size,
            "built_at_epoch_ms": stats.built_at_epoch_ms,
            "path": path.to_string_lossy(),
        }));
    } else {
        pretty_output(
            "index",
            &format!(
                "Index: {} files, {} trigrams\nIn-memory: {} bytes, on-disk: {} bytes\nPath: {}",
                stats.file_count,
                stats.trigram_count,
                stats.index_size_bytes,
                disk_size,
                path.display()
            ),
        );
    }
}

// ── Search ─────────────────────────────────────────────────────────

fn cmd_index_search(json: bool, query: &str, limit: usize) {
    let path = index_path();

    if !path.exists() {
        error_exit("No index found. Run `flowctl index build` first.");
    }

    let idx = match NgramIndex::load(&path) {
        Ok(idx) => idx,
        Err(e) => error_exit(&format!("Failed to load index: {e}")),
    };

    let start = std::time::Instant::now();
    let results = idx.search(query, limit);
    let elapsed_ms = start.elapsed().as_millis();

    if json {
        let matches: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                json!({
                    "path": r.path.to_string_lossy(),
                    "match_count": r.match_count,
                })
            })
            .collect();
        json_output(json!({
            "query": query,
            "match_count": results.len(),
            "elapsed_ms": elapsed_ms,
            "matches": matches,
        }));
    } else {
        if results.is_empty() {
            pretty_output("index", &format!("No matches for \"{query}\" ({elapsed_ms}ms)"));
        } else {
            let mut out = format!("{} matches for \"{}\" ({}ms):\n", results.len(), query, elapsed_ms);
            for r in &results {
                out.push_str(&format!("  {} ({} hits)\n", r.path.display(), r.match_count));
            }
            pretty_output("index", &out);
        }
    }
}

// ── Regex search ──────────────────────────────────────────────────

fn cmd_index_regex(json: bool, pattern: &str, limit: usize) {
    let path = index_path();

    if !path.exists() {
        error_exit("No index found. Run `flowctl index build` first.");
    }

    let idx = match NgramIndex::load(&path) {
        Ok(idx) => idx,
        Err(e) => error_exit(&format!("Failed to load index: {e}")),
    };

    let start = std::time::Instant::now();
    let results = idx.search_regex(pattern, limit);
    let elapsed_ms = start.elapsed().as_millis();

    if json {
        let matches: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                json!({
                    "path": r.path.to_string_lossy(),
                    "match_count": r.match_count,
                })
            })
            .collect();
        json_output(json!({
            "pattern": pattern,
            "match_count": results.len(),
            "elapsed_ms": elapsed_ms,
            "matches": matches,
        }));
    } else {
        if results.is_empty() {
            pretty_output("index", &format!("No regex matches for /{pattern}/ ({elapsed_ms}ms)"));
        } else {
            let mut out = format!("{} regex matches for /{}/ ({}ms):\n", results.len(), pattern, elapsed_ms);
            for r in &results {
                out.push_str(&format!("  {} ({} hits)\n", r.path.display(), r.match_count));
            }
            pretty_output("index", &out);
        }
    }
}
