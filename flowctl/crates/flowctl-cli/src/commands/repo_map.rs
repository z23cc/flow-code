//! `flowctl repo-map` command: generate a ranked repo map within a token budget.

use serde_json::json;

use crate::output::{error_exit, json_output, pretty_output};

pub fn cmd_repo_map(json_mode: bool, budget: usize, path: &str) {
    let root = std::path::Path::new(path);
    if !root.is_dir() {
        error_exit(&format!("Not a directory: {path}"));
    }

    // Generate the ranked map.
    let map = match flowctl_core::repo_map::generate_repo_map(root, budget) {
        Ok(m) => m,
        Err(e) => error_exit(&format!("Repo map generation failed: {e}")),
    };

    if json_mode {
        // Also extract ranked symbols for structured output.
        let symbols = match flowctl_core::code_structure::extract_all_symbols(root) {
            Ok(s) => s,
            Err(e) => error_exit(&format!("Symbol extraction failed: {e}")),
        };
        let graph = flowctl_core::repo_map::build_reference_graph(&symbols, root);
        let ranked = flowctl_core::repo_map::rank_symbols(&symbols, &graph);

        let items: Vec<serde_json::Value> = ranked
            .iter()
            .map(|rs| {
                json!({
                    "name": rs.symbol.name,
                    "kind": rs.symbol.kind,
                    "file": rs.symbol.file,
                    "line": rs.symbol.line,
                    "signature": rs.symbol.signature,
                    "rank": rs.rank,
                })
            })
            .collect();

        json_output(json!({
            "map": map,
            "ranked_symbols": items,
            "count": items.len(),
            "budget": budget,
        }));
    } else {
        pretty_output("repo_map", &map);
    }
}
