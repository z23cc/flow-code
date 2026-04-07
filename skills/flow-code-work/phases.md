# Flow Work Phases

(Branch question already asked in SKILL.md before reading this file)

**CRITICAL**: If you are about to create:
- a markdown TODO list,
- a task list outside `.flow/`,
- or any plan files outside `.flow/`,

**STOP** and instead:
- create/update tasks in `.flow/` using `flowctl`,
- record details in the epic/task spec markdown.

## Setup

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Step 1: Resolve Input

Detect input type in this order (first match wins):

1. **Flow task ID** `fn-N-slug.M` (e.g., fn-1-add-oauth.3) or legacy `fn-N.M`/`fn-N-xxx.M` → **SINGLE_TASK_MODE**
2. **Flow epic ID** `fn-N-slug` (e.g., fn-1-add-oauth) or legacy `fn-N`/`fn-N-xxx` → **EPIC_MODE**
3. **Spec file** `.md` path that exists on disk → **EPIC_MODE**
4. **Idea text** everything else → **EPIC_MODE**

**Track the mode** — it controls looping in the Wave Loop (Steps 3–13).

---

**Flow task ID (fn-N-slug.M or legacy fn-N.M/fn-N-xxx.M)** → SINGLE_TASK_MODE:
- Read task: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get epic from task data for context: `$FLOWCTL show <epic-id> --json && $FLOWCTL cat <epic-id>`
- **This is the only task to execute** — no loop to next task

**Flow epic ID (fn-N-slug or legacy fn-N/fn-N-xxx)** → EPIC_MODE:
- Clear auto-execute marker (confirms work has started): `$FLOWCTL epic auto-exec <id> --done --json`
- Read epic: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get first ready task: `$FLOWCTL ready --epic <id> --json`

**Spec file start (.md path that exists)**:
1. Check file exists: `test -f "<path>"` — if not, treat as idea text
2. Initialize: `$FLOWCTL init --json`
3. Read file and extract title from first `# Heading` or use filename
4. Create epic: `$FLOWCTL epic create --title "<extracted-title>" --json`
5. Set spec from file: `$FLOWCTL epic plan <epic-id> --file <path> --json`
6. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <title>" --json`
7. Continue with epic-id

**Spec-less start (idea text)**:
1. Initialize: `$FLOWCTL init --json`
2. Create epic: `$FLOWCTL epic create --title "<idea>" --json`
3. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <idea>" --json`
4. Continue with epic-id

## Step 2: Apply Branch Choice

- **Worktree** (default when on main): use `skill: flow-code-worktree-kit` to create an isolated worktree. This keeps main clean and allows parallel work.
- **Current branch** (default when on feature branch or dirty tree): proceed in place.
- **New branch** (only if explicitly requested via `--branch=new`):
  ```bash
  git checkout main && git pull origin main
  git checkout -b <branch>
  ```

## Wave Loop (Steps 3–13 repeat per wave)

### Wave Model

A **wave** is one batch of all currently-ready tasks (all dependencies satisfied). The task loop executes in waves:

```
Wave 1: [ready tasks with no deps] → spawn workers → wait → merge → checkpoint
Wave 2: [tasks unblocked by Wave 1] → spawn workers → wait → merge → checkpoint
Wave N: [remaining tasks]           → spawn workers → wait → merge → checkpoint
```

**Wave lifecycle:**
1. **Find ready tasks** (Step 3) — query `$FLOWCTL ready --epic <id>`
2. **Start + spawn workers** (Steps 4–7) — lock files, spawn in parallel
3. **Wait + merge** (Step 8) — collect results, merge worktree branches
4. **Cleanup** (Step 9) — release file locks (`$FLOWCTL unlock --all`)
5. **Checkpoint** (Step 10) — mandatory: run guards + invariants, aggregate results
6. **Plan-sync** (Step 12) — update downstream task specs if drift detected
7. **Loop** (Step 13) — return to Step 3 for next wave, or finish if no ready tasks

**Stop rules:**
- Guards or invariants fail and cannot be auto-fixed
- 2 or more tasks in the same wave failed
- No ready tasks remain (all done or blocked)

