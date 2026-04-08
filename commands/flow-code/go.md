---
name: flow-code:go
description: "Full autopilot: brainstorm, plan, work, review, close. Zero human input from idea to PR."
argument-hint: "<idea or problem description>"
---

# IMPORTANT: This command MUST invoke the skill flow-code-run

The ONLY purpose of this command is to call the flow-code-run skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Pass the user request to the skill. The skill auto-detects mode from input type:
- Natural language → full pipeline (brainstorm → plan → work → review → close)
- Flow ID (fn-N-*) → resume existing epic from current phase
- Spec file path → plan from spec (skip brainstorm)
- `--plan-only` → stop after planning
- `--quick` → fast path for trivial changes (skip brainstorm, plan review, impl review)
