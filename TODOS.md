# TODOS

## MCP Service Layer Extraction
**What:** Extract shared service functions from CLI commands that MCP, daemon API, and CLI can all call.
**Why:** Currently MCP shells out to CLI (full process fork per call), and daemon handlers duplicate logic at a lower level (just DB updates, no dep checks/state machine/evidence). These are two incompatible execution paths.
**Context:** CLI commands (workflow.rs cmd_start/cmd_done) contain business logic: dependency checks, assignee enforcement, --force semantics, runtime state, evidence persistence, Markdown export. Daemon handlers (handlers.rs) only do repo.update_status(). MCP (mcp.rs) forks the full CLI process. The fix is to extract the business logic into Result-returning service functions (no error_exit), then have CLI/MCP/daemon all call them. This is the prerequisite for CLI→daemon HTTP unification (P3 in the original design review).
**Effort:** XL (human: ~2 weeks / CC: ~2 hours)
**Priority:** P2
**Depends on:** File splits (admin/task/workflow) should happen first to reduce diff size.

## Leptos SSR Integration Cleanup
**What:** Move Leptos SSR fallback routing from main.rs to server.rs.
**Why:** Cosmetic separation of concerns. main.rs should only do CLI parsing, server.rs should own all HTTP routing.
**Context:** Currently main.rs:534-539 has the Leptos fallback logic inline. server.rs already has build_router() and serve_tcp(). No correctness issue, just organizational.
**Effort:** S (human: ~1 hour / CC: ~5 min)
**Priority:** P3
**Depends on:** Nothing. Revisit when server.rs grows beyond 200 lines.
