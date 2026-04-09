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

**ZERO-INTERACTION CONTRACT**: This pipeline runs fully autonomously. You MUST NOT use `AskUserQuestion` at any point. All decisions are made automatically based on codebase analysis, config, and best-practice defaults. If information is missing, use the best available default — never block on user input.

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

## Quick Dev Path

When `--quick` flag is present OR auto-detected as trivial:

**Auto-detection signals (2+ triggers quick path):**
- Change involves ≤ 2 files
- No new dependencies needed
- Change type: typo, copy, config, small bug fix
- User explicitly says "quick"/"simple"/"small fix"/"trivial"

**Quick path execution:**
1. Skip brainstorm: `$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm`
2. Skip full plan — create epic + single task directly:
   ```bash
   $FLOWCTL epic plan $EPIC_ID --spec "Quick fix: <description>"
   $FLOWCTL task create --epic $EPIC_ID --title "<description>"
   $FLOWCTL phase done --epic $EPIC_ID --phase plan
   ```
3. Skip plan_review: `$FLOWCTL phase done --epic $EPIC_ID --phase plan_review`
4. Work phase: single worker, no Teams mode, no file locking
5. Skip impl_review — run guard only:
   ```bash
   $FLOWCTL guard
   $FLOWCTL phase done --epic $EPIC_ID --phase impl_review
   ```
6. Close: validate + completion (no PR unless requested)

## Two-Level Phase System

This plugin has TWO independent phase systems operating at different levels:

| Level | Phases | Managed by | Scope |
|-------|--------|-----------|-------|
| **Epic-level** | brainstorm → plan → plan_review → work → impl_review → close (6 phases) | `flowctl phase next/done` | One epic's lifecycle |
| **Worker-level** | 1→2→3→5→6→7→10→12 (up to 12 phases, varies by size/flags) | `flowctl worker-phase next/done` | One task within the Work epic-phase |

Epic phases are sequential. Worker phases run INSIDE the epic "work" phase — multiple workers execute their 12 phases in parallel (one per task).

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

**Execute brainstorm** (input is natural language — a new idea).
**This is always AUTO mode — AI self-interview, no AskUserQuestion, no human input.**
1. **Codebase context**: Search for files related to the request (Grep/Glob for key terms), read git log for recent changes, check existing `.flow/` specs for related work
   - Read `.flow/project-context.md` Non-Goals to filter out excluded approaches.
2. **Classify complexity**: Trivial (1-2 files) / Medium (clear feature) / Large (cross-cutting)
3. **Self-interview**: Ask and answer 6-10 Q&A pairs grounded in code evidence. Core questions:
   - Who uses this and what pain point does it solve?
   - What happens if we do nothing?
   - Is there a simpler version that delivers 80% of the value?
   - How does the codebase currently handle similar problems?
   - What other systems/modules will this touch?
   - What can go wrong? What are the boundary conditions?
4. **Approach generation**: Generate 2-3 approaches with Name/Summary/Effort/Risk/Pros/Cons. Auto-select the best approach based on codebase alignment and risk.
4b. **Structured deepening**: Apply the most relevant reasoning method (Pre-mortem for specs, First Principles for architecture, Inversion for refactoring). Append insights to the self-interview trace.
5. **Write requirements doc** to `.flow/specs/${SLUG}-requirements.md` via `$FLOWCTL write-file --path ".flow/specs/${SLUG}-requirements.md" --stdin --json` with: Problem, Users, Chosen Approach, Requirements checklist, Non-Goals, Constraints, Evidence, Self-Interview Trace
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
5. Verify plan does not propose anything listed in Non-Goals or contradict Architecture Decisions from `.flow/project-context.md`.

### Work (work)
1. Find ready tasks: `$FLOWCTL ready $EPIC_ID --json`
2. Start tasks: `$FLOWCTL start <task-id> --json`
3. Lock files: `$FLOWCTL lock --task <task-id> --files "file1,file2" --json`
4. Spawn ALL ready workers in ONE parallel Agent call with isolation worktree and team_name
   - Include task domain in worker prompt (from task JSON `domain` field)
   - Workers auto-load domain-specific skills (frontend→UI engineering, backend→API design, etc.)
