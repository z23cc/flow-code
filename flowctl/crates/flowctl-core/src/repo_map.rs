//! Repo map: build a reference graph of symbols across a codebase and rank
//! them by importance using PageRank. Generates a token-budgeted summary of
//! the most important symbols grouped by file.

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::path::Path;

use petgraph::graph::{DiGraph, NodeIndex};
use serde::Serialize;

use crate::code_structure::{self, StructureError, Symbol};

// ── Types ───────────────────────────────────────────────────────────

/// A ranked symbol with its PageRank score.
#[derive(Debug, Clone, Serialize)]
pub struct RankedSymbol {
    pub symbol: Symbol,
    pub rank: f64,
}

// ── Reference graph ─────────────────────────────────────────────────

/// Build a reference graph: for each file, find identifiers that match
/// symbol names defined in other files. Create edges (referrer -> definer).
pub fn build_reference_graph(symbols: &[Symbol], root: &Path) -> DiGraph<String, ()> {
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    // Collect unique files.
    let mut files: Vec<String> = symbols.iter().map(|s| s.file.clone()).collect();
    files.sort();
    files.dedup();

    // Create a node per file.
    for file in &files {
        let idx = graph.add_node(file.clone());
        node_map.insert(file.clone(), idx);
    }

    // Build a map: symbol name -> list of defining files.
    let mut name_to_files: HashMap<&str, Vec<&str>> = HashMap::new();
    for sym in symbols {
        name_to_files.entry(&sym.name).or_default().push(&sym.file);
    }

    // For each file, scan its content for references to symbols defined elsewhere.
    for file in &files {
        let full_path = if Path::new(file).is_absolute() {
            std::path::PathBuf::from(file)
        } else {
            root.join(file)
        };

        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let src_node = match node_map.get(file.as_str()) {
            Some(n) => *n,
            None => continue,
        };

        for (name, def_files) in &name_to_files {
            // Skip very short names (likely false positives).
            if name.len() < 3 {
                continue;
            }
            // Check if this name appears in the file content.
            if content.contains(name) {
                for def_file in def_files {
                    if *def_file == file.as_str() {
                        continue; // skip self-references
                    }
                    if let Some(&dst_node) = node_map.get(*def_file) {
                        // Edge: this file references a symbol in def_file.
                        graph.add_edge(src_node, dst_node, ());
                    }
                }
            }
        }
    }

    graph
}

/// Apply PageRank to the reference graph to identify most important files,
/// then rank symbols by their file's PageRank score.
pub fn rank_symbols(symbols: &[Symbol], graph: &DiGraph<String, ()>) -> Vec<RankedSymbol> {
    let node_count = graph.node_count();
    if node_count == 0 {
        return symbols
            .iter()
            .map(|s| RankedSymbol {
                symbol: s.clone(),
                rank: 1.0,
            })
            .collect();
    }

    // Simple PageRank: iterate until convergence.
    let damping = 0.85;
    let iterations = 20;
    let initial = 1.0 / node_count as f64;

    let mut ranks: Vec<f64> = vec![initial; node_count];
    let mut new_ranks: Vec<f64> = vec![0.0; node_count];

    for _ in 0..iterations {
        let base = (1.0 - damping) / node_count as f64;
        for r in &mut new_ranks {
            *r = base;
        }

        for node_idx in graph.node_indices() {
            let out_degree = graph.neighbors(node_idx).count();
            if out_degree == 0 {
                // Distribute rank equally to all nodes (dangling node).
                let share = ranks[node_idx.index()] / node_count as f64;
                for r in &mut new_ranks {
                    *r += damping * share;
                }
            } else {
                let share = ranks[node_idx.index()] / out_degree as f64;
                for neighbor in graph.neighbors(node_idx) {
                    new_ranks[neighbor.index()] += damping * share;
                }
            }
        }

        std::mem::swap(&mut ranks, &mut new_ranks);
    }

    // Map file names to their ranks.
    let mut file_ranks: HashMap<String, f64> = HashMap::new();
    for node_idx in graph.node_indices() {
        let file = &graph[node_idx];
        file_ranks.insert(file.clone(), ranks[node_idx.index()]);
    }

    // Assign each symbol the rank of its file.
    let mut ranked: Vec<RankedSymbol> = symbols
        .iter()
        .map(|s| {
            let rank = file_ranks.get(&s.file).copied().unwrap_or(initial);
            RankedSymbol {
                symbol: s.clone(),
                rank,
            }
        })
        .collect();

    // Sort by rank descending, then by file+line for stability.
    ranked.sort_by(|a, b| {
        b.rank
            .partial_cmp(&a.rank)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.file.cmp(&b.symbol.file))
            .then_with(|| a.symbol.line.cmp(&b.symbol.line))
    });

    ranked
}

