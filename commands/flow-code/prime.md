---
name: flow-code:prime
description: "Assess codebase readiness and propose improvements"
argument-hint: "[--report-only] [--fix-all] [path]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-prime`

The ONLY purpose of this command is to call the `flow-code-prime` skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Choose this front door when you want a readiness/quality assessment of the codebase and suggested remediation. Use `/flow-code:go` for feature execution and `/flow-code:auto-improve` when you want autonomous optimization loops instead of an assessment-first pass.

Pass the user request to the skill. The skill handles all assessment and remediation logic.
