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
- --interactive flag to pause at key decision points (brainstorm result, plan review, impl review)

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

**Auto-skip brainstorm** (detected as trivial — any ONE signal triggers skip):
- Input is 10 words or fewer
- Input contains: "fix", "typo", "config", "update", "bump", "rename", "simple", "trivial", "small", "minor"
- Input references a specific file path
- `--quick` flag is present

When auto-skipped: create epic + single task directly, skip to plan phase.

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

**Self-review first** (always runs, even if external review is "none"):

#### Premise Challenge (4 forcing questions)
1. **Right problem?** — Could a different framing yield a simpler/more impactful solution?
   - Reject: "The spec says X" (spec might be wrong). Accept: Evidence the problem is correctly framed.
2. **Do-nothing test** — What SPECIFICALLY breaks in 30 days if we ship nothing?
   - Reject: "Tech debt grows." Accept: Measurable degradation or blocked feature.
3. **Existing code** — Does the plan reuse what already exists? Run `flowctl graph map --json` and check.
   - Reject: "We checked." Accept: Specific files/functions cited as reused or ruled out.
4. **Non-Goals compliance** — Read `.flow/project-context.md` Non-Goals and ADRs. Does the plan violate any?
   - If yes: flag and fix before proceeding.

#### Architecture Interrogation (6 forcing questions)
5. **Data flow completeness** — For every data path: what happens on happy / nil / empty / error?
   - Reject: Only happy path described. Accept: All 4 paths addressed.
6. **Coupling analysis** — What components become coupled that weren't before?
7. **Scaling characteristics** — What breaks first at 10x load?
8. **Rollback posture** — If this breaks production, how do we undo? (git revert? feature flag? migration rollback?)
9. **Security surface** — New auth boundaries, API surfaces, data access patterns?
10. **Task sizing** — Is every task M-sized (3-5 files)? Any L tasks that should be split?

#### Verdict
Score each question 1-3 (1=concern, 2=acceptable, 3=solid). Total /30.
- **25-30**: SHIP the plan
- **18-24**: Fix flagged issues, then SHIP
- **<18**: MAJOR_RETHINK — plan needs significant revision

Then, if external review backend is configured (rp/codex), also run that. Max 3 iterations total.

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

**Self-review first** (always runs):

#### Correctness Interrogation (5 forcing questions)
1. **Spec fidelity** — Re-read every acceptance criterion. For each: is it MET, PARTIAL, or NOT_MET? Evidence?
   - Reject: "All criteria met." Accept: Each criterion cited with specific file:line proof.
2. **Error path coverage** — For every new function/endpoint: what happens when input is nil? empty? malformed? unauthorized?
   - Reject: "Errors are handled." Accept: Specific error type → response code → user message mapped.
3. **Edge cases** — What are the 3 inputs that would break this code? (empty string, max int, concurrent access, Unicode, null)
   - Must be specific to THIS diff, not generic.
4. **Regression risk** — Did any existing test break? Run `$FLOWCTL guard`. Any new code paths without tests?
5. **Impact verification** — `flowctl graph impact <changed-files>` — are all impacted files still working?

#### Quality Interrogation (5 forcing questions)
6. **Dead code** — Any commented-out code, unused imports, TODO without ticket?
7. **Naming & readability** — Would a new developer understand each function name and variable without the PR context?
8. **Performance** — Any N+1 queries, unbounded loops, missing pagination, large allocations in hot paths?
9. **Security** — Input validated at boundaries? Secrets not in code? SQL parameterized? Auth checked?
10. **Consistency** — Does the code follow project-context.md Critical Rules and existing patterns? (`flowctl find "<pattern>" --json`)

#### Verdict
Score each question 1-3. Total /30.
- **25-30**: SHIP
- **18-24**: NEEDS_WORK — fix flagged issues
- **<18**: MAJOR_RETHINK

Then, spawn 3-layer parallel review (Blind Hunter + Edge Case Hunter + Acceptance Auditor) for external perspective. Apply zero-findings rule. Max 2 iterations.

When NEEDS_WORK, auto-capture pitfalls:
```bash
$FLOWCTL memory add --type pitfall --epic $EPIC_ID "Review finding: <what was wrong and how fixed>"
```

### Close (close)

1. Validate: `$FLOWCTL validate --epic $EPIC_ID --json`
2. Run final guard: `$FLOWCTL guard`
3. Run Quick Commands from epic spec: `$FLOWCTL cat $EPIC_ID | grep -A20 "## Quick commands"`
4. Verify all task checklists: `$FLOWCTL checklist verify --task <TASK_ID> --json` for each task.

#### Ship-Readiness Interrogation (7 forcing questions)

5. **Code quality** — Does `$FLOWCTL guard` pass? Any Critical/Important review findings still open?
   - Reject: "Guard passed" without running it. Accept: Guard output showing pass/skip/0 fail.

6. **Security** — `grep -rn 'password\|secret\|api_key\|token' <changed-files>` — any secrets in code?
   - Reject: "No secrets." Accept: Grep output confirming clean, or justified exceptions documented.

7. **Regression** — Did the FULL test suite pass? Any existing tests modified or deleted?
   - Reject: "Tests pass." Accept: Specific test count and any modifications explained.

8. **Impact verification** — `flowctl graph impact <changed-files> --json` — are all dependent files still functional?
   - If impact list is non-empty: verify each affected module still works.

9. **Non-Goals compliance** — Read `.flow/project-context.md` Non-Goals. Does the diff introduce anything explicitly excluded?
   - Check ADRs: `ls docs/decisions/ADR-*.md` — does the change violate any accepted ADR?

10. **Documentation** — If user-facing changes: README/CHANGELOG/API docs updated?
    - If no user-facing changes: confirm and skip.

11. **Rollback plan** — If this breaks production, how do we undo?
    - Accept: "git revert" / "feature flag" / "migration has down step"
    - Reject: "We'll fix forward" without a concrete plan.

#### Verdict
Score each question 1-3. Total /21.
- **18-21**: SHIP — proceed to push + PR
- **14-17**: Fix flagged items first
- **<14**: Do NOT ship. Fix critical issues.

12. Mark complete: `$FLOWCTL epic completion $EPIC_ID ship --json`
13. Push branch and create draft PR (unless --no-pr)

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
