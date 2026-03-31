#!/usr/bin/env bash
# PreCompact hook: snapshot .flow/ state before context compression
# Ensures critical task state survives compaction

set -euo pipefail

FLOWCTL="${CLAUDE_PLUGIN_ROOT:-${DROID_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}}/scripts/flowctl"

[ -d ".flow" ] || exit 0

if command -v python3 &>/dev/null && [ -f "$FLOWCTL" ]; then
    # Output current state summary so it gets included in compact summary
    echo "=== Flow-Code State Snapshot (pre-compact) ==="
    "$FLOWCTL" status --json 2>/dev/null || true
    echo "=== End Snapshot ==="
fi
