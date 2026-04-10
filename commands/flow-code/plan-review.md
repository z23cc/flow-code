---
name: flow-code:plan-review
description: "Run a formal review pass on an epic plan or spec"
argument-hint: "<epic ID> [--review <backend>]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-plan-review`

The ONLY purpose of this command is to call the `flow-code-plan-review` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the plan already exists and you want the formal review gate before execution starts. Use `/flow-code:autoplan` for multi-perspective critique, `/flow-code:plan` to create or revise the plan, and `/flow-code:go` when you want the full pipeline to keep moving.

Pass the user request to the skill. The skill handles all plan review logic.
