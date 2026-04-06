---
name: flow-code-export-context
description: "Use when you want to review code or plans with an external model (ChatGPT, Claude web, etc.). Triggers on /flow-code:export-context."
---

# Export Context Mode

Export flow-code context to a markdown file for external LLMs (ChatGPT Pro, Claude web, etc.).

## Input

Arguments: $ARGUMENTS — Format: `<type> <target> [focus areas]`

- `plan <epic-id>` — Export plan review context
- `impl` — Export implementation review context (current branch)

## Workflow

### Step 1: Gather Content

```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
OUTPUT_FILE="prompt-exports/$(date +%Y%m%d-%H%M%S)-export.md"
mkdir -p prompt-exports
```

**Plan:** `$FLOWCTL show <epic-id> --json`, `$FLOWCTL cat <epic-id>`, gather task specs.

**Impl:** `git branch --show-current`, `git log main..HEAD --oneline`, `git diff main..HEAD --stat`.

### Step 2: Export (three-tier fallback)

Build instructions from gathered context. Extract the real task from the request — strip meta-framing about exporting.

**Tier 1 — RP MCP** (if `mcp__RepoPrompt__context_builder` available):
```
context_builder(instructions="<task><extracted task></task>\n<context><flowctl content></context>", response_type="clarify")
prompt(op="export", path="<OUTPUT_FILE>", copy_preset="<plan|codeReview>")
```

**Tier 2 — rp-cli** (if `which rp-cli` succeeds):
```bash
WINDOW_ID=$(rp-cli -e 'windows' | head -1 | awk '{print $1}')
rp-cli -w "$WINDOW_ID" -e 'builder "<instructions>" --response-type clarify'
rp-cli -w "$WINDOW_ID" -e "prompt export \"$OUTPUT_FILE\" --copy-preset <plan|codeReview>"
```

**Tier 3 — Basic Markdown** (no RP available):
Write gathered content directly to `$OUTPUT_FILE` as structured markdown with sections for context, specs, changed files, and focus areas.

Preset mapping: plan -> `plan`, impl -> `codeReview`.

### Step 3: Report

Report `$OUTPUT_FILE` path, tier used, and export type. Instruct user to paste into their preferred external LLM.

## Note

Manual external review only. No Ralph support (no receipts, no status updates).
