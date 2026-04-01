#!/usr/bin/env bash
# commit-gate.sh — PreToolUse hook that blocks git commit when flow task
# is in_progress but guard has not been run.
#
# Stdin: JSON with hook_event_name, tool_name, tool_input.command
# Exit 0 = allow, Exit 2 = block
#
# Also handles PostToolUse to track guard pass state.

set -euo pipefail

# No .flow directory → not a flow-code project, allow everything
[ -d ".flow" ] || exit 0

# Read stdin JSON
INPUT=$(cat)

EVENT=$(echo "$INPUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('hook_event_name',''))" 2>/dev/null || echo "")
COMMAND=$(echo "$INPUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('tool_input',{}).get('command',''))" 2>/dev/null || echo "")

# State file keyed by .flow directory (absolute path hash)
FLOW_DIR="$(cd .flow && pwd)"
STATE_FILE="/tmp/flow-commit-gate-$(echo "$FLOW_DIR" | md5 -q 2>/dev/null || echo "$FLOW_DIR" | md5sum | cut -d' ' -f1)"

# --- PostToolUse: track guard pass ---
if [ "$EVENT" = "PostToolUse" ]; then
    # Check if command was flowctl guard and it succeeded
    if echo "$COMMAND" | grep -qE '(flowctl|flowctl\.py)\s+guard'; then
        GUARD_PASSED=$(echo "$INPUT" | python3 -c "
import json, sys
data = json.load(sys.stdin)
resp = data.get('tool_response', {})
text = resp.get('stdout', str(resp)) if isinstance(resp, dict) else str(resp)
# Guard passes when output contains 'guards passed' without 'FAILED'
if 'guards passed' in text.lower() and 'failed' not in text.lower():
    print('yes')
elif 'nothing to run' in text.lower() or 'no stack detected' in text.lower():
    print('yes')  # No guards configured = not blocked
else:
    print('no')
" 2>/dev/null || echo "no")
        if [ "$GUARD_PASSED" = "yes" ]; then
            echo "$(date +%s)" > "$STATE_FILE"
        fi
    fi
    exit 0
fi

# --- PreToolUse: gate git commit ---
if [ "$EVENT" != "PreToolUse" ]; then
    exit 0
fi

# Only care about git commit commands
if ! echo "$COMMAND" | grep -qE '\bgit\s+commit\b'; then
    exit 0
fi

# Check: is any task in_progress?
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-}}/scripts/flowctl.py"
if [ -z "${DROID_PLUGIN_ROOT:-}" ] && [ -z "${CLAUDE_PLUGIN_ROOT:-}" ]; then
    # Plugin root not set — can't check task state, allow
    exit 0
fi

IN_PROGRESS=$(python3 "$FLOWCTL" tasks --json 2>/dev/null | python3 -c "
import json, sys
data = json.load(sys.stdin)
tasks = data.get('tasks', [])
active = [t for t in tasks if t.get('status') == 'in_progress']
print(len(active))
" 2>/dev/null || echo "0")

# No task in_progress → manual commit outside flow workflow, allow
if [ "$IN_PROGRESS" = "0" ]; then
    exit 0
fi

# Task in_progress → check guard evidence
if [ -f "$STATE_FILE" ]; then
    GUARD_TIME=$(cat "$STATE_FILE")
    NOW=$(date +%s)
    AGE=$(( NOW - GUARD_TIME ))
    # Guard evidence valid for 10 minutes (600 seconds)
    if [ "$AGE" -lt 600 ]; then
        # Guard was recently run and passed — allow commit
        # Consume the marker so next commit also requires guard
        rm -f "$STATE_FILE"
        exit 0
    fi
fi

# Block: task in_progress but no recent guard pass
echo "BLOCKED: git commit requires passing guard first." >&2
echo "A task is in_progress but flowctl guard has not been run (or passed) recently." >&2
echo "Run: flowctl guard" >&2
exit 2
