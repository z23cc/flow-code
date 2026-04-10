//! Persistent code graph with query APIs.
//!
//! Builds a symbol-level reference graph from the codebase, computes PageRank,
//! and persists to `.flow/graph.bin` using bincode. Supports incremental
//! updates for changed files and query APIs (refs, impact, repo map).

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::code_structure::{self, Symbol};

// ── Types ───────────────────────────────────────────────────────────

/// A code symbol entry in the persisted graph.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SymbolEntry {
    pub id: usize,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub signature: String,
}

impl From<&Symbol> for SymbolEntry {
    fn from(s: &Symbol) -> Self {
        Self {
            id: 0, // assigned during build
            name: s.name.clone(),
            kind: s.kind.to_string(),
            file: s.file.clone(),
            line: s.line,
            signature: s.signature.clone(),
        }
    }
}

/// Summary statistics about the graph.
#[derive(Debug, Clone, Serialize)]
pub struct GraphStats {
    pub symbol_count: usize,
    pub file_count: usize,
    pub edge_count: usize,
    pub built_at_epoch_ms: u64,
}

/// The persisted code graph.
#[derive(Serialize, Deserialize)]
pub struct CodeGraph {
    /// All symbols indexed by ID.
    pub symbols: Vec<SymbolEntry>,
    /// symbol_name -> list of symbol IDs that define it.
    pub name_to_ids: HashMap<String, Vec<usize>>,
    /// file_path -> list of symbol IDs defined in it.
    pub file_to_ids: HashMap<String, Vec<usize>>,
    /// Forward edges: symbol_id -> list of symbol_ids it references.
    pub refs_forward: HashMap<usize, Vec<usize>>,
    /// Reverse edges: symbol_id -> list of symbol_ids that reference it.
    pub refs_reverse: HashMap<usize, Vec<usize>>,
    /// File-level forward edges: file -> files it depends on.
    pub file_deps: HashMap<String, HashSet<String>>,
    /// File-level reverse edges: file -> files that depend on it.
    pub file_dependents: HashMap<String, HashSet<String>>,
    /// PageRank scores (indexed by symbol ID).
    pub ranks: Vec<f64>,
    /// Build timestamp (ms since epoch).
    pub built_at_epoch_ms: u64,
}

// ── Helpers ─────────────────────────────────────────────────────────

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Rough token estimate: ~4 chars per token.
fn estimate_tokens(s: &str) -> usize {
    (s.len() + 3) / 4
}

// ── Implementation ──────────────────────────────────────────────────

impl CodeGraph {
    /// Build a code graph from scratch by scanning all source files under `root`.
    pub fn build(root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let symbols = code_structure::extract_all_symbols(root)?;

        let mut graph = Self {
            symbols: Vec::new(),
            name_to_ids: HashMap::new(),
            file_to_ids: HashMap::new(),
            refs_forward: HashMap::new(),
            refs_reverse: HashMap::new(),
            file_deps: HashMap::new(),
            file_dependents: HashMap::new(),
            ranks: Vec::new(),
            built_at_epoch_ms: now_epoch_ms(),
        };

        // 1. Populate symbols and indexes.
        for (id, sym) in symbols.iter().enumerate() {
            let mut entry = SymbolEntry::from(sym);
            entry.id = id;
            graph.symbols.push(entry);
            graph
                .name_to_ids
                .entry(sym.name.clone())
                .or_default()
                .push(id);
            graph
                .file_to_ids
                .entry(sym.file.clone())
                .or_default()
                .push(id);
        }

        // 2. Build edges by scanning file contents for symbol name references.
        graph.build_edges(root);

        // 3. Compute PageRank.
        graph.compute_pagerank();

        Ok(graph)
    }