5. Wait for workers, merge worktree branches back
6. Mark tasks complete: `$FLOWCTL done <task-id> --summary "what was done" --json`
7. Wave checkpoint: verify done, run guards
8. Repeat waves until no ready tasks remain

### Impl Review (impl_review)
1. Detect review backend: `$FLOWCTL review-backend` (same as plan_review)
2. If backend is "none" or "ASK", skip review and advance with `$FLOWCTL phase done`
3. Generate diff: `git diff main...HEAD`
4. Spawn 3-layer parallel review (see flow-code-code-review skill):
   - Blind Hunter (diff only, no context)
   - Edge Case Hunter (diff + project access)
   - Acceptance Auditor (diff + spec + project-context.md)
5. Merge findings, apply zero-findings rule
6. Fix Critical/Important issues until SHIP (max 2 iterations)
   When a review returns NEEDS_WORK, auto-capture the key findings as memory pitfalls:
   ```bash
   $FLOWCTL memory add pitfall "Review finding: <summary of what was wrong and how it was fixed>"
   ```

### Close (close)
1. Validate: $FLOWCTL validate --epic $EPIC_ID --json
2. Run final guard: `$FLOWCTL guard` (lint + type + test must pass)
3. **Run Quick Commands** from epic spec: `$FLOWCTL cat $EPIC_ID | grep -A20 "## Quick commands"` — execute each listed smoke test. If any fails, fix before shipping.
4. **Verify all task checklists**: For each task in the epic, run `$FLOWCTL checklist verify --task <TASK_ID> --json`. All items must be checked.
5. **Pre-launch checklist** — verify all six dimensions before shipping:
   - **Code quality**: guard passes, no Critical/Important review findings open
   - **Security**: no secrets in code (`grep -rn password\|secret\|api_key`), input validated at boundaries
   - **Performance**: no N+1 queries, list endpoints paginated, images optimized
   - **Accessibility**: keyboard navigable, screen reader compatible, contrast ratios met (frontend changes only)
   - **Infrastructure**: environment variables documented, migrations reversible, feature flags configured
   - **Documentation**: README/CHANGELOG updated if user-facing, API docs match implementation
   - **Non-Goals compliance**: verify changes don't introduce anything listed in `.flow/project-context.md` Non-Goals
   Any failing dimension: fix before proceeding. Skip dimensions not applicable to the change (e.g., skip accessibility for backend-only epics).
6. Mark complete: $FLOWCTL epic completion $EPIC_ID ship --json
7. Push branch and create draft PR (unless --no-pr)

## Recovery

The loop resumes from wherever flowctl says the current phase is:
$FLOWCTL phase next --epic $EPIC_ID --json

## File Writes

**CRITICAL**: Never use the Write or Edit tools for pipeline artifacts (specs, requirements, docs, output files). These tools trigger Claude Code permission prompts and break the zero-interaction contract.

Instead, always use `flowctl write-file` via Bash:
```bash
# Inline content (short)
$FLOWCTL write-file --path ".flow/specs/my-spec.md" --content "# Spec content" --json

# Heredoc via stdin (long content)
cat <<'FLOWEOF' | $FLOWCTL write-file --path "docs/output.md" --stdin --json
# Long document content here
Multiple lines...
FLOWEOF

# Append mode
$FLOWCTL write-file --path "docs/log.md" --content "New entry" --append --json
```

This ensures all file I/O goes through Bash (which is auto-allowed), keeping the pipeline fully autonomous.

**Exception**: Worker agents in worktree isolation MAY use Write/Edit since they run with `bypassPermissions`.

## Guardrails

- Never skip phases. flowctl enforces the sequence.
- Never bypass flowctl phase done. It records evidence.
- Always use flowctl for ALL state operations.
- Workers use worker-phase next/done internally (unchanged).
- Never use Write/Edit tools in the pipeline coordinator — use `$FLOWCTL write-file` instead.
