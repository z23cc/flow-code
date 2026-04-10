---
name: flow-code-export-context
description: "Use when you want to review code or plans with an external model (ChatGPT, Claude web, etc.). Triggers on /flow-code:export-context."
tier: 1
---

# Export Context Mode

Export flow-code context to a markdown file for external LLMs (ChatGPT Pro, Claude web, etc.).

Canonical RP/MCP orchestration guidance lives in `skills/_shared/rp-mcp-orchestration.md`.

## Input

Arguments: $ARGUMENTS — Format: `<type> <target> [focus areas]`

- `plan <epic-id>` — Export plan review context
- `impl` — Export implementation review context (current branch)

## Workflow

### Step 1: Gather minimum grounding

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
OUTPUT_FILE="prompt-exports/$(date +%Y%m%d-%H%M%S)-export.md"
mkdir -p prompt-exports
```

**Plan:** `$FLOWCTL show <epic-id> --json`, `$FLOWCTL cat <epic-id>`, gather task specs.

**Impl:** `git branch --show-current`, `git log main..HEAD --oneline`, `git diff main..HEAD --stat`.

### Step 2: Export (three-tier fallback)

Extract the real task from the request — strip meta-framing about exporting.
Use the shared orchestration guide's defaults:
- `context_builder(response_type="clarify")` is the normal RP/MCP path;
- `prompt export` is the handoff step;
- fast path is only for tiny, obviously local scope.

```bash
# Detect RP tier (pass --mcp-hint if mcp__RepoPrompt__context_builder is in your tool list)
RP_TIER=$($FLOWCTL rp tier)  # or: $FLOWCTL rp tier --mcp-hint
```

- **If RP_TIER is `mcp`**: Call `context_builder(instructions=..., response_type="clarify")`, then `prompt(op="export", path="<OUTPUT_FILE>", copy_preset="<plan|codeReview>")`
- **If RP_TIER is `cli`**: Use `rp-cli` builder/export only when MCP tools are unavailable or the user explicitly asked for CLI usage
- **If RP_TIER is `none`**: Write gathered content directly to `$OUTPUT_FILE` as structured markdown.

Preset mapping: plan -> `plan`, impl -> `codeReview`.

**Review export rule:** if exporting review context, explicitly include the compare scope and the phrase `code review` in the builder instructions.

**Builder export rule:** after a builder-driven export, trust the generated selection/prompt unless you noticed a concrete issue.

### Step 3: Report

Report `$OUTPUT_FILE` path, tier used, and export type. Instruct user to paste into their preferred external LLM.

## Note

Manual external review only. No Ralph support (no receipts, no status updates).
