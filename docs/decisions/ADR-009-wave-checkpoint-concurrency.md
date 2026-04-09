---
id: ADR-009
status: accepted
date: 2026-04-09
tags: [orchestration, concurrency]
verify: "grep -q 'fs2' flowctl/Cargo.toml && grep -q 'lock' flowctl/crates/flowctl-cli/src/main.rs"
scope: "flowctl/crates/flowctl-core/src/"
---
# ADR-009: Wave-Checkpoint-Wave Concurrency Model

## Status
ACCEPTED

## Date
2026-04-09

## Context
Multiple tasks within an epic can execute in parallel (Teams mode). Need to prevent file conflicts and ensure integration quality between waves.

## Decision
Wave-Checkpoint-Wave: ready tasks (no unresolved deps) spawn as parallel Agent workers. Each wave ends with a checkpoint: aggregate results, run guard, verify integration, unlock files, detect newly unblocked tasks for the next wave. File locking via `flowctl lock/unlock` with stale lock recovery.

## Consequences
- **Constraint**: Workers MUST acquire file locks before editing, release on completion
- **Benefit**: True parallel execution with conflict prevention
- **Benefit**: Stale lock recovery at wave start prevents deadlocks from crashed workers
- **Trade-off**: Wave boundaries add latency (checkpoint takes ~5-10s). Acceptable vs conflict resolution cost

## Rejected Alternatives
- Sequential execution: Safe but slow (1 task at a time)
- Optimistic concurrency (no locks): Fast but risks merge conflicts
- Fine-grained line-level locking: Too complex, diminishing returns
