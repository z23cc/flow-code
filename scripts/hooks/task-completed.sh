#!/usr/bin/env bash
# TaskCompleted hook: sync Claude task completion with .flow/ state
# Bridges Claude's built-in task system with flow-code tracking

set -euo pipefail

FLOWCTL="${CLAUDE_PLUGIN_ROOT:-${DROID_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}}/scripts/flowctl"

[ -d ".flow" ] || exit 0

# Log task completion event for observability
if command -v python3 &>/dev/null && [ -f "$FLOWCTL" ]; then
    echo "Claude task completed - flow-code state check:"
    "$FLOWCTL" tasks --status in_progress --json 2>/dev/null || echo "[]"
fi
