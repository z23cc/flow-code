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
$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --no-gate --json
$FLOWCTL epic plan $EPIC_ID --spec "Quick fix: <description>" --json
$FLOWCTL task create --epic $EPIC_ID --title "<description>" --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan --no-gate --json
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --no-gate --json
# Work: single worker, then:
$FLOWCTL guard
$FLOWCTL phase done --epic $EPIC_ID --phase work --guard-ran --no-gate --json
$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --no-gate --json
$FLOWCTL phase done --epic $EPIC_ID --phase close --guard-ran --no-gate --json
```

---

## MANDATORY Startup Sequence

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"

# Single startup call: detect + interrupted + review-backend + branch + epics
$FLOWCTL startup --json

# Memory inject (separate — may not be enabled)
$FLOWCTL memory inject --json 2>/dev/null || true
```

If startup shows interrupted tasks, auto-resume (or ask in --interactive mode).

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
  # Otherwise: execute the phase, then advance it with the required gate inputs
  # (score/evidence for review phases; receipt/receipt-file for artifact-backed phases).
  $FLOWCTL phase done --epic $EPIC_ID --phase $CURRENT_PHASE ... --json
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
# MANDATORY Step 1: Fast local context first, then deep RP context
cat .flow/project-context.md 2>/dev/null || true
$FLOWCTL graph map --json
$FLOWCTL find "<key terms from request>" --json
```

```
# Then use context_builder for deeper analysis — reused by Plan phase
mcp__RepoPrompt__context_builder({
  instructions: "<request summary>. Local fast-path findings: <graph map + find results>. Analyze: relevant files, existing patterns, potential approaches, complexity.",
  response_type: "question"
})
→ save chat_id as BRAINSTORM_CHAT_ID (Plan phase will reuse this via oracle_send)
```

Use context_builder's response to deepen the fast-path results. Do NOT skip `flowctl find`/`graph map` just because RP is available — they are the default first pass.

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
cat > /tmp/${EPIC_ID}-brainstorm-receipt.json <<EOF
{"requirements_path":".flow/specs/${SLUG}-requirements.md"}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase brainstorm --receipt-file /tmp/${EPIC_ID}-brainstorm-receipt.json --json
```

---

### Plan (plan)

**MANDATORY Step 1: Research** — Refresh the fast path, then check ADRs/invariants:
```bash
# Re-anchor on the cached graph/index before scout fan-out
$FLOWCTL graph map --json
$FLOWCTL find "<key terms from request>" --json

# ADRs and invariants
ls docs/decisions/ADR-*.md 2>/dev/null
$FLOWCTL invariants show --json 2>/dev/null || true
```

**MANDATORY Step 2: Parallel research — scouts + plan context via oracle_send**

Launch scouts AND deepen brainstorm context in parallel:

```
# 1. Spawn scouts (RP agent_run)
for each scout:
  agent_run(start, explore, "scout-<name>-<epic-id>", detach: true)
  → save session_ids

# 2. Deepen brainstorm context via oracle_send (reuse BRAINSTORM_CHAT_ID — no rebuild)
mcp__RepoPrompt__oracle_send({
  chat_id: BRAINSTORM_CHAT_ID,
  message: "Based on the chosen approach, create an implementation plan: file changes, task breakdown, dependencies, risk areas."
})
```

Wait for scouts. Feed scout findings into the ongoing chat if needed:
```
oracle_send(chat_id: BRAINSTORM_CHAT_ID, message: "Scout findings to incorporate: <key refs and gaps>")
```

See `step-02-research.md` for scout selection and output parsing.

