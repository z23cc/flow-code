---
name: flow-code:work
description: "Execute planned tasks with re-anchoring, reviews, and continuous scheduling plus an integration checkpoint"
argument-hint: "<epic ID or task ID>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-work`

The ONLY purpose of this command is to call the `flow-code-work` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the plan already exists and you want task execution to proceed. Use `/flow-code:go` for the full execution/resume path, `/flow-code:plan` for planning-only, and `/flow-code:spec` when requirements should be written before planning.

Pass the user request to the skill. The skill handles all work execution logic.
