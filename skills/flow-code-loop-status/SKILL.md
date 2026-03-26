---
name: flow-code-loop-status
description: Show the current status of a running Ralph or auto-improve loop without interrupting it. Reads status.json from the latest run directory. Use when user asks "how's the run going", "loop status", "ralph status", "what iteration", or "/flow-code:loop-status".
user-invocable: false
---

# Loop Status

Shows the live status of running Ralph or auto-improve loops by reading `status.json` from the run directory. Non-blocking — does not interrupt the running loop.

## Input

Full request: $ARGUMENTS

| Param | Default | Description |
|-------|---------|-------------|
| type | auto-detect | `ralph` or `auto-improve` |
| `--run <id>` | `latest` | Specific run ID to check |

## Workflow

### Step 1: Find active runs

Check both Ralph and auto-improve run directories for `status.json`:

```bash
# Ralph runs
RALPH_STATUS=""
for dir in scripts/ralph/runs/latest scripts/ralph/runs; do
  if [[ -f "$dir/status.json" ]]; then
    RALPH_STATUS="$dir/status.json"
    break
  fi
done

# Auto-improve runs
AI_STATUS=""
for dir in scripts/auto-improve/runs/latest scripts/auto-improve/runs; do
  if [[ -f "$dir/status.json" ]]; then
    AI_STATUS="$dir/status.json"
    break
  fi
done
```

If user specified `--run <id>`, look in that specific run directory instead.

### Step 2: Read and display status

For each found `status.json`, read the file and format output.

**Ralph format:**
```
Ralph Run: <run_id>
Phase: <phase>  |  Iteration: <iteration>/<max_iterations>

Current: <current_id> — "<current_title>"
Progress: <tasks_done>/<tasks_total> tasks  |  <epics_done>/<epics_total> epics
Review mode: <review_mode>

Git: <git_branch>  |  <git_stats>
Updated: <relative_time> ago
```

**Auto-improve format:**
```
Auto-Improve Run: <run_id>
Goal: <goal>
Scope: <scope>

Experiment: <experiment>/<max_experiments>
Results: <kept> kept  |  <discarded> discarded  |  <crashed> crashed
Success rate: <kept/(kept+discarded+crashed)*100>%

Git: <git_branch>
Updated: <relative_time> ago
```

### Step 3: Show recent events (optional)

If the run directory contains `events.jsonl`, show the last 5 events:

```bash
tail -5 "$RUN_DIR/events.jsonl" | python3 -c "
import json, sys
for line in sys.stdin:
    try:
        e = json.loads(line.strip())
        ts = e.get('ts', '?')[-8:]  # HH:MM:SS
        event = e.get('event', '?')
        extra = {k: v for k, v in e.items() if k not in ('ts', 'level', 'event')}
        extra_str = ' '.join(f'{k}={v}' for k, v in extra.items())
        print(f'  {ts} {event} {extra_str}')
    except: pass
"
```

### Step 4: Show summary if run completed

If `status.json` shows phase `complete` or `stopped`, also check for `summary.md` and display it.

### No active runs

If no `status.json` found in either location:

```
No active loops found.

To start a loop:
  Ralph:        cd scripts/ralph && ./ralph.sh
  Auto-improve: cd scripts/auto-improve && ./auto-improve.sh
```