**MANDATORY Step 3: Write spec + create tasks**
```bash
$FLOWCTL epic plan $EPIC_ID --spec "<spec text>" --json
$FLOWCTL task create --epic $EPIC_ID --title "<task 1>" --domain <domain> --json
$FLOWCTL task create --epic $EPIC_ID --title "<task 2>" --deps "$EPIC_ID.1" --json
# ... etc
$FLOWCTL validate --epic $EPIC_ID --json
cat > /tmp/${EPIC_ID}-plan-receipt.json <<EOF
{"spec_path":".flow/specs/${EPIC_ID}.md","task_ids":["${EPIC_ID}.1","${EPIC_ID}.2"]}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase plan --receipt-file /tmp/${EPIC_ID}-plan-receipt.json --json
```

---

### Plan Review (plan_review) — SPECULATIVE EXECUTION

**EXTERNAL REVIEW — DO NOT SELF-REVIEW. Send questions to RP.**

AI self-review failed 5 consecutive audits (scores fabricated). All review questions MUST be answered by RP context_builder (external research model), NOT by the implementing agent.

**Speculative execution**: Start Plan Review AND Work Wave 1 simultaneously. If review passes (>80% of the time) → worker results are valid. If review fails → cancel workers, fix plan, restart.

```
# Launch BOTH in parallel:
# 1. Plan Review via context_builder
# 2. Work Wave 1 workers via agent_run (for ready tasks with no plan-sensitive deps)
```

If `--no-speculative` flag is set, run Plan Review first, then Work (original behavior).

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

Save the returned `chat_id` — use it for follow-up via `oracle_send` if scores need clarification.

If backend is "none": run self-review as fallback (answer questions yourself, but with the understanding that self-review is weaker).

**PR2: Extract scores from RP response and pass to flowctl**

Parse RP's response for Q1:N through Q10:N scores. Compute total.

If any score is unclear or needs elaboration, use `oracle_send` to follow up without rebuilding context:
```
mcp__RepoPrompt__oracle_send({
  chat_id: "<chat_id from context_builder>",
  message: "Clarify Q5: what specific data paths are you concerned about?"
})
```

```bash
cat > /tmp/${EPIC_ID}-plan-review-findings.md <<'EOF'
<paste RP's full response here>
EOF
cat > /tmp/${EPIC_ID}-plan-review-receipt.json <<EOF
{"backend":"$REVIEW_BACKEND","verdict":"SHIP","findings_path":"/tmp/${EPIC_ID}-plan-review-findings.md","score":${TOTAL_SCORE}}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase plan_review --score TOTAL_SCORE --evidence "<paste RP's full response here>" --receipt-file /tmp/${EPIC_ID}-plan-review-receipt.json --json
```

The evidence field contains RP's analysis, not your own. flowctl validates ≥200 chars + ≥5 question references.

Save the `chat_id` as `PLAN_REVIEW_CHAT_ID` — Impl Review can reuse this context.

**Speculative execution check**: If workers were speculatively started during Plan Review:
- If score ≥ 25 (SHIP): workers are valid → proceed to Work phase (skip W1/W2, workers already running)
- If score < 25 (NEEDS_WORK): cancel all speculative workers → fix plan → restart

---

### Work (work)

**⚠️ AUDIT FAILURE HISTORY: This phase failed compliance audit TWICE (fn-5 and fn-6) on the SAME 3 issues. If you skip these again, you are knowingly producing unsafe output.**

**Failure 1 (CRITICAL): Workers ran without worktree isolation — 6 parallel agents wrote to same directory, risking file corruption.**
**Failure 2 (HIGH): No file locking — concurrent edits to settings.py and requirements.txt.**
**Failure 3 (HIGH): No integration checkpoint guard — broken code passed silently.**

---

**W1: Find + start + lock ready tasks**
```bash
READY_JSON=$($FLOWCTL ready $EPIC_ID --json)
# For EACH task in ready list:
$FLOWCTL start TASK_ID --json
$FLOWCTL lock --task TASK_ID --files "file1,file2" --json
```

**W2: Spawn workers (workers self-create worktrees)**

For EACH ready task:

