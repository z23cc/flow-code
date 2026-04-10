---
name: flow-code:brainstorm
description: "Explore and pressure-test an idea before committing to a plan"
argument-hint: "[--auto] <idea or problem>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-brainstorm`

The ONLY purpose of this command is to call the `flow-code-brainstorm` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the problem is still fuzzy and you want to explore options before committing. Use `/flow-code:plan` for planning-only, `/flow-code:spec` for artifact-first requirements capture, and `/flow-code:go` when you want execution to continue after planning.

Pass the user request to the skill. The skill handles all brainstorm logic.

Modes:
- Default: interactive (asks user questions)
- `--auto`: AI self-interview (analyzes code, self-answers, zero human input)