**Default mode: Worktree + Teams** — each worker gets an isolated git worktree AND runs as a Team teammate. Worktree provides kernel-level file isolation; Teams provides coordination (TeamCreate + SendMessage + file locking).

**CRITICAL: When multiple tasks are ready, they MUST run in parallel. Do NOT execute them sequentially "for quality" or "one at a time." Parallel execution with isolation IS the quality mechanism.**

### Step 3. Find Ready Tasks

**State awareness (always runs first):**

Every startup reads current epic state and outputs progress — this is not a special "resume mode", it is normal state reading.

```bash
# 1. Read all tasks for the epic
$FLOWCTL tasks --epic <epic-id> --json
```

Parse the JSON and output a progress summary:

```
── Progress: <epic-id> ───────────────────
  Done:        3/7 (fn-N.1, fn-N.2, fn-N.3)
  In progress: 1   (fn-N.4)
  Blocked:     1   (fn-N.6)
  Ready:       2   (fn-N.5, fn-N.7)
──────────────────────────────────────────
```

**Restart stale in_progress tasks:** If any task has status `in_progress` but no active worker is running for it (e.g., session was interrupted), restart it so `flowctl ready` picks it up:

```bash
# For each stale in_progress task (no active worker):
$FLOWCTL restart <stale-task-id> --json
```

After restarts, find ready tasks normally:

```bash
$FLOWCTL ready --epic <epic-id> --json
```

**Deadlock detection:** If ready tasks = 0 AND no tasks are in_progress, check for stalled state:

```bash
TASKS_JSON=$($FLOWCTL tasks --epic <epic-id> --json)
FAILED=$(echo "$TASKS_JSON" | jq '[.[] | select(.status=="failed")] | length')
BLOCKED=$(echo "$TASKS_JSON" | jq '[.[] | select(.status=="blocked")] | length')
TODO_WITH_DEPS=$(echo "$TASKS_JSON" | jq '[.[] | select(.status=="todo" and (.dependencies | length > 0))] | length')
```

Classify the stall:
- If `FAILED > 0`: "Epic stalled: $FAILED tasks failed, blocking downstream. Consider restart or skip."
- If `BLOCKED > 0`: "Epic stalled: $BLOCKED tasks blocked. Check block reasons."
- If all remaining are `todo` with unmet deps: "Possible circular dependency. Run `$FLOWCTL validate --epic <epic-id>`."

Stop the wave loop in any of these cases.

**No-progress watchdog:** Track `completed_count` at the start of each wave:

```bash
COMPLETED_COUNT=$(echo "$TASKS_JSON" | jq '[.[] | select(.status=="done")] | length')
```

If `completed_count` is unchanged after 2 consecutive waves:
```
"No progress detected across 2 waves. Stopping."
```
Stop the wave loop.

Collect ALL ready tasks (no unresolved dependencies). If no ready tasks, check for completion review gate (see Step 10 below).

### Step 4. Readiness Check

Before starting, validate each task spec is implementation-ready:

```bash
$FLOWCTL cat <task-id>
```

**Spec hash snapshot (mid-wave protection):**

Record a content hash for each task spec at wave start:
```bash
SPEC_HASH_<task-id>=$(echo "$($FLOWCTL cat <task-id>)" | shasum -a 256 | cut -d' ' -f1)
```
Workers compare this hash during re-anchor (Phase 2). If the spec changed since wave start, log:
```
"Warning: spec for <task-id> changed since wave start (hash mismatch)"
```
Continue execution but note the mismatch in evidence.

**Check these fields exist and are non-empty:**
- `## Description` — what to build (not just a title)
- `## Acceptance` — at least one testable `- [ ]` criterion
- `**Files:**` — expected files to create/modify

**If any are missing or vague:**
- Use AskUserQuestion: "Task `<id>` spec is missing [field]. Add it before starting?"
- Do NOT spawn a worker with an incomplete spec — workers guess when specs are vague

### Step 5. Start Tasks

```bash
# 1. Start each task
$FLOWCTL start <task-id-1> --json
$FLOWCTL start <task-id-2> --json
```

### Step 6. File Ownership & Locking (Teams mode)