```bash
REPO_ROOT=$(pwd)
WORKER_PROMPT=$($FLOWCTL worker-prompt --task $TASK_ID --bootstrap)
```

```
mcp__RepoPrompt__agent_run({
  op: "start",
  model_id: "engineer",
  session_name: "worker-$TASK_ID",
  message: "$WORKER_PROMPT\n\nREPO_ROOT: $REPO_ROOT\nOWNED_FILES: $OWNED_FILES",
  detach: true
})
→ save session_id into ACTIVE_SESSIONS[]
```

Workers self-manage: `git worktree add` → work → commit → output `COMMIT_HASH`.
See `step-04-spawn-workers.md` for model selection and prompt template.

**W3: Continuous streaming loop — merge + spawn in one loop**

```
while ACTIVE_SESSIONS is not empty:
  result = agent_run(wait, session_ids: ACTIVE_SESSIONS, timeout: 1800)

  # Handle Codex sandbox approval
  if result.status == "waiting_for_input":
    agent_run(respond, session_id, interaction_id, response: "accept_for_session")
    continue

  # Merge completed worker
  COMMIT_HASH = <parse from output, or: git -C worktree rev-parse HEAD>
  flowctl show $finished_task --json
  git merge --no-ff $COMMIT_HASH -m "merge: $finished_task"
  git worktree remove worktree --force
  flowctl unlock --task $finished_task

  # Steer remaining workers
  for each remaining: agent_run(steer, "$finished_task merged, changed: $FILES")

  ACTIVE_SESSIONS.remove(finished_session)

  # CONTINUOUS SPAWN — immediately start newly unblocked tasks
  NEWLY_READY = flowctl ready $EPIC_ID --json
  for each new_task (not already spawned):
    flowctl start + lock + spawn worker → add to ACTIVE_SESSIONS
```

After loop (all tasks done):
```bash
# Integration checkpoint — guard + invariants (runs ONCE, not per-wave)
$FLOWCTL guard
$FLOWCTL invariants check
```

**W4: Complete work phase**
```bash
cat > /tmp/${EPIC_ID}-work-receipt.json <<EOF
{"guard_passed":true,"invariants_passed":true}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase work --guard-ran --receipt-file /tmp/${EPIC_ID}-work-receipt.json --json
```

---

### Impl Review (impl_review)

**EXTERNAL REVIEW — DO NOT SELF-REVIEW. Send questions to RP after running guard.**

**IR1: Run guard (skip if no new commits since work phase)**
```bash
# Check if any commits were made after work phase guard
LAST_GUARD_COMMIT=$(git log --oneline -1 --format=%H)
WORK_PHASE_HEAD=$(git log --oneline -1 --format=%H)  # same if no fix commits

if [ "$LAST_GUARD_COMMIT" != "$WORK_PHASE_HEAD" ] || true; then
  # New commits since work phase guard — must re-run
  $FLOWCTL guard
fi
```
If guard fails: fix the issues before proceeding. If no new commits since the work phase's integration checkpoint guard, skip (already verified).

**IR2: Generate diff**
```bash
git diff main...HEAD --stat 2>/dev/null || git diff HEAD~5...HEAD --stat
```

**IR3: Send 10 forcing questions to RP**

```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

If backend is "rp", **reuse Plan Review context** via `oracle_send` if `PLAN_REVIEW_CHAT_ID` is available (saves 30s-5min of context rebuilding). Otherwise fall back to fresh `context_builder`:

```
# PREFERRED: reuse Plan Review chat (same epic, files auto-refreshed by RP)
if PLAN_REVIEW_CHAT_ID is available:
  mcp__RepoPrompt__oracle_send({
    chat_id: PLAN_REVIEW_CHAT_ID,
    message: "Implementation is complete. Review this diff against the spec...
      <paste diff + questions below>",
    mode: "review"
  })

# FALLBACK: fresh context_builder (if no chat_id from Plan Review)
else:
  context_builder(instructions: "...", response_type: "review")
