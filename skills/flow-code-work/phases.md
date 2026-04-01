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

**Default mode is Teams** — workers run as Agent Team teammates with shared directory and file locking. This applies to both single-task and multi-task execution.

**Fallback: worktree isolation** (`--worktree-parallel`): Uses git worktrees instead of Teams. Only use when Teams is unavailable or user explicitly requests worktree isolation.

### 3a. Find Ready Tasks

```bash
$FLOWCTL ready --epic <epic-id> --json
```

Collect ALL ready tasks (no unresolved dependencies). If no ready tasks, check for completion review gate (see 3g below).

### 3b. Readiness Check

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

### 3c. Teams Setup & File Locking

```bash
# 1. Get file ownership map and check for conflicts
$FLOWCTL files --epic <epic-id> --json
```

Check the `conflicts` field. If files overlap between ready tasks, those tasks **cannot run in the same wave** — demote one to the next batch.

```bash
# 2. Start tasks and lock files for each (atomic — prevents concurrent edits)
$FLOWCTL start <task-id-1> --json
$FLOWCTL lock --task <task-id-1> --files "file1,file2" --json

$FLOWCTL start <task-id-2> --json
$FLOWCTL lock --task <task-id-2> --files "file3,file4" --json
# Check conflict_count in response — if >0, a file is already locked by another task
```

```
# 3. Create the team (skip if only 1 task — single worker doesn't need Teams overhead)
TeamCreate({team_name: "flow-<epic-id>", description: "Working on <epic-title>"})
```

> **Single ready task?** Skip TeamCreate. Spawn one worker with `TEAM_MODE: false` and `run_in_background: false` (foreground). This avoids Teams overhead while still using file locking for safety.

### 3d. Spawn Workers

**Prompt template for worker:**

Pass config values only. Worker reads worker.md for phases. Do NOT paraphrase or add step-by-step instructions - worker.md has them.

**Multiple ready tasks (Teams mode):**

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

Spawn ALL ready task workers in a SINGLE message with multiple Agent tool calls.

**Single ready task (no Teams overhead):**

```
Agent({
  subagent_type: "flow-code:worker",
  description: "Implement <task-title>",
  prompt: """
    Implement flow-code task.

    TASK_ID: <task-id>
    EPIC_ID: <epic-id>
    FLOWCTL: /path/to/flowctl
    REVIEW_MODE: none|rp|codex
    RALPH_MODE: true|false
    TDD_MODE: true|false

    Follow your phases in worker.md exactly.
  """
})
```

No `team_name`, no `TEAM_MODE`, no `OWNED_FILES`. Worker runs in foreground.

**Worker returns**: Summary of implementation, files changed, test results, review verdict.

### 3e. Lead Coordination Loop (Teams mode — multiple workers)

**Skip if only 1 worker (foreground mode).**

The main conversation acts as team lead. Worker↔lead communication uses **plain text** SendMessage with structured `summary` prefixes for routing.

> **Why plain text?** Claude Code's SendMessage `message` field only accepts strings or 3 native types (`shutdown_request`, `shutdown_response`, `plan_approval_response`). Custom JSON objects are rejected by schema validation. Plain text with consistent summary prefixes is reliable and parseable.

**Worker → Lead message types (detect by summary prefix):**

| Summary prefix | Meaning | Action |
|----------------|---------|--------|
| `"Task complete: <id>"` | Worker finished task | Verify via `$FLOWCTL show <id> --json`, assign next task |
| `"Spec conflict: <id>"` | Spec wrong/contradicts codebase | Pause, report to user |
| `"Blocked: <id>"` | Dependency or external blocker | Check blocker, unblock or reassign |
| `"Need file access: <file>"` | Worker needs unowned file | Check owner, grant or deny |

**Lead → Worker message types:**

| Action | Summary prefix | Message format |
|--------|---------------|----------------|
| Assign new task | `"New task: <id>"` | Plain text with TASK_ID, OWNED_FILES, FLOWCTL path, and instruction to re-anchor |
| Grant file access | `"Access granted: <file>"` | `"Access granted for <file>. You may now edit it."` |
| Deny file access | `"Access denied: <file>"` | `"Access denied for <file>. Reason: <why>. Find an alternative approach."` |
| Shutdown | `"Shutdown"` | Native `{"type": "shutdown_request"}` object (schema-supported) |

**Coordination loop:**

```
While tasks remain in this wave:
  1. Route incoming worker messages by summary prefix:

     "Task complete: <id>":
       → Verify: $FLOWCTL show <id> --json (status must be "done")
       → Unlock completed task's files: $FLOWCTL unlock --task <id> --json
       → Check for next ready task (see step 2)

     "Spec conflict: <id>":
       → Pause wave, report details to user
       → Wait for user resolution before continuing

     "Blocked: <id>":
       → Parse message body for blocker info
       → If in-flight task: wait for it to complete, then notify blocked worker
       → If external: report to user

     "Need file access: <file>":
       → Check lock status: $FLOWCTL lock-check --file <file> --json
       → If not locked or owner task is done:
         → $FLOWCTL lock --task <requesting-task-id> --files <file> --json
         → SendMessage(to: "worker-<task-id>", summary: "Access granted: <file>",
             message: "Access granted for <file>. You may now edit it.")
       → If locked by active task:
         → SendMessage(to: "worker-<task-id>", summary: "Access denied: <file>",
             message: "Access denied for <file>. Locked by <owner-task-id>. Find an alternative approach.")

  2. When a worker completes and goes idle:
     → Run $FLOWCTL ready --epic <epic-id> --json
     → If new tasks available and no file conflicts with active workers:
       → $FLOWCTL start <new-task-id> --json
       → $FLOWCTL lock --task <new-task-id> --files <comma-separated files> --json
       → SendMessage(to: "worker-<task-id>", summary: "New task: <new-task-id>",
           message: "New task assigned.\n\nTASK_ID: <new-task-id>\nOWNED_FILES: <comma-separated files>\n\nRead spec: $FLOWCTL cat <new-task-id>\nFollow your worker phases to implement it.")
     → If no tasks, let worker idle until wave completes

  3. When all workers in wave are done:
     → Proceed to 3f (cleanup)
```

