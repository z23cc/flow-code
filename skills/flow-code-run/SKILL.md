---
name: flow-code-run
description: Internal pipeline engine. Manages the entire pipeline (brainstorm, plan, plan-review, work, impl-review, close) via flowctl phase commands. Invoked by /flow-code:go.
tier: 3
user-invocable: false
---

# Flow Code Run

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.
<!-- SKILL_TAGS: workflow,pipeline,planning -->

Internal pipeline engine. Drives the entire development lifecycle (brainstorm, plan, work, review, close) through flowctl phase next/done. Invoked by `/flow-code:go`.

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

### Brainstorm (brainstorm)

Detect input type to decide whether to execute or skip:

**Skip brainstorm** (input is a Flow ID like `fn-N-*`, a spec file path, or `--plan-only` flag):
1. Run: `$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --json`
2. Proceed to Plan phase

**Execute brainstorm** (input is natural language — a new idea):
1. **Codebase context**: Search for files related to the request (Grep/Glob for key terms), read git log for recent changes, check existing `.flow/` specs for related work
2. **Classify complexity**: Trivial (1-2 files) / Medium (clear feature) / Large (cross-cutting)
3. **Self-interview**: Ask and answer 6-10 Q&A pairs grounded in code evidence. Core questions:
   - Who uses this and what pain point does it solve?
   - What happens if we do nothing?
   - Is there a simpler version that delivers 80% of the value?
   - How does the codebase currently handle similar problems?
   - What other systems/modules will this touch?
   - What can go wrong? What are the boundary conditions?
4. **Approach generation**: Generate 2-3 approaches with Name/Summary/Effort/Risk/Pros/Cons. Auto-select the best approach based on codebase alignment and risk.
5. **Write requirements doc** to `.flow/specs/${SLUG}-requirements.md` with: Problem, Users, Chosen Approach, Requirements checklist, Non-Goals, Constraints, Evidence, Self-Interview Trace
6. Run: `$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --json`

### Plan (plan)
1. Spawn research scouts in parallel (repo-scout, context-scout, practice-scout)
2. Write epic spec via `$FLOWCTL epic plan $EPIC_ID --spec "..." --json` (ID is positional, not a flag)
3. Create tasks via `$FLOWCTL task create --epic $EPIC_ID --title "..." --deps "task1,task2" --json` (use `--deps` for dependencies, `--epic` is required)
4. Validate: $FLOWCTL validate --epic $EPIC_ID --json

### Plan Review (plan_review)
1. Detect review backend: `$FLOWCTL review-backend` (returns "rp", "codex", "none", or "ASK")
2. If backend is "none" or "ASK", skip review and advance with `$FLOWCTL phase done`
3. Otherwise run review via RP context_builder or Codex
4. Fix issues until SHIP verdict (max 3 iterations)

### Work (work)
1. Find ready tasks: `$FLOWCTL ready $EPIC_ID --json`
2. Start tasks: `$FLOWCTL start <task-id> --json`
3. Lock files: `$FLOWCTL lock --task <task-id> --files "file1,file2" --json`
4. Spawn ALL ready workers in ONE parallel Agent call with isolation worktree and team_name
   - Include task domain in worker prompt (from task JSON `domain` field)
   - Frontend-domain tasks: worker auto-loads `flow-code-frontend-ui` skill
5. Wait for workers, merge worktree branches back
6. Mark tasks complete: `$FLOWCTL done <task-id> --summary "what was done" --json`
7. Wave checkpoint: verify done, run guards
8. Repeat waves until no ready tasks remain

### Impl Review (impl_review)
1. Detect review backend: `$FLOWCTL review-backend` (same as plan_review)
2. If backend is "none" or "ASK", skip review and advance with `$FLOWCTL phase done`
3. Otherwise run adversarial review via Codex or RP
4. Fix issues until SHIP (max 2 iterations)

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