```

Review questions (used in either path):
```
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

Save the returned `chat_id` for the fix loop.

If backend is "none": run self-review as fallback.

**IR4: Process RP response + fix loop via oracle_send**

Parse scores. If NEEDS_WORK (total <25):

```bash
$FLOWCTL memory add --type pitfall --epic $EPIC_ID "Review: <RP finding summary>"
```

Fix the issues, then **re-review via `oracle_send`** (reuses existing context — no expensive rebuild):

```
mcp__RepoPrompt__oracle_send({
  chat_id: "<chat_id from context_builder>",
  message: "Issues fixed. Changes:
    - <list what you fixed>
    Re-review and re-score Q1-Q10. Format: Q[N]([topic]):[score] [evidence]",
  mode: "review"
})
```

Max 2 iterations. After max → proceed with warning.

**Do NOT call `context_builder` again** — `oracle_send` continues in the same chat with all prior context intact. This saves 30s-5min of context rebuilding per re-review.

```bash
cat > /tmp/${EPIC_ID}-impl-review-findings.md <<'EOF'
<paste RP's full response>
EOF
cat > /tmp/${EPIC_ID}-impl-review-receipt.json <<EOF
{"backend":"$REVIEW_BACKEND","verdict":"SHIP","findings_path":"/tmp/${EPIC_ID}-impl-review-findings.md","score":${TOTAL_SCORE}}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase impl_review --score TOTAL_SCORE --evidence "<paste RP's full response>" --receipt-file /tmp/${EPIC_ID}-impl-review-receipt.json --json
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

**MANDATORY Step 5.5: Unified RP Cleanup**

All RP session cleanup happens HERE — not during the streaming loop. This is the single cleanup point for the entire epic.

```
# Batch cleanup ALL sessions from this epic (scouts + workers + plan-sync)
mcp__RepoPrompt__agent_manage({
  op: "cleanup_sessions",
  session_ids: ALL_SESSION_IDS  # collected during plan + work phases
})
```

If `ALL_SESSION_IDS` is not available (e.g., session crashed), fall back to listing:
```
mcp__RepoPrompt__agent_manage({ op: "list_sessions", state: "completed", limit: 50 })
mcp__RepoPrompt__agent_manage({ op: "cleanup_sessions", session_ids: [<matched>] })
```

```bash
# Remove any leftover worktrees
git worktree list | grep "worker-" | awk '{print $1}' | while read wt; do
  git worktree remove "$wt" --force 2>/dev/null || true
done
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
cat > /tmp/${EPIC_ID}-close-receipt.json <<EOF
{"guard_passed":true,"pre_launch_passed":true,"validate_passed":true}
EOF
$FLOWCTL phase done --epic $EPIC_ID --phase close --guard-ran --receipt-file /tmp/${EPIC_ID}-close-receipt.json --json
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
- **NEVER skip guard** — run `$FLOWCTL guard` at the work integration checkpoint AND close
- **NEVER fake command output** — actually run the command and use its real output

## Post-Run Compliance Verification

Before declaring the epic complete, verify EACH item. Output this table with actual Y/N values:

```
COMPLIANCE CHECK (must be ALL Y before shipping):
[ ] Startup: detect + status --interrupted + memory inject + review-backend + git branch
[ ] Brainstorm: complexity classified + all tier questions answered + 3 approaches scored + requirements.md written
[ ] Plan: research tools used (graph map/find) + scouts spawned + spec written + tasks created + validated
[ ] Plan Review: all 10 self-review questions answered with scores + total /30 computed
[ ] Work: tasks started + files locked + workers used isolation:"worktree" + integration checkpoint guard receipt recorded
[ ] Impl Review: guard run + diff generated + all 10 questions answered with scores + total /30 computed
[ ] Close: validate + guard + quick commands + checklists + 7 ship questions scored + push + PR
```

If any item is N: go back and execute it before shipping.
