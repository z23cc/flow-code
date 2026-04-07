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

1. **Flow task ID** `fn-N-slug.M` → **SINGLE_TASK_MODE**
2. **Flow epic ID** `fn-N-slug` → **EPIC_MODE**
3. **Spec file** `.md` path that exists on disk → **EPIC_MODE**
4. **Idea text** everything else → **EPIC_MODE**

**Track the mode** — it controls looping in the Wave Loop (Steps 3–13).

---

- **Flow task ID** → Read task + spec via `$FLOWCTL show/cat`, get epic context. This is the only task to execute — no loop.
- **Flow epic ID** → Clear auto-execute marker (`epic auto-exec --done`), read epic + spec, get first ready task.
- **Spec file** → Init, create epic from file, create single task, continue with epic-id.
- **Idea text** → Init, create epic + single task from idea text, continue with epic-id.

## Step 2: Apply Branch Choice & Work Mode Detection

- **Worktree** (default when on main): use `skill: flow-code-worktree-kit` to create an isolated worktree.
- **Current branch** (default when on feature branch or dirty tree): proceed in place.
- **New branch** (only if explicitly requested via `--branch=new`): checkout main, pull, create branch.

### Work Mode Detection

Run: `$FLOWCTL ready --epic <id> --json`

Count ready tasks from output.

- If `ready_count == 1` AND no `in_progress` tasks: **SINGLE_TASK_MODE**. Skip Steps 6 (file locking), 8 (merge-back), 9 (cleanup), 10 (wave checkpoint only). Worker runs directly in current branch, no worktree isolation needed.
- Otherwise: **WAVE_MODE** (execute all steps as documented).

## Wave Loop (Steps 3–13 repeat per wave)

### Wave Model

A **wave** is one batch of all currently-ready tasks. The task loop executes in waves:

```
Wave 1: [ready tasks with no deps] → spawn → wait → merge → checkpoint
Wave 2: [tasks unblocked by Wave 1] → spawn → wait → merge → checkpoint
Wave N: [remaining tasks]           → spawn → wait → merge → checkpoint
```

**Stop rules:**
- Guards or invariants fail and cannot be auto-fixed
- 2 or more tasks in the same wave failed
- No ready tasks remain (all done or blocked)

**Default mode: Worktree + Teams** — each worker gets an isolated git worktree AND runs as a Team teammate.

**CRITICAL: When multiple tasks are ready, they MUST run in parallel. Do NOT execute them sequentially.**

### Step 3. Find Ready Tasks

**State awareness:** Every startup reads current epic state via `$FLOWCTL tasks --epic <id> --json` and outputs progress summary (done/in_progress/blocked/ready counts).

**Restart stale in_progress tasks:** If any task has `in_progress` status but no active worker, restart it via `$FLOWCTL restart`.

Find ready tasks: `$FLOWCTL ready --epic <id> --json`

**Deadlock detection:** If ready tasks = 0 AND no tasks are in_progress:
- If failed tasks exist: "Epic stalled: N tasks failed, blocking downstream."
- If blocked tasks exist: "Epic stalled: N tasks blocked. Check block reasons."
- If all remaining are todo with unmet deps: "Possible circular dependency. Run validate."
- Stop the wave loop in any of these cases.

**No-progress watchdog:** Track `completed_count` at wave start. If unchanged after 2 consecutive waves, stop.

### Step 4. Readiness Check

Read each task spec via `$FLOWCTL cat <task-id>`. Validate these fields exist and are non-empty:
- `## Description` — what to build (not just a title)
- `## Acceptance` — at least one testable `- [ ]` criterion
- `**Files:**` — expected files to create/modify

**If any are missing or vague:** Use AskUserQuestion. Do NOT spawn a worker with an incomplete spec.

**Spec hash snapshot:** Record content hash for each task spec at wave start. Workers compare during re-anchor — if changed, log warning and note in evidence.

### Step 5. Start Tasks

Start each ready task via `$FLOWCTL start <task-id> --json`.

### Step 6. File Ownership & Locking (Teams mode)

Parse `**Files (write):**` and `**Files (read):**` from task specs. Lock with appropriate modes via `$FLOWCTL lock`. Read locks are shared.

**File overlap detection:** Check for conflicts across ready tasks via `$FLOWCTL files --epic <id>`. If conflicts exist:
1. Log the conflict
2. Ask user: serialize (add dep), let both proceed, or reassign files
3. Block worker spawn for conflicting tasks until resolved

