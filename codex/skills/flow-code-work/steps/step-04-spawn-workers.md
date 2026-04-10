# Step 4: File Locking & Spawn Workers via RP agent_run

## Stale Lock Recovery

```bash
$FLOWCTL lock-check --stale --json
$FLOWCTL unlock --stale
```

## File Ownership & Locking

For each ready task:

```bash
TASK_SPEC=$($FLOWCTL cat <task-id>)
OWNED_FILES=$(echo "$TASK_SPEC" | grep -A20 '^\*\*Files:\*\*' | grep -oE '[a-zA-Z0-9/_.-]+\.(rs|md|toml|yml|yaml|sh|py|ts|js|json)' | paste -sd,)

if [[ -n "$OWNED_FILES" ]]; then
  $FLOWCTL lock --task <task-id> --files $(echo "$OWNED_FILES" | tr ',' ' ')
fi
```

## Spawn Workers

The coordinator's job is minimal: **start the agent and wait for a commit hash.**

Workers self-manage their worktree lifecycle (create → cd → work → commit). No `manage_workspaces` or `bind_context` needed — RP agents are independent bash processes.

```bash
# Generate prompt
WORKER_PROMPT=$($FLOWCTL worker-prompt --task <task-id> --bootstrap [--tdd] [--review rp|codex])
REPO_ROOT=$(pwd)
```

```
mcp__RepoPrompt__agent_run({
  op: "start",
  model_id: "<see model guide>",
  session_name: "worker-<task-id>",
  message: "$WORKER_PROMPT

REPO_ROOT: $REPO_ROOT
OWNED_FILES: $OWNED_FILES

Your FIRST ACTIONS (before any worker-phase):
  cd $REPO_ROOT
  git worktree add .claude/worktrees/worker-$TASK_ID HEAD
  cd .claude/worktrees/worker-$TASK_ID

Your LAST ACTION (after all phases complete):
  Output COMMIT_HASH=$(git rev-parse HEAD)

EPIC SPEC SUMMARY:
<paste from flowctl cat epic-id>

PROJECT CONTEXT:
<paste Non-Goals + Critical Rules from .flow/project-context.md>",
  detach: true
})
→ save session_id into ACTIVE_SESSIONS[]
```

**Model selection:**

| Task complexity | model_id | Model |
|----------------|----------|-------|
| Standard | `engineer` | Codex GPT-5.3 |
| Complex/architectural | `pair` | GPT-5.4 High |
| Research-heavy | `explore` | GPT-5.4 Mini |

Spawn ALL ready tasks, collect `session_id`s.

## Continuous Streaming Loop

**No wave boundaries.** Merge each worker as it completes, immediately spawn any newly unblocked tasks, and keep going until no tasks remain.

```
ACTIVE_SESSIONS = {session_id → task_id}
ALL_SESSION_IDS = []  # for final cleanup

# Initial spawn (all currently ready tasks)
for each ready task:
  spawn worker → save session_id into ACTIVE_SESSIONS
  ALL_SESSION_IDS.push(session_id)

while ACTIVE_SESSIONS is not empty:

  # Wait for ANY one to reach terminal/input state
  result = agent_run(wait,
    session_ids: keys(ACTIVE_SESSIONS),
    timeout: 1800)

  # Handle Codex sandbox approval
  if result.status == "waiting_for_input":
    agent_run(respond, session_id: result.session_id,
      interaction_id: result.interaction_id,
      response: "accept_for_session")
    continue

  finished_task = ACTIVE_SESSIONS[result.session_id]

  # 1. Parse COMMIT_HASH (with fallback)
  COMMIT_HASH = <parse "COMMIT_HASH:" from output>
  if empty: COMMIT_HASH = $(git -C ".claude/worktrees/worker-$finished_task" rev-parse HEAD)

  # 2. Verify + merge + cleanup
  flowctl show $finished_task --json
  git merge --no-ff $COMMIT_HASH -m "merge: $finished_task"
  git worktree remove ".claude/worktrees/worker-$finished_task" --force
  flowctl unlock --task $finished_task

  # 3. Steer remaining workers
  for each remaining in ACTIVE_SESSIONS:
    agent_run(steer, session_id: remaining,
      message: "$finished_task merged, changed: $FILES",
      wait: false)

  ACTIVE_SESSIONS.remove(result.session_id)

  # 4. CONTINUOUS SPAWN — check for newly unblocked tasks
  NEWLY_READY = flowctl ready $EPIC_ID --json
  for each new_task in NEWLY_READY (not already spawned):
    flowctl start $new_task --force --json
    flowctl lock --task $new_task --files ...
    spawn worker → save session_id into ACTIVE_SESSIONS
    ALL_SESSION_IDS.push(session_id)
```

**Key difference from wave model:** Tasks that depend on only ONE completed task start immediately — they don't wait for the entire "wave" to finish. This eliminates idle time between waves.

**Conflict handling**: If merge fails → `git merge --abort`, log, continue loop.

**Timeout**: `agent_run(cancel)` → `flowctl task fail` → `flowctl unlock` → `git worktree remove`.

## Next Step

Read `steps/step-05-wave-checkpoint.md` and execute.
