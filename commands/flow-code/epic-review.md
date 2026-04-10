---
name: flow-code:epic-review
description: "Verify a completed epic against its spec before close"
argument-hint: "<epic ID> [--review <backend>] [--skip-gap-check]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-epic-review`

The ONLY purpose of this command is to call the `flow-code-epic-review` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when all tasks in an epic are done and you want the final spec-compliance review before closing. Use `/flow-code:impl-review` for code-level review on a branch or task, and `/flow-code:plan-review` earlier in the lifecycle before execution begins.

Pass the user request to the skill. The skill handles all epic completion review logic.
