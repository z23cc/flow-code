---
name: flow-code:django
description: "Apply Django-specific architecture, ORM, security, testing, and verification guidance"
argument-hint: "[architecture|orm|drf|security|testing|verification]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-django`

The ONLY purpose of this command is to call the `flow-code-django` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the request is specifically about Django, DRF, ORM, security, testing, or verification patterns. Use `/flow-code:go` for a full feature pipeline and `/flow-code:plan` when you want planning-only behavior.

Pass the user request to the skill. The skill handles all Django-related logic.