**RP context detection:** Detect RP availability once per wave. Check config `rp_context.enabled`, then tier (MCP → CLI → none). Set `RP_CONTEXT` for workers.

**Worker prompt:** Use `$FLOWCTL worker-prompt --bootstrap` to generate a minimal bootstrap prompt for each worker.

### Step 7. Spawn Workers (Worktree + Teams — Default)

Enable git rerere. Create team once per epic (`TeamCreate`). Spawn all workers with `isolation: "worktree"` and `team_name` using Agent tool, all in a single message.

**Team lifecycle**: `TeamCreate` called ONCE per epic (not per wave). Same team persists across waves.

### Step 8. Wait for Workers & Merge Back

Wait for all workers. Timeout handling: if worker exceeds 45min, `$FLOWCTL fail <task-id>`.

**Merge-back:** For each worker branch, use `worktree.sh merge-back`.

**Conflict handling** (AI judgment): abort merge, classify conflict (in worker's files or not), try rebase if outside worker's files, restart task if unresolvable. Continue merging remaining branches. **Retry storm**: if >50% need retry, start fresh wave.

### Step 9. Wave Cleanup

Release file locks: `$FLOWCTL unlock --all`. Worktrees cleaned up by worktree kit.

### Step 10. Verify Completion & Checkpoint

Verify each task status via `$FLOWCTL show <task-id> --json`. If not `done`, investigate.

#### Wave Checkpoint (EPIC_MODE — MANDATORY after each wave)

**Sub-step 1 — Aggregate Results:** Collect status, files changed, test results, review verdict from every worker.

**Sub-step 2 — Integration Verification:** Run `$FLOWCTL guard` and `$FLOWCTL invariants check`. If they fail, identify which task caused the regression.

**Sub-step 3 — Wave Summary:** Output checkpoint report (tasks completed, files changed, guards/invariants status, issues, next ready).

**When to STOP:** Guards/invariants fail unfixably, or 2+ tasks in same wave failed.

### Step 11. Interactive Checkpoint (if `--interactive`)

Pause after each task, show checkpoint (files changed, tests, review). Ask: continue/pause/skip/abort.

### Step 12. Plan Sync (if enabled)

Runs in both modes. Only if task status is `done` and `planSync.enabled` is `true`. Get todo tasks, spawn `plan-sync` subagent for downstream spec updates.

### Step 13. Loop or Finish

- **SINGLE_TASK_MODE**: Go to Step 15. No loop.
- **EPIC_MODE**: Return to Step 3 for next wave.

### Step 14. Adversarial Review (EPIC_MODE only — Layer 3)

When no ready tasks remain and all tasks are done. A different model family (GPT via Codex) tries to break the code.

- If codex available: scope diff to epic changes, run `$FLOWCTL codex adversarial`. Max 2 iterations.
- If codex unavailable: skip Layer 3 (different model family is the point — no fallback to RP).
- After SHIP: `$FLOWCTL epic completion <epic-id> ship --json`

## Step 15: Quality

After all tasks complete: run tests, lint/format, quality auditor subagent if large/risky, fix critical issues.

## Step 16: Ship

Verify all tasks done (`$FLOWCTL show/validate`). Final commit if needed. Do NOT close epic unless user asked. Auto push + draft PR (default, `--no-pr` to skip) using `prompts/pr-body.md` template. Output session summary (epic, tasks, commits, duration, quality, PR URL). Suggest next: retro, next epic.

## Definition of Done

- All tasks have status "done"
- `$FLOWCTL validate --epic <id>` passes
- Tests pass
- Lint/format pass
- Docs updated if needed
- Working tree is clean

## Example flow

```
Step 1 (resolve) → Step 2 (branch + mode detect) → Wave Loop:
  ├─ Step 3: state + progress, restart stale, find ready
  ├─ Step 4: readiness check
  ├─ Step 5: start tasks
  ├─ Step 6: file locking (WAVE_MODE only)
  ├─ Step 7: spawn workers
  ├─ Step 8: wait + merge back (WAVE_MODE only)
  ├─ Step 9: cleanup (WAVE_MODE only)
  ├─ Step 10: verify + checkpoint
  ├─ Step 11: interactive pause (if --interactive)
  ├─ Step 12: plan-sync (if enabled)
  ├─ Step 13: loop or finish
  ├─ Step 14: adversarial review (EPIC_MODE, all done)
  └─ Step 15 (quality) → Step 16 (ship)
```
