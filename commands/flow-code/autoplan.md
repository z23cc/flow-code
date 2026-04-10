---
name: flow-code:autoplan
description: "Review an existing epic or plan from multiple perspectives"
argument-hint: "[epic-id]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-autoplan`

The ONLY purpose of this command is to call the `flow-code-autoplan` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when an existing epic or plan needs multi-perspective critique before execution. Use `/flow-code:plan` to create the plan in the first place and `/flow-code:go` when you want the pipeline to execute instead of only reviewing.

Pass the user request to the skill. The skill handles all review logic.
