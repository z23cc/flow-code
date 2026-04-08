---
name: flow-code-go
description: "Full autopilot pipeline: auto-brainstorm → plan → plan-review → work → impl-review → close. Zero human input from idea to PR."
tier: 3
user-invocable: true
---

# Flow Code Go

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.
<!-- SKILL_TAGS: workflow,pipeline,autopilot -->

Full autopilot. Takes a raw idea and drives it through every phase — brainstorm, plan, work, review, close — with zero human input. Produces a draft PR at the end.

**CRITICAL: flowctl is BUNDLED.** Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Input

Arguments: $ARGUMENTS

Accepts:
- Idea or problem description in natural language: "Add OAuth login"
- `--no-pr` flag: skip PR creation at close
- `--plan-only` flag: stop after plan phase (skip brainstorm, create epic directly, plan only, no work)

Does NOT accept:
- Flow IDs (use `/flow-code:run fn-N` to resume an existing epic)
- File paths (use `/flow-code:run spec.md` to plan from a spec)

If input is empty, ask: "What should I build? Describe the idea in 1-5 sentences."

If input looks like a Flow ID (fn-N pattern), redirect:
> "Use `/flow-code:run <id>` to resume an existing epic. `/flow-code:go` starts from scratch."

## Early Exit: --plan-only

If `--plan-only` is set, **skip Phase 0 (brainstorm) entirely**. Instead:
1. Create epic directly: `$FLOWCTL epic create --title "<idea>" --json`
2. Jump to Phase 1 (Plan) with research scouts
3. Stop after plan completes. Do NOT enter work phase.

Rationale: `--plan-only` means the user wants structured tasks, not spec refinement. Brainstorming would waste tokens.

## Pipeline Overview

```
Phase 0: Auto-Brainstorm ─── AI self-interview, requirements doc
                │
                ▼
Phase 1: Plan ─────────────── Research scouts, epic + tasks
                │
                ▼
Phase 2: Plan Review ──────── Cross-model validation (if backend available)
                │
                ▼
Phase 3: Work ─────────────── Parallel workers, wave checkpoints
                │
                ▼
Phase 4: Impl Review ──────── Adversarial review (if backend available)
                │
                ▼
Phase 5: Close ────────────── Validate, push, draft PR
```

All phases are autonomous. No `AskUserQuestion` calls anywhere in the pipeline.

## Phase 0: Auto-Brainstorm

This is what distinguishes `/flow-code:go` from `/flow-code:run`. The AI refines the raw idea into a structured requirements doc before planning.

### Step 0.1: Codebase Context Gathering

1. Search for files related to the request (Grep/Glob for key terms)
2. Read git log for recent changes in relevant areas
3. Check existing `.flow/` specs/epics for related work
4. Identify affected modules, dependencies, and integration points

### Step 0.2: Complexity Assessment

Classify:
- **Trivial** (1-2 files, clear fix): skip self-interview. Create epic directly and jump to Phase 1:
  ```bash
  $FLOWCTL epic create --title "<idea>" --json
  ```
  Capture `EPIC_ID`. Skip Steps 0.3-0.5. Go straight to Phase 1 (Plan).
- **Medium** (clear feature, moderate scope): 6 Q&A self-interview + 2 approaches.
- **Large** (cross-cutting, vague): 10+ Q&A self-interview + 3 approaches + risk matrix.

Report tier in one sentence.

### Step 0.3: Self-Interview (Medium/Large only)

Ask and answer questions internally. Output each as a visible block so the user can review the reasoning later:

```
### Q: <question>
**A:** <answer grounded in code evidence>
```

**Core questions (always):**

1. **Problem & Users** — Who is affected? What's the pain point? (Derive from code: who calls the affected area, what user-facing behavior)
2. **Cost of Inaction** — What breaks or degrades if we do nothing? (Check error patterns, open issues, tech debt)
3. **Simpler Framing** — Is there an 80% version? What's the minimum viable change? (Analyze the request for deferrable parts)
4. **Existing Patterns** — How does the codebase handle similar problems? (Cite specific files and functions)
5. **Integration Points** — What modules/APIs/schemas are affected? What contracts must be preserved?
6. **Edge Cases & Failure Modes** — What can go wrong? Boundary conditions? Concurrency risks?

**Extended questions (Large only):**

7. **Performance Impact** — Hot paths, data volume, caching affected?
8. **Security Surface** — Auth, input validation, data handling changes?
9. **Migration & Compatibility** — Breaking changes? Data migration needed?
10. **Testing Strategy** — Coverage gaps in affected area?

**Adaptive**: If any answer reveals unexpected complexity, add 1-2 follow-up Q&A. Cap at 15 total.

### Step 0.4: Approach Selection

Generate 2-3 approaches:

| Field | Format |
|-------|--------|
| **Name** | Short label |
| **Summary** | One sentence |
| **Effort** | S / M / L |
| **Risk** | Low / Med / High |
| **Pros** | 2-3 bullets |
| **Cons** | 2-3 bullets |

