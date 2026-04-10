//! Persistent code graph with query APIs.
//!
//! Builds a symbol-level reference graph from the codebase, computes PageRank,
//! and persists to `.flow/graph.bin` using bincode. Supports incremental
//! updates for changed files and query APIs (refs, impact, repo map).

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::code_structure::{self, Symbol};

// ── Constants ──────────────────────────────────────────────────────

/// Serialization format version. Increment when CodeGraph fields change.
const GRAPH_FORMAT_VERSION: u32 = 2;

// ── Types ───────────────────────────────────────────────────────────

/// Kind of edge relationship between symbols.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Function/method call: `foo()`
    Calls,
    /// Import/use: `use X`, `import X`, `require(X)`
    Imports,
    /// Inheritance/implementation: `extends X`, `impl X for Y`
    Inherits,
    /// Generic reference (fallback when type cannot be determined)
    References,
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Calls => write!(f, "calls"),
            Self::Imports => write!(f, "imports"),
            Self::Inherits => write!(f, "inherits"),
            Self::References => write!(f, "references"),
        }
    }
}

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
    pub typed_edge_counts: HashMap<String, usize>,
    pub built_at_epoch_ms: u64,
}

/// Risk assessment for a single file in the review context.
#[derive(Debug, Clone, Serialize)]
pub struct FileRisk {
    pub file: String,
    pub risk_score: f64,
    pub pagerank: f64,
    pub dependent_count: usize,
    pub changed_symbols: Vec<String>,
    pub is_test: bool,
}

/// Review context: blast radius + risk scores + test gaps for a set of changes.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewContext {
    pub changed_files: Vec<String>,
    pub impacted_files: Vec<FileRisk>,
    pub test_gaps: Vec<String>,
    pub total_risk_score: f64,
}

/// The persisted code graph.
#[derive(Serialize, Deserialize, Debug)]
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
    /// Typed edges: symbol_id -> list of (target_symbol_id, edge_kind).
    pub typed_edges: HashMap<usize, Vec<(usize, EdgeKind)>>,
    /// SHA-256 content hashes per file for incremental skip.
    pub file_hashes: HashMap<String, String>,
    /// Edge provenance: file_path -> list of (source_id, target_id) edges produced by scanning that file.
    pub edge_provenance: HashMap<String, Vec<(usize, usize)>>,
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

