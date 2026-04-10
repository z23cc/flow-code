---
name: flow-code:queue
description: "Show multi-epic queue status with dependency visualization"
argument-hint: "[--json]"
---

# Multi-Epic Queue Status

This command is a direct `flowctl queue` wrapper rather than a skill-dispatch wrapper.

Choose this front door when you want a portfolio-level view of all epics, dependencies, and blockers rather than working on a single execution path. Use `/flow-code:go` or `/flow-code:work` for one epic, and `/flow-code:loop-status` for long-running autonomous loop status.

Run `flowctl queue` to show the status of all epics and their tasks, including dependency relationships, ready/blocked counts, and progress bars.

```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
$FLOWCTL queue
```

**User input:** $ARGUMENTS

If user passes `--json`, add `--json` flag to the command.

Present the output to the user. If there are blocked epics, explain what's blocking them.
