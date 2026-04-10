---
name: flow-code:impl-review
description: "Review implementation changes on the current branch or task scope"
argument-hint: "[task ID] [--base <commit>] [--review <backend>]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-impl-review`

The ONLY purpose of this command is to call the `flow-code-impl-review` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when code has already changed and you want the formal implementation review gate. Use `/flow-code:work` to make the changes first, `/flow-code:epic-review` when the whole epic is done, and `/flow-code:qa` or `/flow-code:design-review` for browser or UX-specific audits.

Pass the user request to the skill. The skill handles all implementation review logic.