**Auto-select** the best approach:
1. Aligns with existing codebase patterns
2. Lowest risk for the effort level
3. Maximizes code reuse

Output: "**Selected: Approach N** — <reason>"

If approaches are close, note it but still pick one (no blocking on human).

### Step 0.5: Write Requirements Doc

```bash
SLUG=$(echo "$IDEA" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-' | head -c 40)
mkdir -p .flow/specs
```

Write to `.flow/specs/${SLUG}-requirements.md`:

```markdown
# Requirements: <Title>

## Problem
<derived from self-interview>

## Users
<who is affected>

## Chosen Approach
<name + summary>

## Requirements
- [ ] Requirement 1
- [ ] Requirement 2
...

## Non-Goals
- Explicitly excluded items

## Constraints
- Technical/codebase constraints discovered

## Evidence
- `path/to/file.rs:42` — what it shows
- `path/to/other.rs` — pattern found

## Open Questions
- Items for plan-phase research scouts to resolve
```

### Step 0.6: Create Epic

```bash
$FLOWCTL epic create --title "<title from requirements>" --json
$FLOWCTL epic plan <epic-id> --file .flow/specs/${SLUG}-requirements.md --json
```

Capture `EPIC_ID` for subsequent phases.

Report: "Brainstorm complete. Requirements: `.flow/specs/<slug>-requirements.md`. Epic: <id>. Entering plan phase."

If `--plan-only` is set, note that work will not execute.

## Phases 1-5: Delegate to Run Pipeline

After Phase 0 creates the epic, the remaining phases follow the exact same logic as `/flow-code:run`. Execute them inline (do NOT invoke the run skill — that would reset context).

### Phase 1: Plan

1. Spawn research scouts in parallel (repo-scout, context-scout, practice-scout)
   - Scouts receive the requirements doc as context (not just the raw idea)
2. Enrich epic spec with scout findings via `$FLOWCTL epic plan`
3. Create tasks via `$FLOWCTL task create` with dependencies
4. Validate: `$FLOWCTL validate --epic $EPIC_ID --json`
5. `$FLOWCTL phase done --epic $EPIC_ID --phase plan --json`

If `--plan-only`: stop here. Report epic ID and task list.

### Phase 2: Plan Review

1. Detect review backend: `$FLOWCTL review-backend`
2. If backend available: run review, fix until SHIP (max 3 iterations)
3. If no backend: skip and advance
4. `$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --json`

### Phase 3: Work

1. Find ready tasks: `$FLOWCTL ready $EPIC_ID --json`
2. Start tasks: `$FLOWCTL start <task-id> --json`
3. Lock files: `$FLOWCTL lock --task <id> --files "<files>"`
4. Spawn ALL ready workers in ONE parallel Agent call with `isolation: "worktree"` and `team_name`
5. Wait for workers, merge worktree branches back
6. Wave checkpoint: verify done, run guards
7. Repeat waves until no ready tasks remain
8. `$FLOWCTL phase done --epic $EPIC_ID --phase work --json`

### Phase 4: Impl Review

1. Run adversarial review via Codex or RP (if available)
2. Fix issues until SHIP (max 2 iterations)
3. If no review backend: skip and advance
4. `$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --json`

### Phase 5: Close

1. Validate: `$FLOWCTL validate --epic $EPIC_ID --json`
2. Run final guard if configured
3. Mark complete: `$FLOWCTL epic completion $EPIC_ID ship --json`
4. Push branch and create draft PR (unless `--no-pr`)
5. `$FLOWCTL phase done --epic $EPIC_ID --phase close --json`

## Recovery

If the session is interrupted at any point:
- The epic and its phase state are persisted in `.flow/`
- Resume with `/flow-code:run <epic-id>` (phases 1-5 are identical)
- Phase 0 output is already written to `.flow/specs/` — won't be lost

## Guardrails

- **Zero human input**: No `AskUserQuestion` calls in any phase. All decisions are AI-driven.
- **Never skip phases**: flowctl enforces the sequence (except Phase 0 which runs before flowctl phases).
- **Evidence-grounded**: Phase 0 answers must cite code. Speculation without file references is a red flag.
- **Circuit breakers**: Plan review max 3 iterations, impl review max 2 iterations. Prevents infinite loops.
- **Trivial bypass**: If Phase 0 classifies as trivial, skip brainstorm — don't waste tokens on obvious changes.

## When to Use What

| Command | When |
|---------|------|
| `/flow-code:go "idea"` | Full autopilot from raw idea to PR. No human input. |
| `/flow-code:run "idea"` | Plan → work → close. No brainstorm. Human may intervene. |
| `/flow-code:run fn-N` | Resume existing epic from current phase. |
| `/flow-code:brainstorm "idea"` | Just brainstorm. Produces requirements doc, stops. |
| `/flow-code:brainstorm --auto "idea"` | Just auto-brainstorm. Same output, no questions. |
| `/flow-code:interview fn-N` | Interactive deep Q&A to refine existing spec. |
