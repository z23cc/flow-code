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
# Quick path — use --no-gate to skip evidence requirements
$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --json
$FLOWCTL epic plan $EPIC_ID --spec "Quick fix: <description>" --json
$FLOWCTL task create --epic $EPIC_ID --title "<description>" --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --no-gate --json
# Work: single worker, then:
$FLOWCTL guard
$FLOWCTL phase done --epic $EPIC_ID --phase work --guard-ran --json
$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --no-gate --json
$FLOWCTL phase done --epic $EPIC_ID --phase close --guard-ran --json
```

---

## MANDATORY Startup Sequence

**YOU MUST RUN ALL 6 STEPS. DO NOT SKIP ANY.**

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"

# Step 0: Version check — detect stale plugin cache
FLOWCTL_VERSION=$($FLOWCTL --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
PLUGIN_VERSION=$(cat "${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-.}}/.claude-plugin/plugin.json" 2>/dev/null | grep -oE '"version":\s*"[^"]+"' | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')
if [ -n "$PLUGIN_VERSION" ] && [ "$FLOWCTL_VERSION" != "$PLUGIN_VERSION" ]; then
  echo "WARNING: flowctl $FLOWCTL_VERSION != plugin $PLUGIN_VERSION — skill cache may be stale. Run /flow-code:update"
fi

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

**EXTERNAL REVIEW — DO NOT SELF-REVIEW. Send questions to RP.**

AI self-review failed 5 consecutive audits (scores fabricated). All review questions MUST be answered by RP context_builder (external research model), NOT by the implementing agent.

**PR1: Send 10 forcing questions to RP**

```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

If backend is "rp" (RP available), call `mcp__RepoPrompt__context_builder` with:
```
instructions: "Answer each question about this plan with evidence from the codebase. Score each 1-3 (1=concern, 2=acceptable, 3=solid). Cite specific files.

Plan spec: <paste epic spec>
Tasks: <paste task list>

Questions:
Q1(Right problem): Could different framing yield simpler solution?
Q2(Do-nothing): What SPECIFICALLY breaks in 30 days if we ship nothing?
Q3(Existing code): Does plan reuse existing code? Cite files.
Q4(Non-Goals): Check .flow-config/project-context.md Non-Goals + ADRs. Violations?
Q5(Data flow): For each data path — happy/nil/empty/error?
Q6(Coupling): What components become coupled that weren't before?
Q7(Scaling): What breaks first at 10x load?
Q8(Rollback): If this breaks production, how to undo?
Q9(Security): New auth boundaries, API surfaces?
Q10(Task sizing): All tasks M-sized (3-5 files)?

Format each answer as: Q[N]([topic]):[score] [evidence]"

response_type: "review"
```

If backend is "none": run self-review as fallback (answer questions yourself, but with the understanding that self-review is weaker).

**PR2: Extract scores from RP response and pass to flowctl**

Parse RP's response for Q1:N through Q10:N scores. Compute total.

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --score TOTAL_SCORE --evidence "<paste RP's full response here>" --json
```

The evidence field contains RP's analysis, not your own. flowctl validates ≥200 chars + ≥5 question references.

---

### Work (work)

**⚠️ AUDIT FAILURE HISTORY: This phase failed compliance audit TWICE (fn-5 and fn-6) on the SAME 3 issues. If you skip these again, you are knowingly producing unsafe output.**

**Failure 1 (CRITICAL): Workers ran without worktree isolation — 6 parallel agents wrote to same directory, risking file corruption.**
**Failure 2 (HIGH): No file locking — concurrent edits to settings.py and requirements.txt.**
**Failure 3 (HIGH): No wave checkpoint guard — broken code passed silently.**

---

**W1: Find + start + lock ready tasks**
```bash
READY_JSON=$($FLOWCTL ready $EPIC_ID --json)
# For EACH task in ready list:
$FLOWCTL start TASK_ID --json
$FLOWCTL lock --task TASK_ID --files "file1,file2" --json
```

**W2: Spawn workers — WORKTREE ISOLATION IS NON-NEGOTIABLE**

When calling the Agent tool for each worker, you MUST include these parameters:
- `isolation` parameter set to `"worktree"` — creates a separate git worktree copy
- `mode` parameter set to `"auto"`
- `run_in_background` parameter set to `true`

The worker prompt MUST include:
1. Task spec (from `$FLOWCTL cat TASK_ID`)
2. Epic spec summary
3. Content of `.flow/project-context.md`
4. The task's `domain` field value
5. FLOWCTL path: `FLOWCTL="$HOME/.flow/bin/flowctl"`

If you spawn workers WITHOUT the `isolation: "worktree"` parameter, you are creating race conditions where two workers edit the same file simultaneously. This has caused real bugs in previous runs.

**W3: After ALL workers in this wave complete**
```bash
# Mark each task done
$FLOWCTL done TASK_ID --summary "what was done" --json

# MANDATORY: Run guard BETWEEN waves (not just at close)
$FLOWCTL guard

