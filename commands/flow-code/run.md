---
name: flow-code:run
description: "Internal pipeline entry point. Use /flow-code:go instead."
---

# This command redirects to /flow-code:go

`/flow-code:run` is now internal. The user-facing entry point is `/flow-code:go`.

Tell the user: "Use `/flow-code:go` instead. It runs the full pipeline: brainstorm, plan, work, review, close."

If the user provided arguments, invoke the flow-code-run skill directly:

**User request:** $ARGUMENTS

Pass the user request to the skill. The skill handles all pipeline logic.
