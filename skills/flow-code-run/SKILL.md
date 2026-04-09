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

**EXECUTION DISCIPLINE**: Every numbered step below is MANDATORY. You MUST execute each Bash command shown. Do NOT skip steps, summarize them, or "note that you would do X". Actually run the command. If a command fails, fix the issue and retry — do not skip it.

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
- --interactive flag to pause at key decisions
- --quick flag for trivial changes
- --no-pr flag to skip draft PR

## Quick Dev Path

When `--quick` flag is present OR auto-detected as trivial (input ≤10 words, contains "fix"/"typo"/"config"/"bump"/"rename"/"simple"/"trivial"):

```bash
# Quick path — skip brainstorm/plan_review/impl_review
$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --json
$FLOWCTL epic plan $EPIC_ID --spec "Quick fix: <description>" --json
$FLOWCTL task create --epic $EPIC_ID --title "<description>" --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --json
# Work: single worker, then guard-only impl_review, then close
```

---

## MANDATORY Startup Sequence

**YOU MUST RUN ALL 5 STEPS. DO NOT SKIP ANY.**

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"

# Step 1: Check .flow/ exists
$FLOWCTL detect --json

# Step 2: Check for interrupted work from previous sessions
$FLOWCTL status --interrupted --json

# Step 3: Load project memory (if enabled)
$FLOWCTL memory inject --json 2>/dev/null || true

# Step 4: Verify review backend availability
$FLOWCTL review-backend --json

# Step 5: Session context
git branch --show-current 2>/dev/null || echo "not a git repo"
$FLOWCTL epics --json 2>/dev/null || true
```

If Step 2 shows interrupted epics, report them and ask whether to resume or start fresh (in --interactive mode) or auto-resume (in default mode).

---

## Step 1: Resolve or Create Epic

```bash
# If input is a Flow ID (fn-N-*):
$FLOWCTL show <id> --json

# If input is natural language:
$FLOWCTL epic create --title "<title>" --json
```

## Step 2: Phase Loop

```bash
# MANDATORY LOOP — execute until all_done is true
while true; do
  PHASE_JSON=$($FLOWCTL phase next --epic $EPIC_ID --json)
  # Parse: if all_done == true, break
  # Otherwise: execute the phase, then:
  $FLOWCTL phase done --epic $EPIC_ID --phase $CURRENT_PHASE --json
done
```

---

## Phase Details

### Brainstorm (brainstorm)

**Skip conditions** (any one → skip):
- Input is a Flow ID, spec file, or --plan-only flag
- Auto-detected as trivial (see Quick Dev Path)

**Otherwise EXECUTE ALL of the following:**

```bash
# MANDATORY Step 1: Codebase context gathering
$FLOWCTL find "<key terms from request>" --json
git log --oneline -10 2>/dev/null || true
ls .flow/specs/ 2>/dev/null || true
cat .flow/project-context.md 2>/dev/null || true
```

**MANDATORY Step 2: Classify complexity** — Output one of: Trivial / Medium / Large

**MANDATORY Step 3: Self-interview** — Read `steps/step-03-self-interview.md` and execute ALL forcing questions for the detected tier. Output EVERY Q&A pair to conversation.

**MANDATORY Step 4: Approach generation** — Read `steps/step-04-approaches.md`. Generate EXACTLY 3 approaches (Narrow/Balanced/Ideal), score them, select highest.

**MANDATORY Step 5: Write requirements doc**
```bash
cat <<'SPECEOF' | $FLOWCTL write-file --path ".flow/specs/${SLUG}-requirements.md" --stdin --json
# Requirements: <title>
## Problem
## Users
## Chosen Approach (Approach X, score Y/55)
## Requirements
## Non-Goals
## Self-Interview Trace
<paste all Q&A pairs here>
## Approach Comparison
<paste scoring table here>
SPECEOF
```

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --json
```

---

### Plan (plan)

**MANDATORY Step 1: Research** — Read `steps/step-02-research.md`. Use intent-level tools:
```bash
# MANDATORY: project overview
$FLOWCTL graph map --json 2>/dev/null || true

# MANDATORY: find related code
$FLOWCTL find "<request key terms>" --json

# MANDATORY: check ADRs and invariants
ls docs/decisions/ADR-*.md 2>/dev/null
$FLOWCTL invariants show --json 2>/dev/null || true
```

**MANDATORY Step 2: Spawn research scouts** — Launch AT LEAST repo-scout in parallel:
```
Agent(subagent_type="Explore", prompt="Research: <what to find>", run_in_background=true)
```

**MANDATORY Step 3: Write spec + create tasks**
```bash
$FLOWCTL epic plan $EPIC_ID --spec "<spec text>" --json
$FLOWCTL task create --epic $EPIC_ID --title "<task 1>" --domain <domain> --json
$FLOWCTL task create --epic $EPIC_ID --title "<task 2>" --deps "$EPIC_ID.1" --json
# ... etc
$FLOWCTL validate --epic $EPIC_ID --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan --json
```

---

### Plan Review (plan_review)

**MANDATORY: Self-review ALWAYS runs (even if review backend is "none").**

Execute ALL 10 forcing questions from the Premise Challenge + Architecture Interrogation. Score /30.

