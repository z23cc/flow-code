# TODOS

## MCP Service Layer Extraction
**What:** Extract shared service functions from CLI commands that MCP, daemon API, and CLI can all call.
**Why:** Currently MCP shells out to CLI (full process fork per call), and daemon handlers duplicate logic at a lower level (just DB updates, no dep checks/state machine/evidence). These are two incompatible execution paths.
**Context:** CLI commands (workflow.rs cmd_start/cmd_done) contain business logic: dependency checks, assignee enforcement, --force semantics, runtime state, evidence persistence, Markdown export. Daemon handlers (handlers.rs) only do repo.update_status(). MCP (mcp.rs) forks the full CLI process. The fix is to extract the business logic into Result-returning service functions (no error_exit), then have CLI/MCP/daemon all call them. This is the prerequisite for CLI→daemon HTTP unification (P3 in the original design review).
**Effort:** XL (human: ~2 weeks / CC: ~2 hours)
**Priority:** P2
**Depends on:** File splits (admin/task/workflow) should happen first to reduce diff size.
**Note:** The Web Platform project (React frontend + daemon POST API) partially addresses this -- the POST API endpoints will call service layer functions, establishing the pattern for full extraction.

## Create DESIGN.md
**What:** Create a formal design system document for the flow-code web platform.
**Why:** No DESIGN.md exists. Design decisions are scattered across CEO plan and design review notes. A formal document ensures consistency for contributors.
**Context:** Design review rated the project 4/10 → 7/10 on design completeness. Key design tokens (colors, spacing, typography) were defined during review but not persisted in a DESIGN.md. Run /design-consultation to create it.
**Effort:** S (human: ~2 hours / CC: ~15 min)
**Priority:** P3
**Depends on:** Web platform implementation (so the design system reflects actual usage)