**Do NOT micromanage** — only intervene on protocol messages from workers. Workers handle their own phases autonomously.

**Idle detection**: Claude Code automatically sends `idle_notification` via the built-in Stop hook when a teammate finishes its turn. Use this as a secondary signal that a worker is ready for reassignment.

### 3f. Wave Cleanup

**Teams mode (multiple workers):**
```
# 1. Shutdown all workers (native schema type)
For each active worker:
  SendMessage(to: "worker-<task-id>", message: {"type": "shutdown_request"})

# 2. Unlock all file locks for this wave
$FLOWCTL unlock --all --json

# 3. Delete team
TeamDelete()
```

**Single worker mode:** Just unlock files:
```bash
$FLOWCTL unlock --all --json
```

No merge-back needed — all work is on the same branch with file ownership preventing conflicts.

### 3g. Verify Completion & Checkpoint

After worker(s) return, verify each task completed:

```bash
$FLOWCTL show <task-id> --json
```

If status is not `done`, the worker failed. Check output and retry or investigate.

#### Wave Checkpoint (EPIC_MODE — MANDATORY after each wave)

After ALL workers in a wave return, run a structured checkpoint before finding the next wave of tasks. This prevents cascading failures and ensures integration quality.

**Step 1 — Aggregate Results:**
Collect from every worker in the batch:
- Status: done / failed / spec_conflict
- Files changed (from worker summary)
- Tests: pass / fail / skipped
- Review verdict (if REVIEW_MODE != none)

**Step 2 — Integration Verification:**
```bash
# Run guards on the result (catches cross-task breakage)
$FLOWCTL guard

# Check architecture invariants still hold
$FLOWCTL invariants check
```

If guards or invariants fail, identify which task's changes caused the regression and report to user.

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

**When to STOP the wave loop:**
- Guards or invariants fail and cannot be auto-fixed → report to user
- ≥ 2 tasks in the same wave failed → likely a systemic issue, pause and investigate

### 3g½. Interactive Checkpoint (if `--interactive`)

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

### 3h. Plan Sync (if enabled) — BOTH MODES

**Runs in SINGLE_TASK_MODE and EPIC_MODE.** Only the loop-back in 3i differs by mode.

Only run plan-sync if the task status is `done` (from step 3g). If not `done`, skip plan-sync and investigate/retry.

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

### 3i. Loop or Finish

**SINGLE_TASK_MODE**: After 3g→3h, go to Phase 4 (Quality). No loop.

**EPIC_MODE**: After 3g→3h, return to 3a for next wave.

### 3j. Completion Review Gate (EPIC_MODE only)

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

---

### Worktree Parallel Fallback (`--worktree-parallel`)

**Only use when Teams is unavailable or user explicitly passes `--worktree-parallel`.**

Instead of Teams coordination, each worker gets an isolated git worktree:

```
[Agent tool call 1: worker for fn-1.1, isolation: "worktree"]
[Agent tool call 2: worker for fn-1.2, isolation: "worktree"]
[Agent tool call 3: worker for fn-1.3, isolation: "worktree"]
```

All run concurrently in isolated worktrees. flowctl state is shared across worktrees automatically (uses git-common-dir). Wait for all workers to complete.

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
1. The merge is automatically aborted (working tree stays clean)
2. Log which worker branch conflicted
3. **Stop the merge sequence** — do NOT merge remaining branches
4. Report to the user: conflicting branch name + suggestion to resolve manually

After merge-back, proceed to 3g (Verify Completion).

---

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

**Default mode (Teams — auto-parallel):**
```
Phase 1 (resolve) → Phase 2 (branch) → Phase 3:
  ├─ 3a: find ALL ready tasks
  ├─ 3b: readiness check
  ├─ 3c: lock files + create team (if >1 task)
  ├─ 3d: spawn workers (parallel if >1, foreground if 1)
  ├─ 3e: lead coordination loop (if Teams)
  ├─ 3f: cleanup (shutdown workers, unlock, delete team)
  ├─ 3g: verify done + wave checkpoint
  ├─ 3g½: interactive pause (if --interactive)
  ├─ 3h: plan-sync (if enabled + downstream tasks exist)
  ├─ 3i: EPIC_MODE? → loop to 3a | SINGLE_TASK_MODE? → Phase 4
  ├─ no more tasks → 3j: completion review gate
  └─ Phase 4 (quality) → Phase 5 (ship)
```
