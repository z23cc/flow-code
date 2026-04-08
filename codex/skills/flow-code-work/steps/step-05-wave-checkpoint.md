# Step 5: Wave Checkpoint, Loop & Ship

## Wait for Workers & Merge Back

Wait for all workers to complete.

**Merge-back** (after all workers return):

```bash
WORKTREE_SH="$HOME/.codex/skills/flow-code-worktree-kit/scripts/worktree.sh"
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

## Wave Cleanup

Release file locks so the next wave can re-lock with new ownership:

```bash
$FLOWCTL unlock --all
```

Worktrees are cleaned up automatically by the worktree kit.

## Verify Completion & Checkpoint

After worker(s) return, verify each task completed:

```bash
$FLOWCTL show <task-id> --json
```

If status is not `done`, the worker failed. Check output and retry or investigate.

### Wave Checkpoint (EPIC_MODE — MANDATORY after each wave)

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

### Guard Execution Points

Guard (`$FLOWCTL guard`) runs at these specific points:

| When | Where | On Failure |
|------|-------|------------|
| Worker Phase 6 (self-review) | Per-task, after implementation | Worker retries fix (max 2 attempts), then marks task as failed |
| Wave checkpoint (Step 10) | After all wave workers complete | Stop pipeline — do not advance to next wave |
| Close phase | Final validation before PR | Block close until guard passes |

Guard auto-detects the project stack and runs: linter, type checker, and test suite. It does NOT run on every git commit automatically — it runs at the phase boundaries listed above.

**Sub-step 3 — Wave Summary:**
Output a concise checkpoint report:
```
-- Wave N Checkpoint -------------------------
  Tasks completed: 3/3 (fn-1.1, fn-1.2, fn-1.3)
  Files changed:   12
  Guards:          pass
  Invariants:      pass
  Issues:          none
  Next ready:      fn-1.4, fn-1.5
----------------------------------------------
```

**When to STOP the wave loop:**
- Guards or invariants fail and cannot be auto-fixed -> report to user
- >= 2 tasks in the same wave failed -> likely a systemic issue, pause and investigate

## Interactive Checkpoint (if `--interactive`)

If `--interactive` was passed, pause after each task completes and show a checkpoint:

```
Checkpoint: Task <task-id> complete
  Files changed: <list from git diff --stat>
  Tests: <pass/fail>
  Review: <verdict if review ran>

Continue to next task? (y/n/skip/abort)
  y = continue (default)
  n = pause here, I'll review manually
  skip = skip remaining tasks, go to Quality
  abort = stop execution entirely
```

Use AskUserQuestion to wait for response. If no `--interactive` flag, skip this step entirely.

## Plan Sync (if enabled) — BOTH MODES

**Runs in SINGLE_TASK_MODE and EPIC_MODE.** Only the loop-back differs by mode.

Only run plan-sync if the task status is `done` (from verification above). If not `done`, skip plan-sync and investigate/retry.

Check if plan-sync should run:

```bash
$FLOWCTL config get planSync.enabled --json
```

Plan-sync is **enabled by default** (default config: `planSync.enabled: true`). Skip only if explicitly set to `false`.

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

Check cross-epic sync setting:
```bash
CROSS_EPIC=$($FLOWCTL config get planSync.crossEpic --json 2>/dev/null | jq -r '.value // false')
```

Use the Task tool to spawn the `plan-sync` subagent with this prompt:

```
Sync downstream tasks after implementation.

COMPLETED_TASK_ID: fn-X.Y
EPIC_ID: fn-X
FLOWCTL: /path/to/flowctl
DOWNSTREAM_TASK_IDS: fn-X.3,fn-X.4,fn-X.5
CROSS_EPIC: $CROSS_EPIC

Follow your phases in plan-sync.md exactly.
```

**Cross-epic sync** (enabled by default): When `planSync.crossEpic` is `true`, plan-sync also checks other open epics for stale references to the completed task's APIs. Disable via:
```bash
$FLOWCTL config set planSync.crossEpic false
```
Disable for single-epic work to save tokens. Keep enabled (default) for multi-epic projects where epics share APIs.

Plan-sync returns a summary. **Check the result before advancing:**

- **"Drift detected: yes"** with updates applied -> log changes, proceed.
- **"Drift detected: no"** -> proceed (no drift = fast path).
- **Plan-sync fails or times out** -> **STOP the wave**. Do NOT advance to next wave with stale specs. Log the failure and report to user:
  ```
  Plan-sync failed after <COMPLETED_TASK_ID>. Downstream specs may be stale.
  Manual fix: run /flow-code:sync <EPIC_ID> or inspect specs manually.
  ```
  This prevents implementation drift from silently propagating to downstream tasks.

## Loop or Finish

**SINGLE_TASK_MODE**: After verification and plan-sync, go to Quality & Ship. No loop.

**EPIC_MODE**: After verification and plan-sync, return to Step 3 (Find Ready Tasks) for next wave.

## Adversarial Review (EPIC_MODE only — Layer 3)

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
- `verdict: "SHIP"` -> go to Quality & Ship
- `verdict: "NEEDS_WORK"` -> increment `ADVERSARIAL_ITERATIONS`. If `>= 2`: log "Adversarial review: 2 iterations completed. First iteration finds real issues, second verifies fixes. Proceeding." -> go to Quality & Ship. Otherwise: fix issues, commit, re-run.

**If codex not available:**
```
Warning: Codex CLI not found — skipping Layer 3 adversarial review.
  Install: npm install -g @openai/codex
```
Go to Quality & Ship directly. No fallback to RP — different model family is the point.

**After SHIP (or skip):**
```bash
$FLOWCTL epic completion <epic-id> ship --json
```

## Quality & Pre-Launch Checklist

After all tasks complete (or periodically for large epics):

- Run `$FLOWCTL guard` (lint + type + test must pass)
- If change is large/risky, run the quality auditor subagent:
  - Task flow-code:quality-auditor("Review recent changes")
- Fix critical issues

**Pre-launch checklist** — verify all applicable dimensions before shipping:
- **Code quality**: guard passes, no Critical/Important review findings open
- **Security**: no secrets in code, input validated at boundaries (see `references/security-checklist.md`)
- **Performance**: no N+1 queries, list endpoints paginated (see `references/performance-checklist.md`)
- **Accessibility**: keyboard navigable, screen reader compatible (frontend changes — see `references/accessibility-checklist.md`)
- **Infrastructure**: env vars documented, migrations reversible, feature flags configured
- **Documentation**: README/CHANGELOG updated if user-facing, API docs match implementation

Skip dimensions not applicable to the change (e.g., skip accessibility for backend-only epics).

## Ship

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
  $FLOWCTL show <epic-id> --json   # get title + spec -> {{epic_overview}}
  $FLOWCTL tasks --epic <epic-id> --json  # get task list -> {{task_list}}
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
-- Session Summary ----------------------------
  Epic: <epic-id> "<title>"
  Tasks: N completed, M skipped
  Commits: K
  Duration: Xm Ys (from first task start to now)
  Quality:
    Layer 1 (guard): <pass/fail/nothing to run>
    Layer 3 (adversarial): <SHIP/skipped>
  PR: <URL or "skipped">
-----------------------------------------------
```

**Suggest next steps:**
```
Done! Next:
1) Run retrospective: /flow-code:retro <epic-id>
2) Start next epic: /flow-code:work <next-epic-id>
```

## Definition of Done

Confirm before ship:
- All tasks have status "done"
- `$FLOWCTL validate --epic <id>` passes
- Tests pass
- Lint/format pass
- Docs updated if needed
- Working tree is clean
