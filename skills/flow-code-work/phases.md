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
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl.py"
```

## Phase 1: Resolve Input

Detect input type in this order (first match wins):

1. **Flow task ID** `fn-N-slug.M` (e.g., fn-1-add-oauth.3) or legacy `fn-N.M`/`fn-N-xxx.M` → **SINGLE_TASK_MODE**
2. **Flow epic ID** `fn-N-slug` (e.g., fn-1-add-oauth) or legacy `fn-N`/`fn-N-xxx` → **EPIC_MODE**
3. **Spec file** `.md` path that exists on disk → **EPIC_MODE**
4. **Idea text** everything else → **EPIC_MODE**

**Track the mode** — it controls looping in Phase 3.

---

**Flow task ID (fn-N-slug.M or legacy fn-N.M/fn-N-xxx.M)** → SINGLE_TASK_MODE:
- Read task: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get epic from task data for context: `$FLOWCTL show <epic-id> --json && $FLOWCTL cat <epic-id>`
- **This is the only task to execute** — no loop to next task

**Flow epic ID (fn-N-slug or legacy fn-N/fn-N-xxx)** → EPIC_MODE:
- Read epic: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get first ready task: `$FLOWCTL ready --epic <id> --json`

**Spec file start (.md path that exists)**:
1. Check file exists: `test -f "<path>"` — if not, treat as idea text
2. Initialize: `$FLOWCTL init --json`
3. Read file and extract title from first `# Heading` or use filename
4. Create epic: `$FLOWCTL epic create --title "<extracted-title>" --json`
5. Set spec from file: `$FLOWCTL epic set-plan <epic-id> --file <path> --json`
6. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <title>" --json`
7. Continue with epic-id

**Spec-less start (idea text)**:
1. Initialize: `$FLOWCTL init --json`
2. Create epic: `$FLOWCTL epic create --title "<idea>" --json`
3. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <idea>" --json`
4. Continue with epic-id

## Phase 2: Apply Branch Choice

Based on user's answer from setup questions:

- **Worktree**: use `skill: flow-code-worktree-kit`
- **New branch**:
  ```bash
  git checkout main && git pull origin main
  git checkout -b <branch>
  ```
- **Current branch**: proceed (user already confirmed)

## Phase 3: Task Loop

**For each task**, spawn a worker subagent with fresh context.

**Parallel mode** (`--parallel`): When the user passes `--parallel` flag, find ALL ready tasks (no unresolved dependencies) and spawn workers for them simultaneously using multiple Task tool calls in a single message. This is safe because ready tasks have no inter-dependencies. After all parallel workers return, verify each, run plan-sync, then find the next batch of ready tasks. Skip parallel mode for SINGLE_TASK_MODE.

### 3a. Find Next Task(s)

```bash
$FLOWCTL ready --epic <epic-id> --json
```

**Sequential mode (default):** Pick the first ready task.
**Parallel mode (`--parallel`):** Collect ALL ready tasks for simultaneous execution.

If no ready tasks, check for completion review gate (see 3g below).

### 3b. Readiness Check + Start Task(s)

Before starting, validate each task spec is implementation-ready:

```bash
$FLOWCTL cat <task-id>
```

**Check these fields exist and are non-empty:**
- `## Description` — what to build (not just a title)
- `## Acceptance` — at least one testable `- [ ]` criterion
- `**Files:**` — expected files to create/modify

**If any are missing or vague:**
- Use AskUserQuestion: "Task `<id>` spec is missing [field]. Add it before starting?"
- Do NOT spawn a worker with an incomplete spec — workers guess when specs are vague

**If all checks pass:**
```bash
$FLOWCTL start <task-id> --json
```

In parallel mode, check all ready tasks, then start them before spawning workers.

### 3c. Spawn Worker(s)

Use the Task tool to spawn a `worker` subagent. The worker gets fresh context and handles:
- Re-anchoring (reading spec, git status)
- Implementation
- Committing
- Review cycles (if enabled)
- Completing the task (flowctl done)

**Prompt template for worker:**

Pass config values only. Worker reads worker.md for phases. Do NOT paraphrase or add step-by-step instructions - worker.md has them.