For each ready task, read file ownership from the task spec and lock with appropriate modes:

```bash
TASK_SPEC=$($FLOWCTL cat <task-id>)

# Parse write files
WRITE_FILES=$(echo "$TASK_SPEC" | grep '^\*\*Files (write):\*\*' | sed 's/\*\*Files (write):\*\*//' | tr ',' '\n' | xargs)
# Parse read files
READ_FILES=$(echo "$TASK_SPEC" | grep '^\*\*Files (read):\*\*' | sed 's/\*\*Files (read):\*\*//' | tr ',' '\n' | xargs)
# Backward compat: plain **Files:** treated as write
LEGACY_FILES=$(echo "$TASK_SPEC" | grep '^\*\*Files:\*\*' | sed 's/\*\*Files:\*\*//' | tr ',' '\n' | xargs)
if [[ -z "$WRITE_FILES" && -n "$LEGACY_FILES" ]]; then
  WRITE_FILES="$LEGACY_FILES"
fi

# Lock with appropriate modes
for f in $WRITE_FILES; do $FLOWCTL lock --task <task-id> --files "$f" --mode write; done
for f in $READ_FILES; do $FLOWCTL lock --task <task-id> --files "$f" --mode read; done

if [[ -z "$WRITE_FILES" && -z "$READ_FILES" ]]; then
  echo "Warning: <task-id> has no **Files (write/read):** field — worker gets unrestricted access"
fi
```

If a task spec has no file fields, log a warning but still spawn. Worker will have unrestricted access (backward compat). Read locks are shared — multiple tasks can read the same file concurrently.

**File overlap detection (before locking):**

After collecting file lists for ALL ready tasks in the current wave, check for overlaps:

```bash
# Detect file conflicts across ready tasks in this wave
$FLOWCTL files --epic <epic-id> --status in_progress --json
# Check output for "conflicts" array — non-empty means overlap
```

If conflicts exist (two tasks declare the same file):
1. Log the conflict: `"File conflict: <file> claimed by <task-a> and <task-b>"`
2. Use AskUserQuestion: "Tasks <A> and <B> both need <file>. Options: (a) serialize them (add dependency), (b) let both proceed (risk merge conflicts), (c) reassign files"
3. If serialize: `$FLOWCTL dep add <task-b> <task-a> --json` and remove task-b from this wave's ready list
4. Block worker spawn for conflicting tasks until resolved

**RP context detection (once per wave, before spawning workers):**

Detect RP availability and set `RP_CONTEXT` for workers. This controls whether workers use `context_builder` for deep implementation context in Worker Phase 6.

```bash
# 1. Check if RP context is enabled (default: false — opt-in only)
RP_ENABLED=$($FLOWCTL config get rp_context.enabled --json 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('value', False))" 2>/dev/null || echo "False")

# 2. Override: --rp-context flag forces enabled, --no-rp-context forces disabled
# (flags are parsed from $ARGUMENTS in SKILL.md)
```

Determine the RP_CONTEXT tier (check in order, first match wins):
1. If `--no-rp-context` flag was passed OR `RP_ENABLED` is false → `RP_CONTEXT=none`
2. If `--rp-context` flag was passed OR `RP_ENABLED` is true:
   - **Tier 1 (MCP)**: Check if `mcp__RepoPrompt__context_builder` is in the available tools list for this session → `RP_CONTEXT=mcp`
   - **Tier 2 (CLI)**: `which rp-cli >/dev/null 2>&1` succeeds → `RP_CONTEXT=cli`
   - **Tier 3 (fallback)**: Neither available → `RP_CONTEXT=none`

**Prompt generation for worker:**

Use `flowctl worker-prompt --bootstrap` to generate a minimal bootstrap prompt for each worker. This outputs a ~200 token prompt that instructs the worker to call `worker-phase next` in a loop, fetching full phase instructions on demand.

```bash
# Build the bootstrap prompt — worktree isolation is the default
WORKER_PROMPT=$($FLOWCTL worker-prompt --task <task-id> --bootstrap [--tdd] [--review rp|codex])
```

### Step 7. Spawn Workers (Worktree + Teams — Default)