```bash
# MANDATORY: Check review backend
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

If backend is "rp" or "codex" AND the tool is available (verified by review-backend):
- Run external review via RP context_builder or Codex
- Fix issues until SHIP (max 3 iterations)
- **DO NOT SKIP external review if backend is configured and available**

If backend is "none": self-review score is the only gate.

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --json
```

---

### Work (work)

```bash
# MANDATORY Step 1: Find ready tasks
$FLOWCTL ready $EPIC_ID --json
```

**MANDATORY Step 2: For EACH ready task, execute ALL of:**
```bash
# Start the task
$FLOWCTL start <task-id> --json

# Lock files (get file list from task spec)
$FLOWCTL lock --task <task-id> --files "<file1>,<file2>" --json
```

**MANDATORY Step 3: Spawn workers with isolation**

Spawn ALL ready workers in ONE parallel Agent call:
```
Agent(
    prompt="<worker prompt with task spec, domain, project-context>",
    mode="auto",
    isolation="worktree",   # MANDATORY — prevents race conditions
    team_name="epic-workers",
    run_in_background=true
)
```

**CRITICAL**: Workers MUST use `isolation: "worktree"`. Without worktree isolation, parallel workers writing to the same directory have race conditions. This was the #1 Critical finding in the execution audit.

**MANDATORY Step 4: After workers complete**
```bash
# For each completed task:
$FLOWCTL done <task-id> --summary "<what was done>" --json

# MANDATORY: Wave checkpoint — run guard between waves
$FLOWCTL guard
```

**MANDATORY Step 5: Repeat** — Check `$FLOWCTL ready $EPIC_ID --json` for newly unblocked tasks. Loop until no ready tasks remain.

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase work --json
```

---

### Impl Review (impl_review)

**MANDATORY: Self-review ALWAYS runs.**

Execute ALL 10 forcing questions (Correctness 5Q + Quality 5Q). Score /30.

**MANDATORY: Run these commands (not just describe them):**
```bash
# MANDATORY: Run guard
$FLOWCTL guard

# MANDATORY: Check review backend
REVIEW_BACKEND=$($FLOWCTL review-backend)

# MANDATORY: Generate diff for review
git diff main...HEAD --stat 2>/dev/null || git diff HEAD~5...HEAD --stat
```

If backend is "rp" or "codex" AND available:
- Run 3-layer parallel review (Blind Hunter + Edge Case Hunter + Acceptance Auditor)
- Apply zero-findings rule
- Fix Critical/Important issues (max 2 iterations)
- **DO NOT SKIP if backend is configured**

When NEEDS_WORK:
```bash
$FLOWCTL memory add --type pitfall --epic $EPIC_ID "Review: <finding summary>"
```

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --json
```

---

### Close (close)

**MANDATORY: Execute EVERY step. Do NOT skip any.**

```bash
# MANDATORY Step 1: Validate
$FLOWCTL validate --epic $EPIC_ID --json

# MANDATORY Step 2: Run guard
$FLOWCTL guard

# MANDATORY Step 3: Run Quick Commands (if present in spec)
$FLOWCTL cat $EPIC_ID 2>/dev/null | grep -A20 "## Quick commands" || true

# MANDATORY Step 4: Verify checklists
# For each task: $FLOWCTL checklist verify --task <TASK_ID> --json
```

**MANDATORY Step 5: Ship-Readiness Interrogation (7 questions)**

Execute ALL 7 questions. Score /21. Do NOT ship if score <14.

```bash
# MANDATORY: Security check
grep -rn 'password\|secret\|api_key\|token' <changed-files> || echo "clean"

# MANDATORY: Impact check
$FLOWCTL graph impact <main-changed-file> --json 2>/dev/null || true

# MANDATORY: ADR compliance
ls docs/decisions/ADR-*.md 2>/dev/null
$FLOWCTL invariants check --json 2>/dev/null || true
```

**MANDATORY Step 6: Documentation** — If user-facing changes exist, update README/CHANGELOG.

**MANDATORY Step 7: Ship**
```bash
$FLOWCTL epic completion $EPIC_ID ship --json

# MANDATORY: Push and create PR (unless --no-pr)
git push origin HEAD 2>/dev/null || true
# Create draft PR if on a feature branch
```

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase close --json
```

---

## Recovery

```bash
$FLOWCTL phase next --epic $EPIC_ID --json
# Resumes from current phase
```

## File Writes

**CRITICAL**: Never use Write or Edit tools for pipeline artifacts. Use `$FLOWCTL write-file` via Bash.

```bash
cat <<'FLOWEOF' | $FLOWCTL write-file --path "path/to/file.md" --stdin --json
Content here
FLOWEOF
```

**Exception**: Workers in worktree isolation MAY use Write/Edit (bypassPermissions).

## Guardrails

- **NEVER skip phases** — flowctl enforces sequence
- **NEVER skip mandatory commands** — run every `$FLOWCTL` command shown above
- **NEVER skip self-review questions** — execute all forcing questions per phase
- **NEVER skip worktree isolation** — workers MUST use `isolation: "worktree"`
- **NEVER skip guard** — run `$FLOWCTL guard` at work wave checkpoints AND close
- **NEVER fake command output** — actually run the command and use its real output
