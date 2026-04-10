---
name: flow-code:adr
description: "Capture a durable architecture decision with alternatives and consequences"
argument-hint: "[--status proposed|accepted|superseded|deprecated] <decision, change, or ADR path>"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-adr`

The ONLY purpose of this command is to call the `flow-code-adr` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the main artifact is an architecture decision record. Use `/flow-code:spec` for general requirements capture, `/flow-code:plan` for planning-only task breakdown, and `flow-code-deprecation` when the main question is how to replace or remove an old surface.

Pass the user request to the skill. The skill handles all ADR workflow logic.
