---
name: flow-code:go
description: "Run the full execution pipeline or resume existing work"
argument-hint: "<idea or problem description>"
---

# IMPORTANT: This command MUST invoke the skill flow-code-run

The ONLY purpose of this command is to call the flow-code-run skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when the user wants flow-code to execute or resume work. Use `/flow-code:plan` for planning-only, `/flow-code:brainstorm` for open-ended exploration, `/flow-code:spec` for requirements capture, and `/flow-code:adr` for architecture decision records.

Pass the user request to the skill. The skill auto-detects mode from input type:
- Natural language → full execution pipeline (brainstorm → plan → work → review → close)
- Flow ID (fn-N-*) → resume existing epic from current phase
- Spec file path → continue from spec into planning/work (skip brainstorm)
- `--plan-only` → stop after planning (prefer `/flow-code:plan` when execution is not wanted yet)
- `--quick` → fast path for trivial changes (skip brainstorm, plan review, impl review)
- `--interactive` → pause at key decisions for user confirmation
