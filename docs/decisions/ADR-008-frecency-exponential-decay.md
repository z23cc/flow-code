---
id: ADR-008
status: accepted
date: 2026-04-09
tags: [search, memory]
verify: "grep -q 'HALF_LIFE_DAYS' flowctl/crates/flowctl-core/src/frecency.rs"
scope: "flowctl/crates/flowctl-core/src/frecency.rs"
---
# ADR-008: Frecency with 14-Day Exponential Decay

## Status
ACCEPTED

## Date
2026-04-09

## Context
File search needs smart ranking beyond fuzzy score. Files recently/frequently modified are more relevant. Mozilla Firefox's frecency algorithm (2008) is the proven approach, used by fff.nvim and telescope.nvim.

## Decision
Exponential decay with 14-day half-life: `new_score = old_score * 0.5^(days/14) + weight`. Weights: git-modified=3.0, recently-opened=2.0, normal=1.0. Stored as JSON in `.flow/frecency.json`. Auto-tracked on `flowctl done`.

## Consequences
- **Constraint**: Half-life is 14 days (not configurable yet). Weight values are hardcoded constants
- **Benefit**: After ~50 task completions, search results are noticeably better ranked
- **Trade-off**: JSON storage limits scale to ~10K entries (sufficient for any single project)

## Rejected Alternatives
- LMDB (fff-core approach): Heavy dependency, multi-process locking complexity
- Sliding window: Doesn't compress well, needs full history
- No frecency (fuzzy score only): Misses "this file was just modified" signal
