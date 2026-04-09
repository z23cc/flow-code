---
id: ADR-004
status: accepted
date: 2026-04-09
tags: [core, state-machine]
verify: "grep -c 'Todo\\|InProgress\\|Done\\|Blocked\\|Skipped\\|Failed\\|UpForRetry\\|UpstreamFailed' flowctl/crates/flowctl-core/src/state_machine.rs | grep -q '^[8-9]\\|^[1-9][0-9]'"
scope: "flowctl/crates/flowctl-core/src/state_machine.rs"
---
# ADR-004: Eight-State Task Machine

## Status
ACCEPTED

## Date
2026-04-09

## Context
Tasks need richer lifecycle than 3-state (pending/running/done) to support autonomous recovery. Workers can timeout, upstream tasks can fail, and users may want to skip tasks without breaking the DAG.

## Decision
8 states: `todo`, `in_progress`, `done`, `blocked`, `skipped`, `failed`, `up_for_retry`, `upstream_failed`. State transitions are formally validated in `state_machine.rs` — illegal transitions are compile-time errors.

## Consequences
- **Constraint**: New states require updating ALL match branches across the codebase
- **Benefit**: Worker timeout → `failed` → automatic `upstream_failed` propagation to downstream tasks
- **Benefit**: `skipped` tasks satisfy downstream dependencies (DAG continues)
- **Trade-off**: 8 states is more complex than 3, but enables autonomous recovery without human intervention

## Rejected Alternatives
- 3-state (pending/running/done): Cannot express blocked/failed, no recovery path
- 5-state (add blocked + failed): Missing skipped and cascade propagation