/// Rough token estimate: ~4 chars per token (conservative).
fn estimate_tokens(s: &str) -> usize {
    (s.len() + 3) / 4
}

/// Generate a repo map string within a token budget.
/// Lists the top-ranked symbols (signatures only) grouped by file.
pub fn generate_repo_map(root: &Path, token_budget: usize) -> Result<String, StructureError> {
    let symbols = code_structure::extract_all_symbols(root)?;
    if symbols.is_empty() {
        return Ok(String::from("(no symbols found)"));
    }

    let graph = build_reference_graph(&symbols, root);
    let ranked = rank_symbols(&symbols, &graph);

    // Make file paths relative to root for display.
    let root_str = root.display().to_string();

    let mut output = String::new();
    let mut current_file = String::new();
    let mut tokens_used: usize = 0;

    for rs in &ranked {
        let display_file = if rs.symbol.file.starts_with(&root_str) {
            rs.symbol.file[root_str.len()..]
                .trim_start_matches('/')
                .to_string()
        } else {
            rs.symbol.file.clone()
        };

        // Check if adding a new file header + this symbol fits.
        let file_header_cost = if display_file != current_file {
            estimate_tokens(&format!("{display_file}:\n"))
        } else {
            0
        };
        let sig_cost = estimate_tokens(&format!("  {}\n", rs.symbol.signature));

        if token_budget > 0 && tokens_used + file_header_cost + sig_cost > token_budget {
            break;
        }

        if display_file != current_file {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("{display_file}:\n"));
            tokens_used += file_header_cost;
            current_file = display_file;
        }

        output.push_str(&format!("  {}\n", rs.symbol.signature));
        tokens_used += sig_cost;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        // auth.rs - defines authenticate and User
        let auth = dir.path().join("auth.rs");
        let mut f = std::fs::File::create(&auth).unwrap();
        write!(
            f,
            r#"pub fn authenticate(token: &str) -> Result<User> {{
    let user = query_user(42);
    Ok(user.unwrap())
}}

pub struct User {{
    pub id: u64,
    pub email: String,
}}
"#
        )
        .unwrap();

        // db.rs - defines query_user, references User
        let db = dir.path().join("db.rs");
        let mut f = std::fs::File::create(&db).unwrap();
        write!(
            f,
            r#"use crate::auth::User;

pub fn query_user(id: u64) -> Option<User> {{
    None
}}
"#
        )
        .unwrap();

        dir
    }

    #[test]
    fn test_build_reference_graph() {
        let dir = setup_test_dir();
        let symbols = code_structure::extract_all_symbols(dir.path()).unwrap();
        assert!(!symbols.is_empty());

        let graph = build_reference_graph(&symbols, dir.path());
        // Should have edges: auth.rs -> db.rs (references query_user)
        // and db.rs -> auth.rs (references User)
        assert!(graph.edge_count() > 0);
    }

    #[test]
    fn test_rank_symbols() {
        let dir = setup_test_dir();
        let symbols = code_structure::extract_all_symbols(dir.path()).unwrap();
        let graph = build_reference_graph(&symbols, dir.path());
        let ranked = rank_symbols(&symbols, &graph);
        assert!(!ranked.is_empty());
        // All ranks should be positive.
        for rs in &ranked {
            assert!(rs.rank > 0.0);
        }
    }

    #[test]
    fn test_generate_repo_map() {
        let dir = setup_test_dir();
        let map = generate_repo_map(dir.path(), 1024).unwrap();
        assert!(!map.is_empty());
        // Should contain the function names.
        assert!(map.contains("authenticate") || map.contains("query_user"));
    }

    #[test]
    fn test_generate_repo_map_budget() {
        let dir = setup_test_dir();
        // Very small budget should still produce some output.
        let map = generate_repo_map(dir.path(), 50).unwrap();
        // May be truncated but should not panic.
        assert!(!map.is_empty() || map == "(no symbols found)");
    }
}
