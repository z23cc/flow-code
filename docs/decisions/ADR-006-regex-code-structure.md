---
id: ADR-006
status: accepted
date: 2026-04-09
tags: [search, code-structure]
verify: "! grep -n 'tree-sitter' flowctl/Cargo.toml flowctl/crates/*/Cargo.toml 2>/dev/null | grep -v '#'"
scope: "flowctl/crates/flowctl-core/src/code_structure.rs"
---
# ADR-006: Regex-Based Code Structure Extraction (Not tree-sitter)

## Status
ACCEPTED

## Date
2026-04-09

## Context
`flowctl code-structure` needs to extract function/type signatures across 9+ languages. tree-sitter provides perfect AST parsing but adds 6+ grammar crate dependencies and significant compile time. Regex extraction is imperfect but covers 90% of use cases with zero new deps.

## Decision
Use regex patterns per language for symbol extraction. API designed for future tree-sitter swap (`Symbol`, `SymbolKind`, `extract_symbols` are stable). Tree-sitter can be added later without breaking consumers.

## Consequences
- **Constraint**: Do NOT add tree-sitter deps to flowctl Cargo.toml (use regex fallback)
- **Benefit**: Zero new dependencies, fast compilation, works immediately
- **Trade-off**: Regex misses nested/complex definitions. Acceptable for repo-map ranking (PageRank tolerates noise)

## Rejected Alternatives
- tree-sitter-language-pack: 248 languages but heavy runtime downloads, unclear stability
- Individual tree-sitter-* crates: 5+ crates, minutes of C compilation, binary bloat
- LSP-based: Requires running language servers, too heavy for CLI
