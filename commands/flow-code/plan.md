---
name: flow-code:plan
description: "Research the codebase and create an epic/task breakdown without executing work"
argument-hint: "<description, spec file path, or epic ID>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-plan`

The ONLY purpose of this command is to call the `flow-code-plan` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when execution is not wanted yet. Use `/flow-code:go` for the full execution/resume path, `/flow-code:brainstorm` for open-ended exploration first, and `/flow-code:spec` when you want a durable requirements artifact before planning.

Pass the user request to the skill. The skill handles all planning logic.
