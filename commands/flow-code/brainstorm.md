---
name: flow-code:brainstorm
description: Explore and pressure-test an idea before planning. Use --auto for AI self-interview.
argument-hint: "[--auto] <idea or problem>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-brainstorm`

The ONLY purpose of this command is to call the `flow-code-brainstorm` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Pass the user request to the skill. The skill handles all brainstorm logic.

Modes:
- Default: interactive (asks user questions)
- `--auto`: AI self-interview (analyzes code, self-answers, zero human input)
