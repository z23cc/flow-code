---
name: flow-code:design-review
description: "Audit visual design and UX with browser automation"
argument-hint: "[url] [--pages <paths>] [--no-fix]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-design-review`

The ONLY purpose of this command is to call the `flow-code-design-review` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when you want a visual/UX audit of an existing site or app. Use `/flow-code:frontend-ui` to build or revise UI and `/flow-code:qa` when the primary goal is behavioral/browser testing.

Pass the user request to the skill. The skill handles all design review logic.