```
Implement flow-code task.

TASK_ID: fn-X.Y
EPIC_ID: fn-X
FLOWCTL: /path/to/flowctl
REVIEW_MODE: none|rp|codex
RALPH_MODE: true|false
TDD_MODE: true|false

Follow your phases in worker.md exactly.
```

**Worker returns**: Summary of implementation, files changed, test results, review verdict.

**Parallel mode:** Spawn ALL ready task workers in a SINGLE message with multiple Agent tool calls. **Add `isolation: "worktree"` to each call** so each worker gets its own git worktree. Example with 3 ready tasks:

```
[Agent tool call 1: worker for fn-1.1, isolation: "worktree"]
[Agent tool call 2: worker for fn-1.2, isolation: "worktree"]
[Agent tool call 3: worker for fn-1.3, isolation: "worktree"]
```

All three run concurrently in isolated worktrees. The Agent tool automatically creates a temporary git worktree, runs the worker in it, and returns the `worktree_path` and `branch` if changes were made. flowctl state is shared across worktrees automatically (uses git-common-dir). Wait for all workers to complete before proceeding to 3c½.

**Important parallel constraints:**
- Only tasks with NO unresolved dependencies are eligible (flowctl ready guarantees this)
- Each worker commits to its own branch in its own worktree
- flowctl state (task status, locks, evidence) is shared across worktrees automatically
- After parallel batch completes, merge branches back (3c½) before finding next batch

### 3-teams. Agent Teams Execution (teams mode only)

**Skip if not `--teams` mode.**

Teams mode replaces worktree isolation with Agent Teams coordination. Workers share the working directory but have exclusive file ownership. They communicate via SendMessage.

#### 3-teams-a. Team Setup

```bash
# 1. Get file ownership map
$FLOWCTL files --epic <epic-id> --json
```

Check the `conflicts` field. If files overlap between ready tasks, those tasks **cannot run in the same wave** — demote one to the next batch.

```
# 2. Create the team
TeamCreate({team_name: "flow-<epic-id>", description: "Working on <epic-title>"})
```

#### 3-teams-b. Spawn Workers as Teammates

For each ready task with no file conflicts:

```
Agent({
  subagent_type: "flow-code:worker",
  name: "worker-<task-id>",
  description: "Implement <task-title>",
  team_name: "flow-<epic-id>",
  run_in_background: true,
  prompt: """
    Implement flow-code task.

    TASK_ID: <task-id>
    EPIC_ID: <epic-id>
    FLOWCTL: /path/to/flowctl
    REVIEW_MODE: none|rp|codex
    RALPH_MODE: true|false
    TDD_MODE: true|false
    TEAM_MODE: true
    OWNED_FILES: <comma-separated file list from flowctl files>

    Follow your phases in worker.md exactly.
  """
})
```

**Key differences from parallel mode:**
- No `isolation: "worktree"` — workers share the working directory
- Workers have `team_name` for SendMessage communication
- `TEAM_MODE: true` activates file ownership enforcement in worker
- `OWNED_FILES` lists exactly which files this worker may edit

#### 3-teams-c. Lead Coordination Loop

The main conversation acts as team lead. Monitor and coordinate:

```
While tasks remain in this wave:
  1. Workers send messages automatically when they:
     - Complete a task → verify via $FLOWCTL show <task-id> --json
     - Hit a spec conflict → pause, report to user
     - Need a file they don't own → decide: grant access or reassign
     - Get blocked → check dependency, unblock or reassign

  2. When a worker completes and goes idle:
     - Run $FLOWCTL ready --epic <epic-id> --json
     - If new tasks available and no file conflicts with active workers:
       SendMessage(to: "worker-<task-id>", message: "New assignment: <task-id>. OWNED_FILES: <files>")
     - If no tasks, let worker idle until wave completes

  3. When all workers in wave are done:
     - Proceed to 3-teams-d
```

**Do NOT micromanage** — only intervene on messages from workers. Workers handle their own phases autonomously.

#### 3-teams-d. Cleanup

```
# 1. Shutdown all workers
For each active worker:
  SendMessage(to: "worker-<task-id>", message: {"type": "shutdown_request"})

# 2. Delete team
TeamDelete({team_name: "flow-<epic-id>"})

# 3. Proceed to batch checkpoint (Phase 3d)
```

No merge-back needed — all work was on the same branch with file ownership preventing conflicts.