    /// Build reference edges by scanning file content for symbol names.
    fn build_edges(&mut self, root: &Path) {
        // Collect unique files.
        let files: Vec<String> = self.file_to_ids.keys().cloned().collect();

        // Build name -> defining file set for quick lookup.
        let mut name_to_files: HashMap<&str, Vec<usize>> = HashMap::new();
        for sym in &self.symbols {
            name_to_files.entry(&sym.name).or_default().push(sym.id);
        }

        for file in &files {
            let full_path = if Path::new(file).is_absolute() {
                std::path::PathBuf::from(file)
            } else {
                root.join(file)
            };

            let content = match std::fs::read(&full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let src_ids: Vec<usize> = self.file_to_ids.get(file).cloned().unwrap_or_default();

            for (name, def_ids) in &name_to_files {
                // Skip very short names (likely false positives).
                if name.len() < 3 {
                    continue;
                }

                // Use memchr for fast substring search.
                let needle = name.as_bytes();
                if memchr::memmem::find(&content, needle).is_none() {
                    continue;
                }

                // This file references this symbol name.
                for &def_id in def_ids {
                    let def_file = &self.symbols[def_id].file;
                    if def_file == file {
                        continue; // skip self-references
                    }

                    // Create symbol-level edges: each src symbol -> def symbol.
                    for &src_id in &src_ids {
                        self.refs_forward.entry(src_id).or_default().push(def_id);
                        self.refs_reverse.entry(def_id).or_default().push(src_id);
                    }

                    // Create file-level edges.
                    self.file_deps
                        .entry(file.clone())
                        .or_default()
                        .insert(def_file.clone());
                    self.file_dependents
                        .entry(def_file.clone())
                        .or_default()
                        .insert(file.clone());
                }
            }
        }

        // Deduplicate edge lists.
        for edges in self.refs_forward.values_mut() {
            edges.sort();
            edges.dedup();
        }
        for edges in self.refs_reverse.values_mut() {
            edges.sort();
            edges.dedup();
        }
    }

    /// Compute PageRank over file-level dependency graph.
    fn compute_pagerank(&mut self) {
        let files: Vec<String> = {
            let mut f: Vec<String> = self.file_to_ids.keys().cloned().collect();
            f.sort();
            f
        };
        let file_count = files.len();
        if file_count == 0 {
            self.ranks = vec![1.0; self.symbols.len()];
            return;
        }

        let file_idx: HashMap<&str, usize> = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.as_str(), i))
            .collect();

        let damping = 0.85;
        let iterations = 20;
        let initial = 1.0 / file_count as f64;

        let mut ranks = vec![initial; file_count];
        let mut new_ranks = vec![0.0; file_count];

        for _ in 0..iterations {
            let base = (1.0 - damping) / file_count as f64;
            for r in &mut new_ranks {
                *r = base;
            }

            for (i, file) in files.iter().enumerate() {
                let out_degree = self.file_deps.get(file).map(|s| s.len()).unwrap_or(0);

                if out_degree == 0 {
                    // Dangling node: distribute rank equally.
                    let share = ranks[i] / file_count as f64;
                    for r in &mut new_ranks {
                        *r += damping * share;
                    }
                } else {
                    let share = ranks[i] / out_degree as f64;
                    if let Some(deps) = self.file_deps.get(file) {
                        for dep in deps {
                            if let Some(&dep_idx) = file_idx.get(dep.as_str()) {
                                new_ranks[dep_idx] += damping * share;
                            }
                        }
                    }
                }
            }

            std::mem::swap(&mut ranks, &mut new_ranks);
        }

