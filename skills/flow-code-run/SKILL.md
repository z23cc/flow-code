---
name: flow-code-run
description: Unified entry point for plan-first development. Manages the entire pipeline (plan, plan-review, work, impl-review, close) via flowctl phase commands.
tier: 3
user-invocable: true
---

# Flow Code Run

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

Unified pipeline entry point. Drives the entire development lifecycle through flowctl phase next/done.

**CRITICAL: flowctl is BUNDLED.** Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Input

Arguments: $ARGUMENTS

Accepts:
- Feature description: "Add OAuth login"
- Flow epic ID: fn-1-add-oauth (resume existing epic)
- --plan-only flag to stop after planning

## Phase Loop

Claude is the outer loop; flowctl provides phase content.

### Step 1: Resolve or Create Epic

If input is a Flow ID (fn-N-*): read with $FLOWCTL show <id> --json
If input is text: create with $FLOWCTL epic create --title "<title>" --json

### Step 2: Enter Phase Loop

Loop until all phases complete:
1. Run $FLOWCTL phase next --epic $EPIC_ID --json
2. If all_done is true, break
3. Execute the current phase (see Phase Details)
4. Run $FLOWCTL phase done --epic $EPIC_ID --phase $PHASE --json
5. Repeat

## Phase Details

### Plan (plan)
1. Spawn research scouts in parallel (repo-scout, context-scout, practice-scout)
2. Write epic spec via $FLOWCTL epic plan
3. Create tasks via $FLOWCTL task create with dependencies
4. Validate: $FLOWCTL validate --epic $EPIC_ID --json

### Plan Review (plan_review)
1. Detect review backend: $FLOWCTL review-backend
2. Run review via RP context_builder or Codex
3. Fix issues until SHIP verdict (max 3 iterations)
4. If backend is none, skip and advance

### Work (work)
1. Find ready tasks: $FLOWCTL ready --epic $EPIC_ID --json
2. Start tasks: $FLOWCTL start <task-id> --json
3. Lock files: $FLOWCTL lock --task <id> --files "<files>"
4. Spawn ALL ready workers in ONE parallel Agent call with isolation worktree and team_name
5. Wait for workers, merge worktree branches back
6. Wave checkpoint: verify done, run guards
7. Repeat waves until no ready tasks remain

### Impl Review (impl_review)
1. Run adversarial review via Codex or RP
2. Fix issues until SHIP (max 2 iterations)
3. If no review backend, skip and advance

### Close (close)
1. Validate: $FLOWCTL validate --epic $EPIC_ID --json
2. Run final guard if configured
3. Mark complete: $FLOWCTL epic completion $EPIC_ID ship --json
4. Push branch and create draft PR (unless --no-pr)

## Recovery

The loop resumes from wherever flowctl says the current phase is:
$FLOWCTL phase next --epic $EPIC_ID --json

## Guardrails

- Never skip phases. flowctl enforces the sequence.
- Never bypass flowctl phase done. It records evidence.
- Always use flowctl for ALL state operations.
- Workers use worker-phase next/done internally (unchanged).
