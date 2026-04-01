#!/usr/bin/env bash
# TaskCompleted hook: sync Claude task completion with .flow/ state
# Bridges Claude's built-in task system with flow-code tracking
#
# Input (stdin JSON from Claude Code):
#   hook_event_name: "TaskCompleted"
#   task_id:         Claude's internal task ID
#   task_subject:    Task title/subject
#   task_description: Optional description
#   teammate_name:   Worker name (Teams mode, e.g., "worker-fn-1.2")
#   team_name:       Team name (e.g., "flow-fn-1")
#
# Actions:
#   1. Parse flow task ID from teammate_name or task_subject
#   2. If task is in_progress in .flow, unlock its files
#   3. Log event to .flow/hooks-log/

set -uo pipefail

[ -d ".flow" ] || exit 0

FLOWCTL="${CLAUDE_PLUGIN_ROOT:-${DROID_PLUGIN_ROOT:-$(cd "$(dirname "$(dirname "$0")")" && pwd)}}/scripts/flowctl.py"
[ -f "$FLOWCTL" ] || exit 0
command -v python3 &>/dev/null || exit 0

# Read hook input from stdin
INPUT=$(cat)

# Parse fields
TEAMMATE_NAME=$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('teammate_name',''))" 2>/dev/null || echo "")
TEAM_NAME=$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('team_name',''))" 2>/dev/null || echo "")
TASK_SUBJECT=$(echo "$INPUT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('task_subject',''))" 2>/dev/null || echo "")

# Extract flow task ID from teammate_name (e.g., "worker-fn-1-add-auth.2" → "fn-1-add-auth.2")
FLOW_TASK_ID=""
if [[ -n "$TEAMMATE_NAME" ]]; then
    # Strip "worker-" prefix to get flow task ID
    FLOW_TASK_ID="${TEAMMATE_NAME#worker-}"
fi

# Fallback: try to extract from task_subject (e.g., "Task complete: fn-1.2")
if [[ -z "$FLOW_TASK_ID" || ! "$FLOW_TASK_ID" =~ ^fn- ]]; then
    FLOW_TASK_ID=$(echo "$TASK_SUBJECT" | grep -oE 'fn-[a-z0-9-]+\.[0-9]+' | head -1 || echo "")
fi

# Ensure hooks-log directory exists
LOG_DIR=".flow/hooks-log"
mkdir -p "$LOG_DIR"

# Log the event
TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "{\"event\":\"task_completed\",\"time\":\"$TIMESTAMP\",\"teammate\":\"$TEAMMATE_NAME\",\"team\":\"$TEAM_NAME\",\"flow_task\":\"$FLOW_TASK_ID\",\"subject\":\"$TASK_SUBJECT\"}" >> "$LOG_DIR/events.jsonl"

# If we identified a flow task, unlock its files
if [[ -n "$FLOW_TASK_ID" && "$FLOW_TASK_ID" =~ ^fn- ]]; then
    # Check if task exists and is in_progress
    STATUS=$(python3 "$FLOWCTL" show "$FLOW_TASK_ID" --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null || echo "")

    if [[ "$STATUS" == "in_progress" || "$STATUS" == "done" ]]; then
        # Unlock files for this task (safe even if no locks exist)
        python3 "$FLOWCTL" unlock --task "$FLOW_TASK_ID" --json 2>/dev/null || true
        echo "{\"event\":\"files_unlocked\",\"time\":\"$TIMESTAMP\",\"task\":\"$FLOW_TASK_ID\"}" >> "$LOG_DIR/events.jsonl"
    fi
fi
