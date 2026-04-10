---
name: flow-code:run
description: "Backward-compatible alias for /flow-code:go"
argument-hint: "[same args as /flow-code:go]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-run`

This command exists for backward compatibility. The normal user-facing entry point is `/flow-code:go`.

Choose this front door only for backward compatibility. Prefer `/flow-code:go` for the normal full execution/resume path.

**User request:** $ARGUMENTS

Pass the user request to the skill. The skill handles the full pipeline logic.