# Check for newly unblocked tasks
$FLOWCTL ready $EPIC_ID --json
# If more ready tasks: go back to W1
# If no more: proceed to phase done
```

**W4: Complete work phase**
```bash
$FLOWCTL phase done --epic $EPIC_ID --phase work --guard-ran --json
```

---

### Impl Review (impl_review)

**EXTERNAL REVIEW — DO NOT SELF-REVIEW. Send questions to RP after running guard.**

**IR1: Run guard FIRST**
```bash
$FLOWCTL guard
```
If guard fails: fix the issues before proceeding.

**IR2: Generate diff**
```bash
git diff main...HEAD --stat 2>/dev/null || git diff HEAD~5...HEAD --stat
```

**IR3: Send 10 forcing questions to RP**

```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

If backend is "rp", call `mcp__RepoPrompt__context_builder` with:
```
instructions: "Review this diff against the spec. Answer each question with evidence. Score 1-3. Cite specific file:line.

Diff: <paste git diff --stat output>
Spec: <paste epic spec summary>
Tasks completed: <paste task list>

Correctness:
Q1(Spec fidelity): For each acceptance criterion — MET/PARTIAL/NOT_MET with file:line proof.
Q2(Error paths): For each new function — what happens on nil/empty/malformed/unauthorized?
Q3(Edge cases): 3 specific inputs that could break this code?
Q4(Regression): Any existing test broken? New code paths without tests?
Q5(Impact): Are all files that depend on changed code still working?

Quality:
Q6(Dead code): Any commented-out code, unused imports, TODO without ticket?
Q7(Naming): Would a new developer understand each name without PR context?
Q8(Performance): N+1 queries? Unbounded loops? Missing pagination?
Q9(Security): Input validated at boundaries? Secrets in code? SQL parameterized?
Q10(Consistency): Follows project-context.md Critical Rules?

Format: Q[N]([topic]):[score] [evidence]"

response_type: "review"
```

If backend is "none": run self-review as fallback.

**IR4: Process RP response**

Parse scores. If NEEDS_WORK (total <25):
```bash
$FLOWCTL memory add --type pitfall --epic $EPIC_ID "Review: <RP finding summary>"
```
Fix issues, re-run RP review (max 2 iterations).

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --score TOTAL_SCORE --evidence "<paste RP's full response>" --json
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

**MANDATORY Step 5: Pre-launch + Ship-Readiness**

```bash
# Run automated pre-launch checks (security, a11y, infra, docs)
$FLOWCTL pre-launch --json
```

If pre-launch reports any "fail" dimension, fix before proceeding.

Then run deterministic checks:
```bash
# ADR/invariant compliance
$FLOWCTL invariants check --json 2>/dev/null || true
```

**MANDATORY Step 6: Documentation** — If user-facing changes exist, update README/CHANGELOG.

**MANDATORY Step 7: Ship**
```bash
$FLOWCTL epic completion $EPIC_ID ship --json
```

**MANDATORY Step 8: Git commit + push + PR (unless --no-pr)**

```bash
# Step 8a: Create feature branch if still on main
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" = "main" ] || [ "$CURRENT_BRANCH" = "master" ]; then
  BRANCH_NAME="feat/${EPIC_ID}"
  git checkout -b "$BRANCH_NAME"
fi

# Step 8b: Stage and commit all changes
git add -A
git commit -m "feat(${EPIC_ID}): <one-line summary of what the epic delivered>

Co-Authored-By: Claude <noreply@anthropic.com>"

# Step 8c: Push
git push -u origin HEAD

# Step 8d: Create draft PR (skip if --no-pr)
gh pr create --draft --title "feat(${EPIC_ID}): <title>" --body "$(cat <<'PREOF'
## Summary
<2-3 bullet points from epic spec>

## Tasks completed
<list task IDs and titles>

## Test plan
- [ ] Guard passes (lint + type + test)
- [ ] Manual verification of key changes

Generated by flow-code pipeline
PREOF
)"
```

```bash
$FLOWCTL phase done --epic $EPIC_ID --phase close --guard-ran --json
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

## Post-Run Compliance Verification

Before declaring the epic complete, verify EACH item. Output this table with actual Y/N values:

```
COMPLIANCE CHECK (must be ALL Y before shipping):
[ ] Startup: detect + status --interrupted + memory inject + review-backend + git branch
[ ] Brainstorm: complexity classified + all tier questions answered + 3 approaches scored + requirements.md written
[ ] Plan: research tools used (graph map/find) + scouts spawned + spec written + tasks created + validated
[ ] Plan Review: all 10 self-review questions answered with scores + total /30 computed
[ ] Work: tasks started + files locked + workers used isolation:"worktree" + guard run between waves
[ ] Impl Review: guard run + diff generated + all 10 questions answered with scores + total /30 computed
[ ] Close: validate + guard + quick commands + checklists + 7 ship questions scored + push + PR
```

If any item is N: go back and execute it before shipping.
