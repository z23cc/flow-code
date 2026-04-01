#!/usr/bin/env bash
# commit-gate.sh — PreToolUse hook that blocks git commit when flow task
# is in_progress but guard has not been run.
#
# Stdin: JSON with hook_event_name, tool_name, tool_input.command
# Exit 0 = allow, Exit 2 = block
#
# Also handles PostToolUse to track guard pass state.

set -uo pipefail

# No .flow directory → not a flow-code project, allow everything
[ -d ".flow" ] || exit 0

# Read stdin JSON once
INPUT=$(cat)

# Parse all fields in one python call (efficient)
eval "$(echo "$INPUT" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    event = data.get('hook_event_name', '')
    command = data.get('tool_input', {}).get('command', '')
    session = data.get('session_id', '')
    print(f'EVENT=\"{event}\"')
    print(f'COMMAND={repr(command)}')
    # Subagent detection: teammate sessions have @ in ID
    print(f'IS_SUBAGENT={\"yes\" if \"@\" in session else \"no\"}')
except Exception:
    print('EVENT=\"\"')
    print('COMMAND=\"\"')
    print('IS_SUBAGENT=no')
" 2>/dev/null || echo 'EVENT="" COMMAND="" IS_SUBAGENT=no')"

# Subagent workers bypass commit gate — they run flowctl done themselves
if [ "$IS_SUBAGENT" = "yes" ]; then
    exit 0
fi

# State file keyed by .flow directory (absolute path hash)
FLOW_DIR="$(cd .flow && pwd)"
STATE_FILE="/tmp/flow-commit-gate-$(echo "$FLOW_DIR" | md5 -q 2>/dev/null || echo "$FLOW_DIR" | md5sum | cut -d' ' -f1)"

# --- PostToolUse: track guard pass ---
if [ "$EVENT" = "PostToolUse" ]; then
    if echo "$COMMAND" | grep -qE '(flowctl|flowctl\.py)\s+guard'; then
        GUARD_PASSED=$(echo "$INPUT" | python3 -c "
import json, sys
data = json.load(sys.stdin)
resp = data.get('tool_response', {})
text = resp.get('stdout', str(resp)) if isinstance(resp, dict) else str(resp)
if 'guards passed' in text.lower() and 'failed' not in text.lower():
    print('yes')
elif 'nothing to run' in text.lower() or 'no stack detected' in text.lower():
    print('yes')
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
    exit 0
fi

IN_PROGRESS=$(python3 "$FLOWCTL" tasks --json 2>/dev/null | python3 -c "
import json, sys
data = json.load(sys.stdin)
tasks = data.get('tasks', [])
active = [t for t in tasks if t.get('status') == 'in_progress']
print(len(active))
" 2>/dev/null || echo "0")

# No task in_progress → manual commit, allow
if [ "$IN_PROGRESS" = "0" ]; then
    exit 0
fi

# Task in_progress → check guard evidence
if [ -f "$STATE_FILE" ]; then
    GUARD_TIME=$(cat "$STATE_FILE")
    NOW=$(date +%s)
    AGE=$(( NOW - GUARD_TIME ))
    if [ "$AGE" -lt 600 ]; then
        rm -f "$STATE_FILE"
        exit 0
    fi
fi

# Block: task in_progress but no recent guard pass
echo "BLOCKED: git commit requires passing guard first." >&2
echo "A task is in_progress but flowctl guard has not been run (or passed) recently." >&2
echo "Run: flowctl guard" >&2
exit 2