```bash
# Enable git rerere to auto-learn merge conflict resolutions
git config rerere.enabled true
```

1. Create team: `TeamCreate({team_name: "flow-<epic-id>"})`
2. Spawn all workers with BOTH `isolation: "worktree"` AND `team_name`:

```
Agent({
  subagent_type: "flow-code:worker",
  name: "worker-<task-id>",
  description: "Implement <task-title>",
  team_name: "flow-<epic-id>",
  isolation: "worktree",
  run_in_background: true,
  timeout: 2700000,
  prompt: "$WORKER_PROMPT

    TASK_ID: <task-id>
    EPIC_ID: <epic-id>
    FLOWCTL: /path/to/flowctl
    REVIEW_MODE: none|rp|codex
    RALPH_MODE: true|false
    TDD_MODE: true|false
    RP_CONTEXT: $RP_CONTEXT
    TEAM_MODE: true
    OWNED_FILES: <comma-separated file list from Step 6>
  "
})
```

Spawn ALL ready task workers in a SINGLE message with multiple Agent tool calls. Workers run in isolated worktrees (kernel-level file separation) with Teams coordination (SendMessage for status reporting).

**Team lifecycle**: `TeamCreate` is called ONCE per epic execution (not per wave). The same team persists across waves — workers join via spawn and leave on completion. No `TeamDelete` needed; the team is ephemeral to the session.

**Worker returns**: Summary of implementation, files changed, test results, review verdict.

### Step 8. Wait for Workers & Merge Back

Wait for all workers to complete.

**Worker timeout handling:** If a worker times out (exceeds the 45-minute ceiling):
```bash
$FLOWCTL fail <task-id> "Worker timeout after 45min"
```
This triggers the retry logic in the task lifecycle (`up_for_retry` if retries remaining). Log the timeout and continue processing other workers in the wave.

**Merge-back** (after all workers return):

```bash
WORKTREE_SH="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/skills/flow-code-worktree-kit/scripts/worktree.sh"
```

For each worker that returned a branch name (in spawn order):

```bash
bash "$WORKTREE_SH" merge-back <worker-branch>
git branch -d <worker-branch> 2>/dev/null || true
```

**Conflict handling**: If `merge-back` fails:
1. Run `git merge --abort` (working tree stays clean)
2. Classify the conflict:
   a. Get conflicting files: `git diff --name-only --diff-filter=U`
   b. Get worker's modified files: `git diff --name-only main..<worker-branch>`
   c. If conflict files NOT in worker's modified set → try `git rebase main <worker-branch>`
   d. If rebase succeeds → retry merge
   e. If rebase fails or conflict is in worker's files → mark task for retry:
      `$FLOWCTL restart <task-id>`
