---
name: flow-code:go
description: "Full autopilot: brainstorm, plan, work, review, close. Zero human input from idea to PR."
argument-hint: "<idea or problem description>"
---

# IMPORTANT: This command MUST invoke the skill flow-code-run

The ONLY purpose of this command is to call the flow-code-run skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Pass the user request to the skill with GO_MODE=true. The skill handles all pipeline logic including the brainstorm phase.
