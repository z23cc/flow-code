---
name: flow-code:ralph-init
description: "Scaffold the repo-local Ralph autonomous harness"
argument-hint: ""
---

# IMPORTANT: This command MUST invoke the skill `flow-code-ralph-init`

The ONLY purpose of this command is to call the `flow-code-ralph-init` skill. You MUST use that skill now.

Choose this front door when you want to scaffold the repo-local Ralph autonomous harness. Use `/flow-code:go` for the normal execution pipeline and `/flow-code:setup` for optional local tool/bootstrap setup.

Creates `scripts/ralph/` in the current repo.
