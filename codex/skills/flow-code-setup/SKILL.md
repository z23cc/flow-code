---
name: flow-code-setup
description: "Use when user runs /flow-code:setup or asks to install flow-code locally."
user-invocable: false
---

# Flow-Code Setup (Optional)

Install flowctl locally and add instructions to project docs. **Fully optional** - flow-code works without this via the plugin.

## Benefits

- `flowctl` accessible from command line (add `.flow/bin` to PATH)
- Other AI agents (Codex, Cursor, etc.) can read instructions from AGENTS.md/AGENTS.md
- Works without Claude Code plugin installed

## Workflow

Read [workflow.md](workflow.md) and follow each step in order.

## Notes

- **Fully optional** - standard plugin usage works without local setup
- Copies scripts (not symlinks) for portability across environments
- Safe to re-run - will detect existing setup and offer to update
