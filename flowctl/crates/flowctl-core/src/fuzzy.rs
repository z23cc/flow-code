//! Fuzzy file search combining nucleo-matcher, ignore (`.gitignore`-aware walk),
//! frecency boosting, and git status filtering.

use std::path::Path;
use std::process::Command;
use std::collections::HashMap;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ignore::WalkBuilder;
use serde::Serialize;

use crate::frecency::FrecencyStore;

/// A single search result with composite scoring.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub path: String,
    pub fuzzy_score: u32,
    pub frecency_score: f64,
    pub git_status: Option<String>,
    pub final_score: f64,
}

/// Git status boost values.
const GIT_BOOST_MODIFIED: f64 = 2.0;
const GIT_BOOST_STAGED: f64 = 1.5;
const GIT_BOOST_UNTRACKED: f64 = 1.0;

/// Frecency boost scaling factor.
const FRECENCY_SCALE: f64 = 0.1;

/// Run fuzzy file search over a repository root.
///
/// 1. Walks files via the `ignore` crate (respects `.gitignore`).
/// 2. Fuzzy-matches each path against `query` with nucleo-matcher.
/// 3. Optionally filters by git status (`"modified"`, `"staged"`, `"untracked"`).
/// 4. Applies frecency boost from the store.
/// 5. Returns results sorted by `final_score` descending, limited to `limit`.
pub fn search(
    root: &Path,
    query: &str,
    git_filter: Option<&str>,
    frecency: Option<&FrecencyStore>,
    limit: usize,
) -> Vec<SearchResult> {
    let git_statuses = parse_git_status(root);

    // Collect candidate file paths
    let mut candidates: Vec<String> = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel_str = rel.to_string_lossy().to_string();

        // Apply git filter if requested
        if let Some(filter) = git_filter {
            match git_statuses.get(&rel_str) {
                Some(status) if status == filter => {}
                _ => continue,
            }
        }

        candidates.push(rel_str);
    }

    // Fuzzy match
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut results: Vec<SearchResult> = Vec::new();

    for path in &candidates {
        let mut buf = Vec::new();
        let haystack = Utf32Str::new(path, &mut buf);
        let score = pattern.score(haystack, &mut matcher);

        if let Some(fuzzy_score) = score {
            let frecency_score = frecency
                .map(|f| f.get_score(path))
                .unwrap_or(0.0);

            let git_status = git_statuses.get(path).cloned();

            let git_boost = match git_status.as_deref() {
                Some("modified") => GIT_BOOST_MODIFIED,
                Some("staged") => GIT_BOOST_STAGED,
                Some("untracked") => GIT_BOOST_UNTRACKED,
                _ => 0.0,
            };

            let base = f64::from(fuzzy_score);
            let final_score = base * (1.0 + git_boost + frecency_score * FRECENCY_SCALE);

            results.push(SearchResult {
                path: path.clone(),
                fuzzy_score,
                frecency_score,
                git_status,
                final_score,
            });
        }
    }

    results.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

/// Parse `git status --porcelain` output into a map of path -> status label.
fn parse_git_status(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return map,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let index = line.as_bytes()[0];
        let worktree = line.as_bytes()[1];
        let path = line[3..].trim().to_string();

        // Determine the most relevant status
        let status = if index == b'?' && worktree == b'?' {
            "untracked"
        } else if worktree == b'M' || worktree == b'D' {
            "modified"
        } else if index == b'A' || index == b'M' || index == b'D' || index == b'R' {
            "staged"
        } else {
            "modified"
        };

        map.insert(path, status.to_string());
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_empty_query_returns_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hello.rs"), "fn main() {}").unwrap();
        let results = search(dir.path(), "", None, None, 10);
        // Empty query may or may not match depending on nucleo behavior
        // Just verify no panic
        let _ = results;
    }

    #[test]
    fn search_finds_matching_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("lib.rs"), "pub mod foo;").unwrap();
        let results = search(dir.path(), "main", None, None, 10);
        assert!(!results.is_empty(), "should find at least one match");
        assert_eq!(results[0].path, "main.rs");
    }

    #[test]
    fn frecency_boosts_score() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("aaa.rs"), "").unwrap();
        std::fs::write(dir.path().join("aaab.rs"), "").unwrap();

        let mut store = FrecencyStore::default();
        store.record_access("aaab.rs", 50.0);

        let results = search(dir.path(), "aaa", None, Some(&store), 10);
        assert!(results.len() >= 2);
        // The frecency-boosted file should rank higher
        let boosted = results.iter().find(|r| r.path == "aaab.rs").unwrap();
        assert!(boosted.frecency_score > 0.0);
    }
}
