---
name: flow-code:plan-review
description: Carmack-level plan review via RepoPrompt or Codex
argument-hint: "<fn-N> [--review=rp|codex|export] [focus areas]"
---

# IMPORTANT: This command MUST invoke the skill flow-code-run

The ONLY purpose of this command is to call the flow-code-run skill. You MUST use that skill now.

**User request:** $ARGUMENTS

Pass the user request to the skill. The skill handles all pipeline logic.
