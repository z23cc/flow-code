---
name: flow-code:qa
description: "Run browser-driven QA on an existing UI or flow"
argument-hint: "[url] [--fix] [--viewport <size>]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-qa`

The ONLY purpose of this command is to call the `flow-code-qa` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when you want browser-driven QA of an existing UI or flow. Use `/flow-code:design-review` for visual/UX critique and `/flow-code:frontend-ui` when the main task is building or revising the interface itself.

Pass the user request to the skill. The skill handles all QA testing logic.
