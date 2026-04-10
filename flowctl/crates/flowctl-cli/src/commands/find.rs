//! `flowctl find` — Smart code search: auto-routes to the best search backend.
//!
//! Routing logic:
//! - If query looks like regex (contains \s \d \w .* .+ [^ etc) -> index regex
//! - If query matches a known symbol name (in graph) -> graph refs
//! - If query is short (<3 chars) -> brute force fuzzy search
//! - Otherwise -> index search (trigram literal), falling back to fuzzy search

use serde_json::json;

use flowctl_core::frecency::FrecencyStore;
use flowctl_core::fuzzy;
use flowctl_core::graph_store::CodeGraph;
use flowctl_core::ngram_index::NgramIndex;

use crate::output::{json_output, pretty_output};

use super::helpers::get_flow_dir;

/// Detect whether the query looks like a regex pattern.
fn is_regex_pattern(query: &str) -> bool {
    query.contains("\\s")
        || query.contains("\\d")
        || query.contains("\\w")
        || query.contains(".*")
        || query.contains(".+")
        || query.contains("[^")
        || query.contains('(')
        || query.contains('|')
}

/// Run smart code search and display results.
pub fn cmd_find(json: bool, query: &str, limit: usize) {
    let flow_dir = get_flow_dir();

    // ── Strategy 1: Regex route ─────────────────────────────────────
    if is_regex_pattern(query) {
        let index_path = flow_dir.join("index").join("ngram.bin");
        if index_path.exists() {
            if let Ok(idx) = NgramIndex::load(&index_path) {
                let results = idx.search_regex(query, limit);
                if !results.is_empty() {
                    output_index_results(json, query, "index_regex", &results);
                    return;
                }
            }
        }
        // Regex but no index — fall through to fuzzy
        output_fuzzy_fallback(json, query, limit);
        return;
    }

    // ── Strategy 2: Graph symbol lookup ─────────────────────────────
    let graph_path = flow_dir.join("graph.bin");
    if graph_path.exists() {
        if let Ok(graph) = CodeGraph::load(&graph_path) {
            let refs = graph.find_refs(query);
            if !refs.is_empty() {
                if json {
                    let items: Vec<_> = refs
                        .iter()
                        .map(|s| {
                            json!({
                                "path": s.file,
                                "line": s.line,
                                "kind": s.kind,
                                "context": s.signature,
                            })
                        })
                        .collect();
                    json_output(json!({
                        "query": query,
                        "backend": "graph_refs",
                        "count": items.len(),
                        "results": items,
                    }));
                } else {
                    let mut out = format!("{} symbol references for \"{}\":\n", refs.len(), query);
                    for r in &refs {
                        out.push_str(&format!(
                            "  {}:{} {} ({})\n",
                            r.file, r.line, r.name, r.kind
                        ));
                    }
                    pretty_output("find", &out);
                }
                return;
            }
        }
    }

    // ── Strategy 3: Short queries → skip trigram (needs 3+ chars) ───
    if query.len() < 3 {
        output_fuzzy_fallback(json, query, limit);
        return;
    }

    // ── Strategy 4: Trigram index literal search ────────────────────
    let index_path = flow_dir.join("index").join("ngram.bin");
    if index_path.exists() {
        if let Ok(idx) = NgramIndex::load(&index_path) {
            let results = idx.search(query, limit);
            if !results.is_empty() {
                output_index_results(json, query, "index_literal", &results);
                return;
            }
        }
    }

    // ── Strategy 5: Fuzzy fallback ──────────────────────────────────
    output_fuzzy_fallback(json, query, limit);
}

/// Output results from the trigram index (literal or regex).
fn output_index_results(
    json_flag: bool,
    query: &str,
    backend: &str,
    results: &[flowctl_core::ngram_index::NgramSearchResult],
) {
    if json_flag {
        let items: Vec<_> = results
            .iter()
            .map(|r| {
                json!({
                    "path": r.path.to_string_lossy(),
                    "line": null,
                    "kind": "match",
                    "context": format!("{} hits", r.match_count),
                })
            })
            .collect();
        json_output(json!({
            "query": query,
            "backend": backend,
            "count": items.len(),
            "results": items,
        }));
    } else {
        let mut out = format!(
            "{} matches for \"{}\" (backend: {}):\n",
            results.len(),
            query,
            backend
        );
        for r in results {
            out.push_str(&format!(
                "  {} ({} hits)\n",
                r.path.display(),
                r.match_count
            ));
        }
        pretty_output("find", &out);
    }
}

/// Fuzzy file search fallback.
fn output_fuzzy_fallback(json_flag: bool, query: &str, limit: usize) {
    let flow_dir = get_flow_dir();
    let frecency = if flow_dir.exists() {
        Some(FrecencyStore::load(&flow_dir))
    } else {
        None
    };

    let root = std::env::current_dir().unwrap_or_else(|e| {
        crate::output::error_exit(&format!("Cannot determine current directory: {e}"));
    });

    let results = fuzzy::search(&root, query, None, frecency.as_ref(), limit);

    if json_flag {
        let items: Vec<_> = results
            .iter()
            .map(|r| {
                json!({
                    "path": r.path,
                    "line": null,
                    "kind": "fuzzy",
                    "context": format!("score: {:.1}", r.final_score),
                })
            })
            .collect();
        json_output(json!({
            "query": query,
            "backend": "fuzzy",
            "count": items.len(),
            "results": items,
        }));
    } else if results.is_empty() {
        pretty_output("find", &format!("No matches for \"{query}\"\n"));
    } else {
        let mut out = format!("{} fuzzy matches for \"{}\":\n", results.len(), query);
        for r in &results {
            out.push_str(&format!("  {:>8.1}  {}\n", r.final_score, r.path));
        }
        pretty_output("find", &out);
    }
}
