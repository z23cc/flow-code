#!/usr/bin/env bash
# commit-gate.sh — PreToolUse hook that blocks git commit when flow task
# is in_progress but guard has not been run.
#
# Stdin: JSON with hook_event_name, tool_name, tool_input.command
# Exit 0 = allow, Exit 2 = block
#
# Also registered as PostToolUse to track guard pass state.

# No strict mode — hook must never crash (non-zero exit = error in Claude Code)
set +e

# No .flow directory → not a flow-code project, allow
[ -d ".flow" ] || exit 0

# Read stdin JSON once
INPUT=$(cat 2>/dev/null || echo '{}')

# Parse all fields safely in one python call
PARSED=$(echo "$INPUT" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    event = data.get('hook_event_name', '')
    cmd = data.get('tool_input', {}).get('command', '') if isinstance(data.get('tool_input'), dict) else ''
    session = data.get('session_id', '')
    is_sub = 'yes' if '@' in session else 'no'
    # Escape for safe shell consumption
    cmd_safe = cmd.replace('\\\\', '\\\\\\\\').replace(\"'\", \"'\\\\''\")
    print(f\"{event}|{is_sub}|{cmd_safe}\")
except Exception:
    print('||')
" 2>/dev/null || echo '||')

EVENT=$(echo "$PARSED" | cut -d'|' -f1)
IS_SUBAGENT=$(echo "$PARSED" | cut -d'|' -f2)
COMMAND=$(echo "$PARSED" | cut -d'|' -f3-)

# Subagent workers bypass — they manage their own flow
[ "$IS_SUBAGENT" = "yes" ] && exit 0

# --- PostToolUse: track guard pass ---
if [ "$EVENT" = "PostToolUse" ]; then
    # Only care about flowctl guard commands
    case "$COMMAND" in
        *flowctl*guard*)
            GUARD_OK=$(echo "$INPUT" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    resp = data.get('tool_response', {})
    text = resp.get('stdout', str(resp)) if isinstance(resp, dict) else str(resp)
    text_lower = text.lower()
    if 'guards passed' in text_lower and 'failed' not in text_lower:
        print('yes')
    elif 'nothing to run' in text_lower or 'no stack detected' in text_lower:
        print('yes')
    else:
        print('no')
except Exception:
    print('no')
" 2>/dev/null || echo "no")
            if [ "$GUARD_OK" = "yes" ]; then
                FLOW_DIR=$(cd .flow 2>/dev/null && pwd) || exit 0
                STATE_FILE="/tmp/flow-commit-gate-$(echo "$FLOW_DIR" | md5 -q 2>/dev/null || echo "$FLOW_DIR" | md5sum 2>/dev/null | cut -d' ' -f1 || echo "default")"
                echo "$(date +%s)" > "$STATE_FILE" 2>/dev/null
            fi
            ;;
    esac
    exit 0
fi

# --- PreToolUse: gate git commit ---
[ "$EVENT" = "PreToolUse" ] || exit 0

# Only care about git commit
case "$COMMAND" in
    *git\ commit*|*git\ \ commit*) ;;  # match "git commit"
    *) exit 0 ;;
esac

# Check: is any task in_progress?
PLUGIN_DIR="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-}}"
[ -n "$PLUGIN_DIR" ] || exit 0
FLOWCTL="$PLUGIN_DIR/bin/flowctl"

IN_PROGRESS=$("$FLOWCTL" tasks --json 2>/dev/null | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
    active = [t for t in data.get('tasks', []) if t.get('status') == 'in_progress']
    print(len(active))
except Exception:
    print('0')
" 2>/dev/null || echo "0")

# No task in_progress → allow
[ "$IN_PROGRESS" = "0" ] && exit 0

# Check guard evidence
FLOW_DIR=$(cd .flow 2>/dev/null && pwd) || exit 0
STATE_FILE="/tmp/flow-commit-gate-$(echo "$FLOW_DIR" | md5 -q 2>/dev/null || echo "$FLOW_DIR" | md5sum 2>/dev/null | cut -d' ' -f1 || echo "default")"

if [ -f "$STATE_FILE" ]; then
    GUARD_TIME=$(cat "$STATE_FILE" 2>/dev/null || echo "0")
    NOW=$(date +%s)
    AGE=$(( NOW - GUARD_TIME ))
    if [ "$AGE" -lt 600 ]; then
        rm -f "$STATE_FILE" 2>/dev/null
        exit 0
    fi
fi

# Block
echo "BLOCKED: git commit requires passing guard first." >&2
echo "Run: flowctl guard" >&2
exit 2
