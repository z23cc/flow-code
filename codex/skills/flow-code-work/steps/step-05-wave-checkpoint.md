# Step 5: Integration Checkpoint & Ship

## Integration Checkpoint (MANDATORY after streaming loop completes)

Step 4's continuous loop handled: merge, steer, per-worker cleanup, and continuous spawning.
This step runs ONCE after all tasks are done (ACTIVE_SESSIONS is empty).

### Sub-step 1 — Aggregate Results

Collect across ALL tasks (not per-wave — there are no waves):
- Total tasks: done / failed (from `flowctl show <epic-id> --json`)
- Files changed (from git log)
- Tests: pass / fail / skipped

### Sub-step 2 — Integration Verification

```bash
$FLOWCTL guard
$FLOWCTL invariants check
```

If guard fails, identify which merge introduced the regression (`git log --oneline`).

### Sub-step 3 — Summary

```
-- Checkpoint --------------------------------
  Tasks completed: 5/5
  Files changed:   18
  Guards:          pass
  Invariants:      pass
  Issues:          none
----------------------------------------------
```

### Guard Execution Points

| When | Where | On Failure |
|------|-------|------------|
| Worker Phase 6 | Per-task, in worktree | Worker retries (max 2), then fails |
| After streaming loop | Integration check | Stop pipeline |
| Close phase | Final validation | Block close until passes |

## Plan Sync (if enabled)

```bash
PLAN_SYNC_ENABLED=$($FLOWCTL config get planSync.enabled --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('value', True))" 2>/dev/null || echo "True")
```

If enabled, spawn plan-sync for completed tasks with downstream deps (across epics if `planSync.crossEpic` is true):

```
mcp__RepoPrompt__agent_run({
  op: "start",
  model_id: "explore",
  session_name: "plan-sync-<task-id>",
  message: "Sync downstream tasks. COMPLETED_TASK_ID: <id> EPIC_ID: <id> FLOWCTL: $HOME/.flow/bin/flowctl DOWNSTREAM_TASK_IDS: <ids>",
  timeout: 120
})
```

## Adversarial Review (Layer 3)

```bash
which codex >/dev/null 2>&1 && {
  BRANCH_BASE=$(git merge-base main HEAD)
  $FLOWCTL codex adversarial --base "$BRANCH_BASE" --json
}
```

- `SHIP` → proceed
- `NEEDS_WORK` → fix, re-run (max 2 iterations)
- Codex not available → skip with warning

```bash
$FLOWCTL epic completion <epic-id> ship --json
```

## Quality & Ship

```bash
$FLOWCTL guard
$FLOWCTL pre-launch --json
$FLOWCTL validate --epic <epic-id> --json
```

Final commit, push, and draft PR (unless `--no-pr`).
