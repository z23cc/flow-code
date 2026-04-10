---
name: flow-code:simplify
description: "Reduce code complexity while preserving behavior"
argument-hint: "[file or directory to simplify]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-simplify`

The ONLY purpose of this command is to call the `flow-code-simplify` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the job is to reduce complexity while preserving current behavior. Use `/flow-code:spec` or `/flow-code:plan` when the requirements are changing, and `/flow-code:go` when you want the broader execution pipeline.

Pass the user request to the skill. The skill handles all code simplification logic.
