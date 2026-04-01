#!/usr/bin/env bash
# PreCompact hook: inject critical .flow/ state into compaction context
#
# When Claude Code compresses conversation history, this hook's stdout
# is included in the compressed context. This ensures the model retains
# awareness of task state, file locks, and current work after compaction.
#
# Output is designed to be concise but information-dense — every line
# survives compaction and costs tokens in every subsequent turn.

set -uo pipefail

[ -d ".flow" ] || exit 0

FLOWCTL="${CLAUDE_PLUGIN_ROOT:-${DROID_PLUGIN_ROOT:-$(cd "$(dirname "$(dirname "$0")")" && pwd)}}/scripts/flowctl.py"
[ -f "$FLOWCTL" ] || exit 0
command -v python3 &>/dev/null || exit 0

# Collect state efficiently — single python script to minimize subprocess overhead
python3 - "$FLOWCTL" <<'PYEOF'
import subprocess, json, sys

FLOWCTL = sys.argv[1]

def run(args):
    try:
        r = subprocess.run(
            ["python3", FLOWCTL] + args,
            capture_output=True, text=True, timeout=5
        )
        if r.returncode == 0 and r.stdout.strip():
            return json.loads(r.stdout)
    except Exception:
        pass
    return None

lines = []

# 1. Active epics and their progress
epics = run(["epics", "--json"])
if epics and epics.get("epics"):
    for e in epics["epics"]:
        eid = e["id"]
        status = e.get("status", "open")
        if status == "done":
            continue
        # Get task breakdown
        tasks = run(["tasks", "--epic", eid, "--json"])
        if tasks:
            counts = {}
            for t in tasks.get("tasks", []):
                s = t.get("status", "todo")
                counts[s] = counts.get(s, 0) + 1
            progress = " ".join(f"{s}={c}" for s, c in sorted(counts.items()))
            lines.append(f"Epic {eid}: {progress}")

            # Show in-progress tasks (most important to preserve)
            for t in tasks.get("tasks", []):
                if t.get("status") == "in_progress":
                    assignee = t.get("assignee", "")
                    files_str = ""
                    files = t.get("files", [])
                    if files:
                        files_str = f" files=[{','.join(files[:3])}]"
                    lines.append(f"  IN_PROGRESS: {t['id']} \"{t['title']}\"{files_str}")

# 2. Active file locks (Teams mode)
locks = run(["lock-check", "--json"])
if locks and locks.get("count", 0) > 0:
    lines.append(f"File locks ({locks['count']} active):")
    for f, info in sorted(locks.get("locks", {}).items()):
        lines.append(f"  {f} -> {info['task_id']}")

# 3. Ready tasks (what to work on next)
if epics and epics.get("epics"):
    for e in epics["epics"]:
        if e.get("status") == "done":
            continue
        ready = run(["ready", "--epic", e["id"], "--json"])
        if ready and ready.get("ready"):
            ids = [t["id"] for t in ready["ready"][:5]]
            lines.append(f"Ready: {', '.join(ids)}")

# Output — concise, one block
if lines:
    print("[flow-code state]")
    for line in lines:
        print(line)
    print("[/flow-code state]")
PYEOF
