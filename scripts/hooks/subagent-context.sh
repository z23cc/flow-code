#!/usr/bin/env bash
# SubagentStart hook: inject current epic/task context for worker agents
# Only active during flow-code work sessions

set -euo pipefail

PLUGIN_DIR="${CLAUDE_PLUGIN_ROOT:-${DROID_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}}"
FLOWCTL="$PLUGIN_DIR/bin/flowctl"

# Only inject if .flow/ exists and there's an active epic
[ -d ".flow" ] || exit 0

# Get current in-progress task info for context injection
if [ -f "$FLOWCTL" ] || [ -x "$FLOWCTL" ]; then
    ACTIVE=$("$FLOWCTL" tasks --status in_progress --json 2>/dev/null || echo "[]")
    if [ "$ACTIVE" != "[]" ] && [ -n "$ACTIVE" ]; then
        echo "Active flow-code tasks: $ACTIVE"
    fi
fi
