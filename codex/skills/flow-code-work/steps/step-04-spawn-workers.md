# Step 4: File Locking & Spawn Workers

## Stale Lock Recovery

Stale lock detection runs at TWO points:
1. **Wave start** (before spawning workers) — catches locks from previous wave crashes
2. **Worker completion** (when any worker finishes) — catches mid-wave crashes from sibling workers

Detection criteria: lock owner task has status `done`, `failed`, `blocked`, or `skipped` but lock still held.

Recovery action:
```bash
# Check for stale locks
$FLOWCTL lock-check --stale --json

# Release stale locks
$FLOWCTL unlock --stale
```

**Wave start detection:**

```bash
# Check for any existing locks
EXISTING_LOCKS=$($FLOWCTL lock-check --all --json 2>/dev/null || echo "[]")
```

For each locked file, check if the owning task is still `in_progress`:
```bash
# If owning task is done/failed/blocked but lock was not released -> stale lock
OWNER_STATUS=$($FLOWCTL show <owner-task-id> --json | jq -r .status)
if [[ "$OWNER_STATUS" != "in_progress" ]]; then
  echo "Releasing stale lock: <file> was locked by <owner-task-id> (status: $OWNER_STATUS)"
  $FLOWCTL unlock --task <owner-task-id>
fi
```

If stale locks are found mid-wave, the coordinator releases them immediately. This prevents single-wave deadlocks where a crashed worker holds locks needed by sibling workers.

**Why**: If a worker crashes between lock and unlock (OOM, timeout, network), the lock persists. Without cleanup, the next wave deadlocks waiting for a lock that will never be released. The `TaskCompleted` hook auto-unlocks on normal completion, but crashes bypass hooks.

## File Ownership & Locking (Teams mode)

For each ready task, read file ownership from the task spec and lock:

```bash
# Read owned files from task spec's **Files:** field
TASK_SPEC=$($FLOWCTL cat <task-id>)
OWNED_FILES=$(echo "$TASK_SPEC" | grep -A20 '^\*\*Files:\*\*' | grep -oE '[a-zA-Z0-9/_.-]+\.(rs|md|toml|yml|yaml|sh|py|ts|js|json)' | paste -sd,)

# Lock files for this task (if any declared)
if [[ -n "$OWNED_FILES" ]]; then
  $FLOWCTL lock --task <task-id> --files $(echo "$OWNED_FILES" | tr ',' ' ')
  echo "Locked for <task-id>: $OWNED_FILES"
else
  echo "Warning: <task-id> has no **Files:** field — worker gets unrestricted access"
fi
```

If a task spec has no `**Files:**` field, log a warning but still spawn. Worker will have unrestricted access (backward compat).

## RP Context Detection (once per wave, before spawning workers)

Detect RP availability and set `RP_CONTEXT` for workers. This controls whether workers use `context_builder` for deep implementation context in Worker Phase 6.

```bash
# 1. Check if RP context is enabled (default: false — opt-in only)
RP_ENABLED=$($FLOWCTL config get rp_context.enabled --json 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('value', False))" 2>/dev/null || echo "False")
```

If `--no-rp-context` flag was passed OR `RP_ENABLED` is false → `RP_CONTEXT=none`.
Otherwise, detect tier via unified command:

```bash
# Pass --mcp-hint if mcp__RepoPrompt__context_builder is available in this session.
MCP_FLAG=""
# If mcp__RepoPrompt__context_builder is available -> MCP_FLAG="--mcp-hint"
RP_CONTEXT=$($FLOWCTL rp tier $MCP_FLAG)
```

## Read Project Context for Workers

If `.flow/project-context.md` exists, read the Non-Goals and Critical Implementation Rules sections. Include these in the worker prompt so workers don't violate constraints.

## Worker Prompt Generation

Use `flowctl worker-prompt --bootstrap` to generate a minimal bootstrap prompt for each worker. This outputs a ~200 token prompt that instructs the worker to call `worker-phase next` in a loop, fetching full phase instructions on demand.

```bash
# Build the bootstrap prompt — worktree isolation is the default
WORKER_PROMPT=$($FLOWCTL worker-prompt --task <task-id> --bootstrap [--tdd] [--review rp|codex])
```

## Spawn Workers (Worktree + Teams — Default)

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
    PROJECT_CONTEXT_NON_GOALS: <Non-Goals from .flow/project-context.md if exists>
    PROJECT_CONTEXT_CRITICAL_RULES: <Critical Implementation Rules from .flow/project-context.md if exists>
  "
})
```

Spawn ALL ready task workers in a SINGLE message with multiple Agent tool calls. Workers run in isolated worktrees (kernel-level file separation) with Teams coordination (SendMessage for status reporting).

**Team lifecycle**: `TeamCreate` is called ONCE per epic execution (not per wave). The same team persists across waves — workers join via spawn and leave on completion. No `TeamDelete` needed; the team is ephemeral to the session.

**Worker returns**: Summary of implementation, files changed, test results, review verdict.

### Worker Timeout

Workers have a maximum execution time of **30 minutes** per task (configurable via `.flow/config.json` -> `worker.timeout_minutes`). If a worker does not complete within this time:

1. The coordinator logs the timeout: `$FLOWCTL task fail <task-id> --reason "worker timeout after 30m"`
2. The task is marked as `failed`
3. File locks held by the timed-out worker are released: `$FLOWCTL unlock --task <task-id>`
4. The wave continues with remaining workers
5. Failed tasks can be retried in a subsequent wave via `$FLOWCTL restart <task-id>`

## Next Step

Read `steps/step-05-wave-checkpoint.md` and execute.
