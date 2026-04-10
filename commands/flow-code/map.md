---
name: flow-code:map
description: "Generate or update a codebase architecture map"
argument-hint: "[path] [--update]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-map`

The ONLY purpose of this command is to call the `flow-code-map` skill. You MUST use that skill now.

**User input:** $ARGUMENTS

Choose this front door when you want an architecture map of the existing codebase rather than planning or implementation. Use `/flow-code:plan` to create an execution plan and `/flow-code:prime` to assess readiness or remediation opportunities.

Pass the user input to the skill. The skill handles scanning, subagent dispatch, and map generation.