### 3c½. Merge Parallel Branches (parallel mode only)

**Skip if sequential mode, teams mode, or if no workers made changes.**

After all parallel workers return, each worker's changes are on a separate branch in a Claude Code-managed worktree. Merge them back to the working branch sequentially:

```bash
WORKTREE_SH="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/skills/flow-code-worktree-kit/scripts/worktree.sh"
```

For each worker that returned a branch name (in spawn order):

```bash
# 1. Merge the worker branch back (--no-ff for audit trail)
bash "$WORKTREE_SH" merge-back <worker-branch>

# 2. Clean up the branch (worktree auto-cleaned by Agent tool if merge succeeds)
git branch -d <worker-branch> 2>/dev/null || true
```

**Merge order**: Merge in the order workers were spawned. Deterministic order makes conflict debugging easier.

**Conflict handling**: If `merge-back` fails:
1. The merge is automatically aborted (working tree stays clean)
2. Log which worker branch conflicted
3. **Stop the merge sequence** — do NOT merge remaining branches
4. Report to the user: conflicting branch name + suggestion to resolve manually
5. Tasks that merged successfully proceed to 3d; failed task stays `in_progress`

**If all merges succeed**: Continue to 3d for all tasks in the batch.

### 3d. Verify Completion & Batch Checkpoint

After worker(s) return, verify each task completed:

```bash
$FLOWCTL show <task-id> --json
```

If status is not `done`, the worker failed. Check output and retry or investigate.

**Parallel mode note:** If a merge failed in 3c½, the task may show `done` in flowctl (worker completed in its worktree) but changes are not on the working branch. Flag this to the user.

#### Batch Checkpoint (parallel mode — MANDATORY after each wave)

After ALL workers in a parallel batch return and merges complete (3c½), run a structured checkpoint before finding the next wave of tasks. This prevents cascading failures and ensures integration quality.

**Step 1 — Aggregate Results:**
Collect from every worker in the batch:
- Status: done / failed / spec_conflict
- Files changed (from worker summary)
- Tests: pass / fail / skipped
- Review verdict (if REVIEW_MODE != none)

**Step 2 — Integration Verification:**
```bash
# Run guards on the merged result (catches cross-task breakage)
$FLOWCTL guard

# Check architecture invariants still hold
$FLOWCTL invariants check

# Quick integration test if available
# (use the epic's quick commands or the project's default test runner)
```

If guards or invariants fail after merge, identify which task's changes caused the regression:
1. Check `git log --oneline -<batch_size>` to see merge order
2. If identifiable, revert the offending merge and flag the task for retry
3. If ambiguous, report to user before continuing

**Step 3 — Wave Summary:**
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

**Step 4 — Wave 2 Planning:**
Before looping to 3a, assess the next batch:
- If checkpoint found integration issues → fix before spawning next wave
- If a failed task blocks downstream tasks → resolve or skip before continuing
- If remaining ready tasks have shared files with just-completed tasks → consider sequential execution to avoid conflicts

**When to STOP the wave loop:**
- Guards or invariants fail and cannot be auto-fixed → report to user
- ≥ 2 tasks in the same wave failed → likely a systemic issue, pause and investigate
- Merge conflicts in 3c½ → resolve before next wave

### 3d½. Interactive Checkpoint (if `--interactive`)

If `--interactive` was passed, pause after each task completes and show a checkpoint:

```
Checkpoint: Task <task-id> complete
  Files changed: <list from git diff --stat>
  Tests: <pass/fail>
  Review: <verdict if review ran>

Continue to next task? (y/n/skip/abort)
  y = continue (default)
  n = pause here, I'll review manually
  skip = skip remaining tasks, go to Phase 4
  abort = stop execution entirely
```

Use AskUserQuestion to wait for response. If no `--interactive` flag, skip this step entirely.

### 3e. Plan Sync (if enabled) — BOTH MODES

**Runs in SINGLE_TASK_MODE and EPIC_MODE.** Only the loop-back in 3f differs by mode.

Only run plan-sync if the task status is `done` (from step 3d). If not `done`, skip plan-sync and investigate/retry.

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

### 3f. Loop or Finish

**IMPORTANT**: Steps 3d and 3e ALWAYS run after worker returns, regardless of mode. Only the loop-back behavior differs:

