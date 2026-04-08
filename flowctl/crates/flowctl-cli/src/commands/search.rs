//! `flowctl search` — fuzzy file search with frecency boosting and git status filtering.

use serde_json::json;

use crate::output::{json_output, pretty_output};

use super::helpers::get_flow_dir;

use flowctl_core::frecency::FrecencyStore;
use flowctl_core::fuzzy;

/// Run fuzzy search and display results.
pub fn cmd_search(json: bool, query: String, git_filter: Option<String>, limit: usize) {
    let flow_dir = get_flow_dir();
    let frecency = if flow_dir.exists() {
        Some(FrecencyStore::load(&flow_dir))
    } else {
        None
    };

    let root = std::env::current_dir().unwrap_or_else(|e| {
        crate::output::error_exit(&format!("Cannot determine current directory: {e}"));
    });

    let results = fuzzy::search(
        &root,
        &query,
        git_filter.as_deref(),
        frecency.as_ref(),
        limit,
    );

    if json {
        let items: Vec<_> = results
            .iter()
            .map(|r| {
                json!({
                    "path": r.path,
                    "fuzzy_score": r.fuzzy_score,
                    "frecency_score": r.frecency_score,
                    "git_status": r.git_status,
                    "final_score": r.final_score,
                })
            })
            .collect();
        json_output(json!({
            "query": query,
            "count": items.len(),
            "results": items,
        }));
    } else {
        let mut text = String::new();
        if results.is_empty() {
            text.push_str(&format!("No matches for \"{query}\"\n"));
        } else {
            for r in &results {
                let status = r.git_status.as_deref().unwrap_or("");
                let status_tag = if status.is_empty() {
                    String::new()
                } else {
                    format!(" [{status}]")
                };
                text.push_str(&format!(
                    "{:>8.1}  {}{}\n",
                    r.final_score, r.path, status_tag,
                ));
            }
        }
        pretty_output("search", &text);
    }
}