/// Compute a fast content hash for change detection (not cryptographic).
fn content_hash(data: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    let h1 = hasher.finish();
    // Mix in length for extra collision resistance.
    let mut hasher2 = std::collections::hash_map::DefaultHasher::new();
    data.len().hash(&mut hasher2);
    data.get(..128.min(data.len())).hash(&mut hasher2);
    let h2 = hasher2.finish();
    format!("{h1:016x}{h2:016x}")
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
            typed_edges: HashMap::new(),
            file_hashes: HashMap::new(),
            edge_provenance: HashMap::new(),
            ranks: Vec::new(),
            built_at_epoch_ms: now_epoch_ms(),
        };

        // 1. Populate symbols and indexes, compute file hashes.
        let mut seen_files: HashSet<String> = HashSet::new();
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

            // Compute file hash once per file.
            if seen_files.insert(sym.file.clone()) {
                let full_path = if Path::new(&sym.file).is_absolute() {
                    std::path::PathBuf::from(&sym.file)
                } else {
                    root.join(&sym.file)
                };
                if let Ok(data) = std::fs::read(&full_path) {
                    graph
                        .file_hashes
                        .insert(sym.file.clone(), content_hash(&data));
                }
            }
        }

        // 2. Build edges by scanning file contents for symbol name references.
        graph.build_edges(root);

        // 3. Compute PageRank.
        graph.compute_pagerank();

        Ok(graph)
    }

    /// Classify the edge type for a symbol reference found in file content.
    fn classify_edge(content: &[u8], name: &str, def_kind: &str) -> EdgeKind {
        let content_str = String::from_utf8_lossy(content);

        // Check for import patterns.
        // Rust: `use ...::name` or `use ...{name` or `use name`
        // JS/TS: `import name`, `import { name`, `import {name`
        // Python: `from X import name`, `import name`
        // Go/other: `require("name")`, `require('name')`
        // Direct import keyword patterns.
        let direct_import_checks: &[&str] = &[
            &format!("use {name}"),           // use Name directly
            &format!("import {name}"),        // import Name or from X import Name
            &format!("require(\"{name}"),     // require("Name")
            &format!("require('{name}"),      // require('Name')
        ];

        for pat in direct_import_checks {
            if content_str.contains(pat) {
                return EdgeKind::Imports;
            }
        }

        // Path-based import patterns (must appear in a use/import context).
        let has_use_keyword = content_str.contains("use ") || content_str.contains("import ");
        if has_use_keyword {
            let path_patterns: &[&str] = &[
                &format!("::{name}"),         // use crate::path::Name
                &format!("{{{name}"),          // use path::{Name, ...}
                &format!("{{ {name}"),         // use path::{ Name, ...}
            ];
            for pat in path_patterns {
                if content_str.contains(pat) {
                    return EdgeKind::Imports;
                }
            }
        }

        // Check for inheritance (only for type-like symbols).
        let is_type_like = matches!(
            def_kind,
            "struct" | "class" | "trait" | "interface" | "type" | "enum"
        );
        if is_type_like {
            // More specific patterns to avoid false positives with `: name` in variable declarations.
            let inherit_checks: &[&str] = &[
                &format!("extends {name}"),       // JS/TS/Java class extends
                &format!("impl {name} for"),      // Rust impl Trait for Type
                &format!("implements {name}"),     // Java/TS implements
                &format!("class {name}("),         // Python class Name(Base) — name IS the class
            ];

            for pat in inherit_checks {
                if content_str.contains(pat) {
                    return EdgeKind::Inherits;
                }
            }

            // Python inheritance: `class X(Name)` where Name is the parent.
            // Must be inside parens after `class` keyword.
            if content_str.contains(&format!("({name})"))
                && content_str.contains("class ")
            {
                return EdgeKind::Inherits;
            }
        }

        // Check for call patterns: `name(` — function/method invocations.
        let is_callable = matches!(def_kind, "fn" | "function" | "method" | "const");
        if is_callable {
            let call_needle = format!("{name}(");
            if content_str.contains(&call_needle) {
                return EdgeKind::Calls;
            }
        }

        // Fallback: generic reference.
        EdgeKind::References
    }

    /// Build reference edges by scanning file content for symbol names.
    /// Populates both untyped (refs_forward/refs_reverse) and typed (typed_edges) edge maps,
    /// plus edge_provenance for incremental update support.
    fn build_edges(&mut self, root: &Path) {
        self.build_edges_for_files(root, None);
    }

    /// Build edges for specific files only, or all files if `only_files` is None.
    fn build_edges_for_files(&mut self, root: &Path, only_files: Option<&HashSet<String>>) {
        // Collect files to process.
        let files: Vec<String> = if let Some(subset) = only_files {
            subset.iter().cloned().collect()
        } else {
            self.file_to_ids.keys().cloned().collect()
        };

        // Build name -> defining symbols for quick lookup.
        let mut name_to_syms: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
        for sym in &self.symbols {
            if sym.name.is_empty() {
                continue;
            }
            name_to_syms
                .entry(&sym.name)
                .or_default()
                .push((sym.id, &sym.kind));
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
            let mut file_edges: Vec<(usize, usize)> = Vec::new();

            for (name, sym_entries) in &name_to_syms {
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
                for &(def_id, def_kind) in sym_entries {
                    let def_file = &self.symbols[def_id].file;
                    if def_file == file {
                        continue; // skip self-references
                    }

                    // Classify edge type.
                    let edge_kind = Self::classify_edge(&content, name, def_kind);

                    // Create symbol-level edges.
                    for &src_id in &src_ids {
                        self.refs_forward.entry(src_id).or_default().push(def_id);
                        self.refs_reverse.entry(def_id).or_default().push(src_id);
                        self.typed_edges
                            .entry(src_id)
                            .or_default()
                            .push((def_id, edge_kind));
                        file_edges.push((src_id, def_id));
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

            // Record provenance.
            if !file_edges.is_empty() {
                self.edge_provenance.insert(file.clone(), file_edges);
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
        for edges in self.typed_edges.values_mut() {
            edges.sort_by_key(|&(id, _)| id);
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

    /// Incrementally update: re-process only truly changed files (hash-verified).
    ///
    /// Uses content hashes to skip files that haven't actually changed,
    /// and edge provenance to only rebuild edges for affected files + dependents.
    pub fn update(
        &mut self,
        root: &Path,
        changed_files: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 0. Filter to actually changed files via content hash.
        let mut actually_changed: Vec<String> = Vec::new();
        for file in changed_files {
            let full_path = if Path::new(file).is_absolute() {
                std::path::PathBuf::from(file)
            } else {
                root.join(file)
            };
            if !full_path.is_file() {
                actually_changed.push(file.clone()); // deleted file — must process
                continue;
            }
            match std::fs::read(&full_path) {
                Ok(data) => {
                    let new_hash = content_hash(&data);
                    let old_hash = self.file_hashes.get(file);
                    if old_hash.map_or(true, |h| *h != new_hash) {
                        self.file_hashes.insert(file.clone(), new_hash);
                        actually_changed.push(file.clone());
                    }
                    // else: hash matches, skip this file
                }
                Err(_) => {
                    actually_changed.push(file.clone());
                }
            }
        }

        if actually_changed.is_empty() {
            return Ok(());
        }

        // 1. Find dependent files (2-hop cascade).
        let mut affected_files: HashSet<String> = actually_changed.iter().cloned().collect();
        for file in &actually_changed {
            if let Some(deps) = self.file_dependents.get(file) {
                for dep in deps {
                    affected_files.insert(dep.clone());
                    // 2nd hop.
                    if let Some(deps2) = self.file_dependents.get(dep) {
                        for dep2 in deps2 {
                            affected_files.insert(dep2.clone());
                        }
                    }
                }
            }
        }

        // 2. Remove symbols and edges for actually changed files.
        let mut removed_ids: HashSet<usize> = HashSet::new();
        for file in &actually_changed {
            if let Some(ids) = self.file_to_ids.remove(file) {
                for id in &ids {
                    removed_ids.insert(*id);
                }
            }
        }

        // Remove edges using provenance (targeted, not full rebuild).
        for file in &affected_files {
            if let Some(edges) = self.edge_provenance.remove(file) {
                for (src_id, def_id) in &edges {
                    if let Some(fwd) = self.refs_forward.get_mut(src_id) {
                        fwd.retain(|id| id != def_id);
                    }
                    if let Some(rev) = self.refs_reverse.get_mut(def_id) {
                        rev.retain(|id| id != src_id);
                    }
                    if let Some(typed) = self.typed_edges.get_mut(src_id) {
                        typed.retain(|(id, _)| id != def_id);
                    }
                }
            }
            self.file_deps.remove(file);
        }
        // Clean up file_dependents referencing affected files.
        for deps in self.file_dependents.values_mut() {
            deps.retain(|f| !affected_files.contains(f));
        }

        // 3. Remove old symbols (tombstone by clearing name).
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

        // 4. Re-extract symbols for actually changed files.
        for file in &actually_changed {
            let full_path = if Path::new(file).is_absolute() {
                std::path::PathBuf::from(file)
            } else {
                root.join(file)
            };

            if !full_path.is_file() {
                self.file_hashes.remove(file);
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

        // 5. Rebuild edges only for affected files (changed + dependents).
        self.build_edges_for_files(root, Some(&affected_files));

        // 6. Recompute PageRank.
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
    /// BFS up to `max_depth` hops (default 3) to avoid explosion.
    pub fn find_impact(&self, file_path: &str) -> Vec<String> {
        self.find_impact_with_depth(file_path, 3)
    }

    /// Find impacted files with configurable BFS depth.
    pub fn find_impact_with_depth(&self, file_path: &str, max_depth: usize) -> Vec<String> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();

        queue.push_back((file_path.to_string(), 0));
        visited.insert(file_path.to_string());

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

    /// Generate review context for changed files: blast radius, risk scores, test gaps.
    ///
    /// Returns a structured result with impacted files sorted by risk and
    /// identified test coverage gaps.
    pub fn review_context(
        &self,
        changed_files: &[String],
        max_depth: usize,
    ) -> ReviewContext {
        let mut all_impacted: HashMap<String, FileRisk> = HashMap::new();

        for file in changed_files {
            let impacted = self.find_impact_with_depth(file, max_depth);

            for imp_file in &impacted {
                let entry = all_impacted
                    .entry(imp_file.clone())
                    .or_insert_with(|| FileRisk {
                        file: imp_file.clone(),
                        risk_score: 0.0,
                        pagerank: 0.0,
                        dependent_count: 0,
                        changed_symbols: Vec::new(),
                        is_test: false,
                    });

                // Count how many files depend on this impacted file.
                entry.dependent_count = self
                    .file_dependents
                    .get(imp_file)
                    .map(|d| d.len())
                    .unwrap_or(0);

                // Get PageRank for the file.
                if let Some(ids) = self.file_to_ids.get(imp_file) {
                    if let Some(&first_id) = ids.first() {
                        if first_id < self.ranks.len() {
                            entry.pagerank = self.ranks[first_id];
                        }
                    }
                }

                // Check if this is a test file.
                let lower = imp_file.to_lowercase();
                entry.is_test = lower.contains("test") || lower.contains("spec");
            }
        }

        // Add changed files themselves.
        for file in changed_files {
            let entry = all_impacted
                .entry(file.clone())
                .or_insert_with(|| FileRisk {
                    file: file.clone(),
                    risk_score: 0.0,
                    pagerank: 0.0,
                    dependent_count: 0,
                    changed_symbols: Vec::new(),
                    is_test: false,
                });

            // Collect symbols defined in the changed file.
            if let Some(ids) = self.file_to_ids.get(file) {
                for &id in ids {
                    if id < self.symbols.len() && !self.symbols[id].name.is_empty() {
                        entry
                            .changed_symbols
                            .push(self.symbols[id].signature.clone());
                    }
                }
            }

            entry.dependent_count = self
                .file_dependents
                .get(file)
                .map(|d| d.len())
                .unwrap_or(0);

            if let Some(ids) = self.file_to_ids.get(file) {
                if let Some(&first_id) = ids.first() {
                    if first_id < self.ranks.len() {
                        entry.pagerank = self.ranks[first_id];
                    }
                }
            }

            let lower = file.to_lowercase();
            entry.is_test = lower.contains("test") || lower.contains("spec");
        }

        // Compute risk scores: PageRank * (1 + dependent_count) * depth_factor.
        let max_pagerank = all_impacted
            .values()
            .map(|r| r.pagerank)
            .fold(0.0_f64, f64::max)
            .max(0.001);

        for risk in all_impacted.values_mut() {
            let normalized_rank = risk.pagerank / max_pagerank;
            let dep_factor = 1.0 + (risk.dependent_count as f64).ln().max(0.0);
            risk.risk_score = normalized_rank * dep_factor * 100.0;
        }

        // Sort by risk score descending.
        let mut impacted_files: Vec<FileRisk> = all_impacted.into_values().collect();
        impacted_files.sort_by(|a, b| {
            b.risk_score
                .partial_cmp(&a.risk_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Find test gaps: impacted non-test files that have no test file in the impact set.
        let test_files: HashSet<&str> = impacted_files
            .iter()
            .filter(|r| r.is_test)
            .map(|r| r.file.as_str())
            .collect();
        let test_gaps: Vec<String> = impacted_files
            .iter()
            .filter(|r| !r.is_test)
            .filter(|r| {
                // Check if any test file depends on this file.
                if let Some(deps) = self.file_dependents.get(&r.file) {
                    !deps.iter().any(|d| test_files.contains(d.as_str()))
                } else {
                    true
                }
            })
            .map(|r| r.file.clone())
            .collect();

        let total_risk: f64 = impacted_files.iter().map(|r| r.risk_score).sum();

        ReviewContext {
            changed_files: changed_files.to_vec(),
            impacted_files,
            test_gaps,
            total_risk_score: total_risk,
        }
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

    /// Save the graph to a bincode file with version prefix and atomic write.
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
        // Write 4-byte version prefix for forward compatibility.
        file.write_all(&GRAPH_FORMAT_VERSION.to_le_bytes())?;
        file.write_all(&serialized)?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Load the graph from a bincode file.
    ///
    /// Returns `InvalidData` error if the version prefix doesn't match,
    /// allowing callers to trigger a rebuild.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        // Check version prefix (4 bytes).
        if data.len() < 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "graph.bin too short (missing version header) — rebuild with `flowctl graph build`",
            ));
        }
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if version != GRAPH_FORMAT_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "graph.bin version mismatch (got {version}, expected {GRAPH_FORMAT_VERSION}) — rebuild with `flowctl graph build`"
                ),
            ));
        }

        let (graph, _) =
            bincode::serde::decode_from_slice::<Self, _>(&data[4..], bincode::config::standard())
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

        // Count typed edges by kind.
        let mut typed_edge_counts: HashMap<String, usize> = HashMap::new();
        for edges in self.typed_edges.values() {
            for (_, kind) in edges {
                *typed_edge_counts.entry(kind.to_string()).or_insert(0) += 1;
            }
        }

        GraphStats {
            symbol_count: active_symbols,
            file_count,
            edge_count,
            typed_edge_counts,
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

    #[test]
    fn test_typed_edges_populated() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let stats = graph.stats();
        // Should have typed edges — at least imports (use crate::auth::User)
        // and calls (authenticate(), query_user()).
        assert!(
            !stats.typed_edge_counts.is_empty(),
            "Expected typed edges to be classified, got {:?}",
            stats.typed_edge_counts
        );
    }

    #[test]
    fn test_edge_type_imports() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let stats = graph.stats();
        // db.rs has `use crate::auth::User` — should detect as import.
        let import_count = stats.typed_edge_counts.get("imports").copied().unwrap_or(0);
        assert!(
            import_count > 0,
            "Expected import edges from 'use crate::auth::User', got typed_edge_counts={:?}",
            stats.typed_edge_counts
        );
    }

    #[test]
    fn test_edge_type_calls() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let stats = graph.stats();
        // handler.rs calls authenticate() — should detect as call edge.
        let call_count = stats.typed_edge_counts.get("calls").copied().unwrap_or(0);
        assert!(
            call_count > 0,
            "Expected call edges from 'authenticate(token)', got typed_edge_counts={:?}",
            stats.typed_edge_counts
        );
    }

    #[test]
    fn test_file_hashes_populated() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        // Should have hashes for auth.rs, db.rs, handler.rs.
        assert!(
            graph.file_hashes.len() >= 3,
            "Expected file hashes for at least 3 files, got {}",
            graph.file_hashes.len()
        );
    }

    #[test]
    fn test_hash_skip_unchanged() {
        let dir = setup_test_dir();
        let mut graph = CodeGraph::build(dir.path()).unwrap();

        let initial_count = graph.stats().symbol_count;
        // "Update" with an existing file that hasn't changed — should be a no-op.
        let auth_file = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("auth.rs"))
            .cloned()
            .unwrap();

        graph.update(dir.path(), &[auth_file]).unwrap();

        // Symbol count should remain the same (hash matched, file skipped).
        assert_eq!(
            graph.stats().symbol_count,
            initial_count,
            "Hash-skip should preserve symbol count for unchanged files"
        );
    }

    #[test]
    fn test_version_mismatch_on_load() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let save_path = dir.path().join("graph.bin");
        graph.save(&save_path).unwrap();

        // Corrupt the version header.
        let mut data = std::fs::read(&save_path).unwrap();
        data[0] = 0xFF;
        data[1] = 0xFF;
        std::fs::write(&save_path, &data).unwrap();

        let result = CodeGraph::load(&save_path);
        assert!(result.is_err(), "Should fail on version mismatch");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(
            err.to_string().contains("version mismatch"),
            "Error should mention version mismatch: {}",
            err
        );
    }

    #[test]
    fn test_save_load_preserves_new_fields() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let save_path = dir.path().join("graph.bin");
        graph.save(&save_path).unwrap();

        let loaded = CodeGraph::load(&save_path).unwrap();

        // Verify typed edges preserved.
        let orig_stats = graph.stats();
        let loaded_stats = loaded.stats();
        assert_eq!(
            orig_stats.typed_edge_counts, loaded_stats.typed_edge_counts,
            "Typed edge counts should survive save/load roundtrip"
        );

        // Verify file hashes preserved.
        assert_eq!(
            graph.file_hashes.len(),
            loaded.file_hashes.len(),
            "File hashes should survive save/load roundtrip"
        );

        // Verify edge provenance preserved.
        assert_eq!(
            graph.edge_provenance.len(),
            loaded.edge_provenance.len(),
            "Edge provenance should survive save/load roundtrip"
        );
    }

    #[test]
    fn test_review_context() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        // Get the auth.rs path.
        let auth_file = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("auth.rs"))
            .cloned()
            .unwrap();

        let ctx = graph.review_context(&[auth_file.clone()], 3);

        // Changed file should be in the result.
        assert!(
            ctx.changed_files.contains(&auth_file),
            "Changed file should be listed"
        );

        // Should have impacted files (handler.rs and db.rs depend on auth.rs).
        assert!(
            !ctx.impacted_files.is_empty(),
            "Expected impacted files from auth.rs change"
        );

        // Risk scores should be positive.
        assert!(
            ctx.total_risk_score > 0.0,
            "Expected positive total risk score"
        );

        // All risk scores should be non-negative.
        for risk in &ctx.impacted_files {
            assert!(
                risk.risk_score >= 0.0,
                "Risk score should be non-negative for {}",
                risk.file
            );
        }
    }

    #[test]
    fn test_review_context_test_gaps() {
        let dir = tempfile::tempdir().unwrap();

        // Create a module and a handler but no test file.
        let module = dir.path().join("auth.rs");
        let mut f = std::fs::File::create(&module).unwrap();
        write!(f, "pub fn login(user: &str) -> bool {{ true }}").unwrap();

        let handler = dir.path().join("handler.rs");
        let mut f = std::fs::File::create(&handler).unwrap();
        write!(f, "use crate::auth::login;\npub fn handle() {{ login(\"x\"); }}").unwrap();

        let graph = CodeGraph::build(dir.path()).unwrap();

        let auth_path = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("auth.rs"))
            .cloned()
            .unwrap();

        let ctx = graph.review_context(&[auth_path], 3);

        // No test files exist, so test_gaps should include non-test impacted files.
        // (The test gap detection checks if any test file depends on the impacted file.)
        assert!(
            !ctx.test_gaps.is_empty() || ctx.impacted_files.len() <= 1,
            "Expected test gaps when no test files exist"
        );
    }

    #[test]
    fn test_find_impact_with_depth() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        let auth_file = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("auth.rs"))
            .cloned()
            .unwrap();

        // Depth 0 should find nothing.
        let impact_0 = graph.find_impact_with_depth(&auth_file, 0);
        assert!(
            impact_0.is_empty(),
            "Depth 0 should produce no impact results"
        );

        // Depth 1 should find direct dependents.
        let impact_1 = graph.find_impact_with_depth(&auth_file, 1);
        // Depth 3 (default) should find at least as many.
        let impact_3 = graph.find_impact(&auth_file);
        assert!(
            impact_1.len() <= impact_3.len(),
            "Higher depth should find at least as many impacted files"
        );
    }

    #[test]
    fn test_edge_provenance_tracked() {
        let dir = setup_test_dir();
        let graph = CodeGraph::build(dir.path()).unwrap();

        // Edge provenance should track which file produced which edges.
        assert!(
            !graph.edge_provenance.is_empty(),
            "Expected edge provenance to be populated"
        );

        // Each provenance entry should reference valid symbol IDs.
        for (file, edges) in &graph.edge_provenance {
            assert!(
                graph.file_to_ids.contains_key(file)
                    || graph
                        .symbols
                        .iter()
                        .any(|s| s.file == *file && !s.name.is_empty()),
                "Provenance file should exist in the graph"
            );
            for (src, tgt) in edges {
                assert!(
                    *src < graph.symbols.len(),
                    "Provenance source ID should be valid"
                );
                assert!(
                    *tgt < graph.symbols.len(),
                    "Provenance target ID should be valid"
                );
            }
        }
    }

    #[test]
    fn test_classify_edge_imports() {
        let content = b"use crate::auth::User;\nfn main() {}";
        let kind = CodeGraph::classify_edge(content, "User", "struct");
        assert_eq!(kind, EdgeKind::Imports);
    }

    #[test]
    fn test_classify_edge_calls() {
        let content = b"fn main() { authenticate(token); }";
        let kind = CodeGraph::classify_edge(content, "authenticate", "fn");
        assert_eq!(kind, EdgeKind::Calls);
    }

    #[test]
    fn test_classify_edge_inherits() {
        let content = b"class Dog extends Animal { }";
        let kind = CodeGraph::classify_edge(content, "Animal", "class");
        assert_eq!(kind, EdgeKind::Inherits);
    }

    #[test]
    fn test_classify_edge_references_fallback() {
        let content = b"let x: User = get();";
        let kind = CodeGraph::classify_edge(content, "User", "struct");
        // No import, no call, no inherit — should fall back to References.
        assert_eq!(kind, EdgeKind::References);
    }

    #[test]
    fn test_update_deleted_file() {
        let dir = setup_test_dir();
        let mut graph = CodeGraph::build(dir.path()).unwrap();

        // Find the db.rs path.
        let db_file = graph
            .file_to_ids
            .keys()
            .find(|f| f.ends_with("db.rs"))
            .cloned()
            .unwrap();

        let initial_count = graph.stats().symbol_count;

        // Delete the file from disk.
        let full_path = std::path::PathBuf::from(&db_file);
        std::fs::remove_file(&full_path).unwrap();

        // Update with the deleted file.
        graph.update(dir.path(), &[db_file.clone()]).unwrap();

        // Symbols from db.rs should be removed.
        assert!(
            graph.stats().symbol_count < initial_count,
            "Deleting db.rs should reduce symbol count"
        );

        // File hash should be cleaned up.
        assert!(
            !graph.file_hashes.contains_key(&db_file),
            "Deleted file's hash should be removed"
        );

        // No edges should reference the deleted file.
        assert!(
            !graph.file_to_ids.contains_key(&db_file),
            "Deleted file should not have entries in file_to_ids"
        );
    }

    #[test]
    fn test_load_old_format_file() {
        let dir = tempfile::tempdir().unwrap();
        let save_path = dir.path().join("graph.bin");

        // Write raw bincode without version prefix (simulates old format).
        let graph = CodeGraph::build(dir.path()).unwrap();
        let serialized =
            bincode::serde::encode_to_vec(&graph, bincode::config::standard()).unwrap();
        std::fs::write(&save_path, &serialized).unwrap();

        let result = CodeGraph::load(&save_path);
        assert!(result.is_err(), "Loading old format should fail");
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_classify_edge_no_false_positive_on_substring() {
        // Function named "import_handler" should not trigger Imports classification.
        let content = b"fn main() { import_handler(); }";
        let kind = CodeGraph::classify_edge(content, "import_handler", "fn");
        // "import import_handler" is not present, so should be Calls (since it's fn + has `(`)
        assert_eq!(kind, EdgeKind::Calls);
    }
}