**SINGLE_TASK_MODE**: After 3d→3e, go to Phase 4 (Quality). No loop.

**EPIC_MODE**: After 3d→3e, return to 3a for next task.

### 3g. Completion Review Gate (EPIC_MODE only)

When 3a finds no ready tasks, check if completion review is required.

**Check epic's completion review status directly:**

```bash
$FLOWCTL show <epic-id> --json | jq -r '.completion_review_status'
```

- If `ship` → review already passed, go to Phase 4
- If `unknown` or `needs_work` → needs review

**If review needed:**

1. Invoke `/flow-code:epic-review <epic-id>` skill
   - Pass `--review=<backend>` matching the work review backend
   - Skill handles rp/codex backend dispatch
   - Skill runs fix loop internally until SHIP verdict

2. After skill returns with SHIP:
   - Set status: `$FLOWCTL epic set-completion-review-status <epic-id> --status ship --json`
   - Go to Phase 4 (Quality)

**Note:** The epic-review skill gets SHIP from the reviewer but does NOT set the status itself. The caller (work skill or Ralph) sets `completion_review_status=ship` after successful review.

**Fix loop behavior**: Same as impl-review. If reviewer returns NEEDS_WORK:
1. Skill parses issues
2. Skill fixes code inline
3. Skill commits
4. Skill re-reviews (same chat for rp, same session for codex)
5. Repeat until SHIP

Only after SHIP does control return here. If skill outputs `<promise>RETRY</promise>`, there was a backend error - retry the skill invocation.

---

**Why spawn a worker?**

Context optimization. Each task gets fresh context:
- No bleed from previous task implementations
- Re-anchor info stays with implementation (not lost to compaction)
- Review cycles stay isolated
- Main conversation stays lean (just summaries)

**Ralph mode**: Worker inherits `bypassPermissions` from parent. FLOW_RALPH=1 and REVIEW_RECEIPT_PATH are passed through.

**Interactive mode**: Permission prompts pass through to user. Worker runs in foreground (blocking).

## Phase 4: Quality

After all tasks complete (or periodically for large epics):

- Run relevant tests
- Run lint/format per repo
- If change is large/risky, run the quality auditor subagent:
  - Task flow-code:quality-auditor("Review recent changes")
- Fix critical issues

## Phase 5: Ship

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

Then push + open PR if user wants.

**Suggest next steps:**
```
Done! Next:
1) Review the epic: `/flow-code:epic-review <epic-id>`
2) Run retrospective: `/flow-code:retro <epic-id>`
3) Start next epic: `/flow-code:work <next-epic-id>`
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

**Sequential mode:**
```
Phase 1 (resolve) → Phase 2 (branch) → Phase 3:
  ├─ 3a-c: find task → start → spawn worker
  ├─ 3d: verify done
  ├─ 3e: plan-sync (if enabled + downstream tasks exist)
  ├─ 3f: EPIC_MODE? → loop to 3a | SINGLE_TASK_MODE? → Phase 4
  ├─ no more tasks → 3g: check completion_review_status
  │   ├─ status != ship → invoke /flow-code:epic-review → fix loop until SHIP → set status=ship
  │   └─ status = ship → Phase 4
  └─ Phase 4 (quality) → Phase 5 (ship)
```

**Parallel mode (Wave-Checkpoint-Wave):**
```
Phase 1 (resolve) → Phase 2 (branch) → Phase 3:
  ├─ 3a: find ALL ready tasks (Wave 1)
  ├─ 3b: readiness check all
  ├─ 3c: spawn workers in parallel (worktree isolation)
  ├─ 3c½: merge branches back sequentially
  ├─ 3d: verify done + BATCH CHECKPOINT
  │   ├─ aggregate results (status, files, tests, reviews)
  │   ├─ integration verification (guard + invariants)
  │   ├─ wave summary report
  │   └─ wave 2 planning (assess next batch safety)
  ├─ 3d½: interactive pause (if --interactive)
  ├─ 3e: plan-sync for each completed task
  ├─ 3f: loop to 3a for next wave (or Phase 4 if done)
  ├─ no more tasks → 3g: completion review gate
  └─ Phase 4 (quality) → Phase 5 (ship)
```