3. Continue merging remaining worker branches (don't stop the sequence)
4. **Retry storm protection**: if >50% of wave tasks need retry, skip inline retry and start a fresh wave

### Step 9. Wave Cleanup

Release file locks so the next wave can re-lock with new ownership:

```bash
$FLOWCTL unlock --all
```

Worktrees are cleaned up automatically by the worktree kit.

### Step 10. Verify Completion & Checkpoint

After worker(s) return, verify each task completed:

```bash
$FLOWCTL show <task-id> --json
```

If status is not `done`, the worker failed. Check output and retry or investigate.

#### Wave Checkpoint (EPIC_MODE — MANDATORY after each wave)

After ALL workers in a wave return, run a structured checkpoint before finding the next wave of tasks. This prevents cascading failures and ensures integration quality.

**Sub-step 1 — Aggregate Results:**
Collect from every worker in the batch:
- Status: done / failed / spec_conflict
- Files changed (from worker summary)
- Tests: pass / fail / skipped
- Review verdict (if REVIEW_MODE != none)

**Sub-step 2 — Integration Verification:**
```bash
# Run guards on the result (catches cross-task breakage)
$FLOWCTL guard

# Check architecture invariants still hold
$FLOWCTL invariants check
```

If guards or invariants fail, identify which task's changes caused the regression and report to user.

**Sub-step 3 — Wave Summary:**
Output a concise checkpoint report:
```
── Wave N Checkpoint ──────────────────────
  Tasks completed: 3/3 (fn-1.1, fn-1.2, fn-1.3)
  Files changed:   12
  Guards:          ✓ pass
  Invariants:      ✓ pass
  Issues:          none
  Next ready:      fn-1.4, fn-1.5
───────────────────────────────────────────
```

**When to STOP the wave loop:**
- Guards or invariants fail and cannot be auto-fixed → report to user
- ≥ 2 tasks in the same wave failed → likely a systemic issue, pause and investigate

### Step 11. Interactive Checkpoint (if `--interactive`)

If `--interactive` was passed, pause after each task completes and show a checkpoint:

```
Checkpoint: Task <task-id> complete
  Files changed: <list from git diff --stat>
  Tests: <pass/fail>
  Review: <verdict if review ran>

Continue to next task? (y/n/skip/abort)
  y = continue (default)
  n = pause here, I'll review manually
  skip = skip remaining tasks, go to Step 15
  abort = stop execution entirely
```

Use AskUserQuestion to wait for response. If no `--interactive` flag, skip this step entirely.

### Step 12. Plan Sync (if enabled) — BOTH MODES

**Runs in SINGLE_TASK_MODE and EPIC_MODE.** Only the loop-back in Step 13 differs by mode.

Only run plan-sync if the task status is `done` (from Step 10). If not `done`, skip plan-sync and investigate/retry.

Check if plan-sync should run:

```bash
$FLOWCTL config get planSync.enabled --json
```

Skip unless planSync.enabled is explicitly `true` (null/false/missing = skip).

Get remaining tasks (todo status = not started yet):

```bash
$FLOWCTL tasks --epic <epic-id> --status todo --json
```

Skip if empty (no downstream tasks to update).

Extract downstream task IDs:

```bash
DOWNSTREAM=$($FLOWCTL tasks --epic <epic-id> --status todo --json | jq -r '[.[].id] | join(",")')
```

Note: Only sync to `todo` tasks. `in_progress` tasks are already being worked on - updating them mid-flight could cause confusion.

Use the Task tool to spawn the `plan-sync` subagent with this prompt:

```
Sync downstream tasks after implementation.

COMPLETED_TASK_ID: fn-X.Y
EPIC_ID: fn-X
FLOWCTL: /path/to/flowctl
DOWNSTREAM_TASK_IDS: fn-X.3,fn-X.4,fn-X.5

Follow your phases in plan-sync.md exactly.
```

Plan-sync returns summary. Log it but don't block - task updates are best-effort.

### Step 13. Loop or Finish

**SINGLE_TASK_MODE**: After Step 10 → Step 12, go to Step 15 (Quality). No loop.

**EPIC_MODE**: After Step 10 → Step 12, return to Step 3 for next wave.

### Step 14. Adversarial Review (EPIC_MODE only — Layer 3)

When Step 3 finds no ready tasks, all tasks are done. Run cross-model adversarial review before shipping.

**This is Layer 3 of the quality system.** A different model family (GPT via Codex) tries to **break** the code. This catches blind spots that Claude (implementing model) and RP (same model family) both miss.

```bash
# 1. Check codex CLI
which codex >/dev/null 2>&1
```

**If codex available:**
```bash
# 2. Scope diff to this epic's changes only
BRANCH_BASE=$(git merge-base main HEAD)
$FLOWCTL codex adversarial --base "$BRANCH_BASE" --json
```

Initialize `ADVERSARIAL_ITERATIONS=0`. Parse response:
- `verdict: "SHIP"` → go to Step 15
- `verdict: "NEEDS_WORK"` → increment `ADVERSARIAL_ITERATIONS`. If `>= 2`: log "Adversarial review: 2 iterations completed. First iteration finds real issues, second verifies fixes. Proceeding." → go to Step 15. Otherwise: fix issues, commit, re-run.

**If codex not available:**
```
⚠ Codex CLI not found — skipping Layer 3 adversarial review.
  Install: npm install -g @openai/codex
```
Go to Step 15 directly. No fallback to RP — different model family is the point.

**After SHIP (or skip):**
```bash
$FLOWCTL epic completion <epic-id> ship --json
```

---

**Why spawn a worker?**

Context optimization. Each task gets fresh context:
- No bleed from previous task implementations
- Re-anchor info stays with implementation (not lost to compaction)
- Review cycles stay isolated
- Main conversation stays lean (just summaries)

**Ralph mode**: Worker inherits `bypassPermissions` from parent. FLOW_RALPH=1 and REVIEW_RECEIPT_PATH are passed through.

**Interactive mode**: Permission prompts pass through to user. Worker runs synchronously (blocking).

---

## Step 15: Quality

After all tasks complete (or periodically for large epics):

- Run relevant tests
- Run lint/format per repo
- If change is large/risky, run the quality auditor subagent:
  - Task flow-code:quality-auditor("Review recent changes")
- Fix critical issues

## Step 16: Ship

**Verify all tasks done**:
```bash
$FLOWCTL show <epic-id> --json
$FLOWCTL validate --epic <epic-id> --json
```

**Final commit** (if any uncommitted changes):
```bash
git add -A
git status
git diff --staged
git commit -m "<final summary>"
```

**Do NOT close the epic here** unless the user explicitly asked.
Ralph closes done epics at the end of the loop.

**Auto push + draft PR** (default behavior, skip with `--no-pr`):

```bash
# Get current branch
BRANCH=$(git branch --show-current)

# Only create PR if NOT on main/master (direct push to main doesn't need a PR)
if [ "$BRANCH" != "main" ] && [ "$BRANCH" != "master" ]; then
  git push -u origin "$BRANCH"

  # Build PR body from template (prompts/pr-body.md)
  $FLOWCTL show <epic-id> --json   # get title + spec → {{epic_overview}}
  $FLOWCTL tasks --epic <epic-id> --json  # get task list → {{task_list}}
  # {{guard_result}}: last guard output
  # {{adversarial_result}}: SHIP or "skipped (codex not available)"
  # {{test_summary}}: test pass/fail counts
  # Read prompts/pr-body.md, interpolate placeholders, use as PR body

  gh pr create --draft \
    --title "<epic title>" \
    --body "<interpolated pr-body.md content>"
else
  # On main — just push directly
  git push origin "$BRANCH"
fi
```

If `gh` is not available or PR creation fails, log the error but do not fail the workflow — the code is already pushed.

**Session summary** (always output at end):
```
── Session Summary ─────────────────────────
  Epic: <epic-id> "<title>"
  Tasks: N completed, M skipped
  Commits: K
  Duration: Xm Ys (from first task start to now)
  Quality:
    Layer 1 (guard): <pass/fail/nothing to run>
    Layer 3 (adversarial): <SHIP/skipped>
  PR: <URL or "skipped">
────────────────────────────────────────────
```

**Suggest next steps:**
```
Done! Next:
1) Run retrospective: `/flow-code:retro <epic-id>`
2) Start next epic: `/flow-code:work <next-epic-id>`
```

## Definition of Done

Confirm before ship:
- All tasks have status "done"
- `$FLOWCTL validate --epic <id>` passes
- Tests pass
- Lint/format pass
- Docs updated if needed
- Working tree is clean

## Example flow

**Default mode (worktree isolation — auto-parallel):**
```
Step 1 (resolve) → Step 2 (branch) → Wave Loop:
  ├─ Step 3: read state + progress summary, restart stale tasks, find ready tasks
  ├─ Step 4: readiness check
  ├─ Step 5: start tasks
  ├─ Step 6: file ownership & locking
  ├─ Step 7: spawn workers (worktree isolation, default)
  ├─ Step 8: wait for workers + merge back
  ├─ Step 9: cleanup
  ├─ Step 10: verify done + wave checkpoint
  ├─ Step 11: interactive pause (if --interactive)
  ├─ Step 12: plan-sync (if enabled + downstream tasks exist)
  ├─ Step 13: EPIC_MODE? → loop to Step 3 | SINGLE_TASK_MODE? → Step 15
  ├─ no more tasks → Step 14: adversarial review gate
  └─ Step 15 (quality) → Step 16 (ship)
```
