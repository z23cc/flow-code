---
name: flow-code:queue
description: "Show multi-epic queue status with dependency visualization"
argument-hint: "[--json]"
---

# Multi-Epic Queue Status

Run `flowctl queue` to show the status of all epics and their tasks, including dependency relationships, ready/blocked counts, and progress bars.

```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl.sh"
$FLOWCTL queue
```

**User input:** $ARGUMENTS

If user passes `--json`, add `--json` flag to the command.

Present the output to the user. If there are blocked epics, explain what's blocking them.