        // Map file ranks to symbol ranks.
        let file_ranks: HashMap<&str, f64> = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.as_str(), ranks[i]))
            .collect();

        self.ranks = self
            .symbols
            .iter()
            .map(|sym| {
                file_ranks
                    .get(sym.file.as_str())
                    .copied()
                    .unwrap_or(initial)
            })
            .collect();
    }

    /// Incrementally update: re-process only changed files.
    pub fn update(
        &mut self,
        root: &Path,
        changed_files: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let changed_set: HashSet<&str> = changed_files.iter().map(|s| s.as_str()).collect();

        // 1. Collect IDs of symbols in changed files.
        let mut removed_ids: HashSet<usize> = HashSet::new();
        for file in changed_files {
            if let Some(ids) = self.file_to_ids.remove(file) {
                for id in &ids {
                    removed_ids.insert(*id);
                }
            }
        }

        // 2. Remove edges involving removed symbols.
        self.refs_forward.retain(|k, _| !removed_ids.contains(k));
        self.refs_reverse.retain(|k, _| !removed_ids.contains(k));
        for edges in self.refs_forward.values_mut() {
            edges.retain(|id| !removed_ids.contains(id));
        }
        for edges in self.refs_reverse.values_mut() {
            edges.retain(|id| !removed_ids.contains(id));
        }

        // 3. Remove file-level edges for changed files.
        for file in changed_files {
            self.file_deps.remove(file);
            self.file_dependents.remove(file);
        }
        for deps in self.file_deps.values_mut() {
            deps.retain(|f| !changed_set.contains(f.as_str()));
        }
        for deps in self.file_dependents.values_mut() {
            deps.retain(|f| !changed_set.contains(f.as_str()));
        }

        // 4. Remove old symbols and clean up name_to_ids.
        // Mark removed symbols (we can't easily reindex, so just clear their names).
        for &id in &removed_ids {
            if id < self.symbols.len() {
                self.symbols[id].name.clear();
                self.symbols[id].file.clear();
            }
        }
        for ids in self.name_to_ids.values_mut() {
            ids.retain(|id| !removed_ids.contains(id));
        }
        self.name_to_ids.retain(|_, ids| !ids.is_empty());

        // 5. Re-extract symbols for changed files and add them.
        for file in changed_files {
            let full_path = if Path::new(file).is_absolute() {
                std::path::PathBuf::from(file)
            } else {
                root.join(file)
            };

            if !full_path.is_file() {
                continue;
            }

            let new_symbols = match code_structure::extract_symbols(&full_path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            for sym in &new_symbols {
                let id = self.symbols.len();
                let mut entry = SymbolEntry::from(sym);
                entry.id = id;
                self.symbols.push(entry);
                self.name_to_ids
                    .entry(sym.name.clone())
                    .or_default()
                    .push(id);
                self.file_to_ids
                    .entry(sym.file.clone())
                    .or_default()
                    .push(id);
            }
        }

        // 6. Rebuild edges (full rebuild is simpler and correct).
        self.refs_forward.clear();
        self.refs_reverse.clear();
        self.file_deps.clear();
        self.file_dependents.clear();
        self.build_edges(root);

        // 7. Recompute PageRank.
        self.compute_pagerank();

        self.built_at_epoch_ms = now_epoch_ms();
        Ok(())
    }

    /// Find all references TO a symbol (who calls/uses this?).
    pub fn find_refs(&self, symbol_name: &str) -> Vec<&SymbolEntry> {
        let def_ids = match self.name_to_ids.get(symbol_name) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let mut result_ids: HashSet<usize> = HashSet::new();
        for &def_id in def_ids {
            if let Some(referrers) = self.refs_reverse.get(&def_id) {
                for &ref_id in referrers {
                    result_ids.insert(ref_id);
                }
            }
        }

        let mut results: Vec<&SymbolEntry> = result_ids
            .iter()
            .filter_map(|&id| self.symbols.get(id))
            .filter(|s| !s.name.is_empty()) // skip tombstoned entries
            .collect();
        results.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        results
    }

    /// Find all files that would be impacted by changing a file (transitive dependents).
    /// BFS up to depth 3 to avoid explosion.
    pub fn find_impact(&self, file_path: &str) -> Vec<String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        queue.push_back((file_path.to_string(), 0));
        visited.insert(file_path.to_string());

        let max_depth = 3;

        while let Some((file, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            if let Some(dependents) = self.file_dependents.get(&file) {
                for dep in dependents {
                    if visited.insert(dep.clone()) {
                        queue.push_back((dep.clone(), depth + 1));
                    }
                }
            }
        }

        // Remove the source file itself from the result.
        visited.remove(file_path);
        let mut result: Vec<String> = visited.into_iter().collect();
        result.sort();
        result
    }

    /// Generate a repo map from cached graph data within a token budget.
    pub fn repo_map(&self, budget: usize) -> String {
        if self.symbols.is_empty() {
            return String::from("(no symbols found)");
        }

        // Build ranked list: (symbol_index, rank) sorted by rank descending.
        let mut ranked: Vec<(usize, f64)> = self
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.name.is_empty()) // skip tombstoned
            .map(|(i, _)| {
                let rank = if i < self.ranks.len() {
                    self.ranks[i]
                } else {
                    0.0
                };
                (i, rank)
            })
            .collect();

        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| self.symbols[a.0].file.cmp(&self.symbols[b.0].file))
                .then_with(|| self.symbols[a.0].line.cmp(&self.symbols[b.0].line))
        });

        let mut output = String::new();
        let mut current_file = String::new();
        let mut tokens_used: usize = 0;

        for (idx, _rank) in &ranked {
            let sym = &self.symbols[*idx];
            let display_file = &sym.file;

            let file_header_cost = if *display_file != current_file {
                estimate_tokens(&format!("{display_file}:\n"))
            } else {
                0
            };
            let sig_cost = estimate_tokens(&format!("  {}\n", sym.signature));

            if budget > 0 && tokens_used + file_header_cost + sig_cost > budget {
                break;
            }

            if *display_file != current_file {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&format!("{display_file}:\n"));
                tokens_used += file_header_cost;
                current_file = display_file.clone();
            }

            output.push_str(&format!("  {}\n", sym.signature));
            tokens_used += sig_cost;
        }

        output
    }

    /// Save the graph to a bincode file with atomic write.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let serialized =
            bincode::serde::encode_to_vec(self, bincode::config::standard()).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("bincode encode: {e}"))
            })?;

        let tmp_path = path.with_extension("tmp");
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&serialized)?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Load the graph from a bincode file.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        let (graph, _) =
            bincode::serde::decode_from_slice::<Self, _>(&data, bincode::config::standard())
                .map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("bincode decode: {e}"),
                    )
                })?;

        Ok(graph)
    }

    /// Return summary statistics.
    pub fn stats(&self) -> GraphStats {
        let active_symbols = self.symbols.iter().filter(|s| !s.name.is_empty()).count();
        let edge_count: usize = self.refs_forward.values().map(|v| v.len()).sum();
        let file_count = self.file_to_ids.len();

        GraphStats {
            symbol_count: active_symbols,
            file_count,
            edge_count,
            built_at_epoch_ms: self.built_at_epoch_ms,
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

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

        // handler.rs - references authenticate and User
        let handler = dir.path().join("handler.rs");
        let mut f = std::fs::File::create(&handler).unwrap();
        write!(
            f,
            r#"use crate::auth::{{authenticate, User}};

pub fn handle_request(token: &str) -> User {{
    authenticate(token).unwrap()
}}
"#
        )
        .unwrap();

        dir
    }

    #[test]
    fn test_build_graph() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        assert!(graph.stats().symbol_count > 0);
        assert!(graph.stats().file_count > 0);
        assert!(graph.stats().edge_count > 0);
        assert!(graph.stats().built_at_epoch_ms > 0);
    }

    #[test]
    fn test_find_refs() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        // User is referenced from db.rs and handler.rs
        let refs = graph.find_refs("User");
        assert!(!refs.is_empty(), "Expected references to User");

        // authenticate is referenced from handler.rs
        let refs = graph.find_refs("authenticate");
        assert!(!refs.is_empty(), "Expected references to authenticate");
    }

    #[test]
    fn test_find_impact() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        // Find the auth.rs file path (it will be absolute).
        let auth_file = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("auth.rs"))
            .cloned()
            .unwrap();

        let impact = graph.find_impact(&auth_file);
        // Changing auth.rs should impact handler.rs and db.rs (they reference symbols from auth.rs).
        assert!(!impact.is_empty(), "Expected impact from changing auth.rs");
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let save_path = dir.path().join("graph.bin");
        graph.save(&save_path).unwrap();

        let loaded = CodeGraph::load(&save_path).unwrap();
        assert_eq!(loaded.stats().symbol_count, graph.stats().symbol_count);
        assert_eq!(loaded.stats().file_count, graph.stats().file_count);
        assert_eq!(loaded.stats().edge_count, graph.stats().edge_count);
    }

    #[test]
    fn test_repo_map_from_graph() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let map = graph.repo_map(0);
        assert!(!map.is_empty());
        assert!(
            map.contains("authenticate") || map.contains("query_user"),
            "Map should contain symbol names"
        );
    }

    #[test]
    fn test_repo_map_with_budget() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let map = graph.repo_map(50);
        // Should still produce some output within budget.
        assert!(!map.is_empty() || map == "(no symbols found)");
    }

    #[test]
    fn test_incremental_update() {
        let dir = setup_test_dir();
        let mut graph = CodeGraph::build(dir.path()).unwrap();

        let initial_count = graph.stats().symbol_count;

        // Add a new file.
        let new_file = dir.path().join("cache.rs");
        {
            let mut f = std::fs::File::create(&new_file).unwrap();
            write!(
                f,
                r#"pub fn cache_user(user: User) -> bool {{
    true
}}
"#
            )
            .unwrap();
        }

        let new_file_str = new_file.display().to_string();
        graph.update(dir.path(), &[new_file_str]).unwrap();

        assert!(
            graph.stats().symbol_count > initial_count,
            "Should have more symbols after adding a file"
        );
    }

    #[test]
    fn test_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let graph = CodeGraph::build(dir.path()).unwrap();

        assert_eq!(graph.stats().symbol_count, 0);
        assert_eq!(graph.stats().file_count, 0);
        assert_eq!(graph.stats().edge_count, 0);

        let map = graph.repo_map(0);
        assert_eq!(map, "(no symbols found)");
    }

    #[test]
    fn test_find_refs_nonexistent() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let refs = graph.find_refs("nonexistent_symbol_xyz");
        assert!(refs.is_empty());
    }
}
