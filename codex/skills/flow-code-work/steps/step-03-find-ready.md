# Step 3: Find Ready Tasks & Readiness Check

## State Awareness (always runs first)

Every startup reads current epic state and outputs progress — this is not a special "resume mode", it is normal state reading.

```bash
# 1. Read all tasks for the epic
$FLOWCTL tasks --epic <epic-id> --json
```

Parse the JSON and output a progress summary:

```
-- Progress: <epic-id> -------------------
  Done:        3/7 (fn-N.1, fn-N.2, fn-N.3)
  In progress: 1   (fn-N.4)
  Blocked:     1   (fn-N.6)
  Ready:       2   (fn-N.5, fn-N.7)
------------------------------------------
```

## Restart Stale In-Progress Tasks

If any task has status `in_progress` but no active worker is running for it (e.g., session was interrupted), restart it so `flowctl ready` picks it up:

```bash
# For each stale in_progress task (no active worker):
$FLOWCTL restart <stale-task-id> --json
```

After restarts, find ready tasks normally:

```bash
$FLOWCTL ready <epic-id> --json
```

Collect ALL ready tasks (no unresolved dependencies). If no ready tasks, check for the completion review gate / final integration checkpoint handoff (see Step 10).

## Readiness Check

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

## Start Tasks

```bash
# 1. Start each task
$FLOWCTL start <task-id-1> --json
$FLOWCTL start <task-id-2> --json
```

## Next Step

Read `steps/step-04-spawn-workers.md` and execute.
