---
id: ADR-007
status: accepted
date: 2026-04-09
tags: [search, dependencies]
verify: "grep -q 'nucleo-matcher' flowctl/Cargo.toml"
scope: "flowctl/crates/flowctl-core/src/fuzzy.rs"
---
# ADR-007: nucleo-matcher for Fuzzy Search (Not frizbee/skim)

## Status
ACCEPTED

## Date
2026-04-09

## Context
`flowctl search` needs typo-tolerant fuzzy file matching. Three main contenders: nucleo-matcher (Helix/Nushell), frizbee (fff.nvim), fuzzy-matcher (skim, unmaintained).

## Decision
nucleo-matcher 0.3 — fzf-compatible scoring, proper Unicode/grapheme handling, 3 dependencies only, near 1.0 stability. Combined with ignore crate for .gitignore-aware file walking and custom frecency + git status boosting.

## Consequences
- **Constraint**: Use nucleo-matcher for all fuzzy matching, not frizbee or custom algorithms
- **Benefit**: fzf-compatible ranking (users get familiar results), 6x faster than skim
- **Trade-off**: No typo tolerance (frizbee has it). Acceptable since AI agents don't make typos

## Rejected Alternatives
- frizbee: 1.8x faster but no Unicode support, younger ecosystem, SIMD-dependent
- fuzzy-matcher (skim): Unmaintained since 2020, 6-10x slower than nucleo
- sublime_fuzzy: Unmaintained since 2020, simpler algorithm
- fff-core: 67 dependencies, edition 2024 conflict (resolved but too heavy)
