---
name: flow-code:spec
description: "Draft or refine a planning-ready requirements spec"
argument-hint: "[--output <path>] <idea, change, refactor, or markdown file>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-spec`

The ONLY purpose of this command is to call the `flow-code-spec` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when you want a durable requirements artifact before planning or execution. Use `/flow-code:brainstorm` for open-ended ideation, `/flow-code:adr` for architecture decisions, and `/flow-code:plan` when you are ready for task breakdown.

Pass the user request to the skill. The skill handles all spec-drafting logic.
