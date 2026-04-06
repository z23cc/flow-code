---
name: flow-code-plan
description: "Use when planning features or designing implementation. Triggers on /flow-code:plan with text descriptions or Flow IDs."
user-invocable: false
---

# Flow plan

Turn a rough idea into an epic with tasks in `.flow/`. This skill does not write code.

Follow this skill and linked workflows exactly. Deviations cause drift, bad gates, retries, and user frustration.

**IMPORTANT**: This plugin uses `.flow/` for ALL task tracking. Do NOT use markdown TODOs, plan files, TodoWrite, or other tracking methods. All task state must be read and written via `flowctl`.

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
$FLOWCTL <command>
```

## Pre-check: Local setup version

If `.flow/meta.json` exists and has `setup_version`, compare to plugin version:
```bash
SETUP_VER=$(jq -r '.setup_version // empty' .flow/meta.json 2>/dev/null)
# Portable: Claude Code uses .claude-plugin, Factory Droid uses .factory-plugin
PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.claude-plugin/plugin.json"
[[ -f "$PLUGIN_JSON" ]] || PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.factory-plugin/plugin.json"
PLUGIN_VER=$(jq -r '.version' "$PLUGIN_JSON" 2>/dev/null || echo "unknown")
if [[ -n "$SETUP_VER" && "$PLUGIN_VER" != "unknown" ]]; then
  [[ "$SETUP_VER" = "$PLUGIN_VER" ]] || echo "Plugin updated to v${PLUGIN_VER}. Run /flow-code:setup to refresh local scripts (current: v${SETUP_VER})."
fi
```
Continue regardless (non-blocking).

**Role**: product-minded planner with strong repo awareness.
**Goal**: produce an epic with tasks that match existing conventions and reuse points.
**Task size**: every task must fit one `/flow-code:work` iteration (~100k tokens max). If it won't, split it.

## The Golden Rule: No Implementation Code

**Plans are specs, not implementations.** Do NOT write the code that will be implemented.

### Code IS allowed:
- **Signatures/interfaces** (what, not how): `function validate(input: string): Result`
- **Patterns from this repo** (with file:line ref): "Follow pattern at `src/auth.ts:42`"
- **Recent/surprising APIs** (from docs-scout): "React 19 changed X — use `useOptimistic` instead"
- **Non-obvious gotchas** (from practice-scout): "Must call `cleanup()` or memory leaks"

### Code is FORBIDDEN:
- Complete function implementations
- Full class/module bodies
- "Here's what you'll write" blocks
- Copy-paste ready snippets (>10 lines)

**Why:** Implementation happens in `/flow-code:work` with fresh context. Writing it here wastes tokens in planning, review, AND implementation — then causes drift when the implementer does it differently anyway.

## Input

Full request: $ARGUMENTS

Accepts:
- Feature/bug description in natural language
- Flow epic ID `fn-N-slug` (e.g., `fn-1-add-oauth`) or legacy `fn-N`/`fn-N-xxx` to refine existing epic
- Flow task ID `fn-N-slug.M` (e.g., `fn-1-add-oauth.2`) or legacy `fn-N.M`/`fn-N-xxx.M` to refine specific task
- Chained instructions like "then review with /flow-code:plan-review"

Examples:
- `/flow-code:plan Add OAuth login for users`
- `/flow-code:plan fn-1-add-oauth`
- `/flow-code:plan fn-1` (legacy formats fn-1, fn-1-xxx still supported)
- `/flow-code:plan fn-1-add-oauth then review via /flow-code:plan-review`

If empty, ask: "What should I plan? Give me the feature or bug in 1-5 sentences."

## Context Analysis (replaces setup questions)

Analyze the request and context — no questions asked:
```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

Based on the request text, decide:
- **Research**: always `repo-scout`. For deep context, detect RP availability:
  - MCP tools available (context_builder in tool list) → `rp(mcp)`
  - `which rp-cli` succeeds → `rp(cli)`
  - Neither → `rp(scout-fallback)` (uses context-scout subagent)
- **Depth**: clear and scoped request → `short`. needs design decisions → `standard`. architecture change → `deep`.
- **Review** (auto, layer-aware):
  - Check `$REVIEW_BACKEND`:
    - Returns `rp` → verify `which rp-cli` succeeds. If available → use RP. If NOT available → **degrade to codex** (RP is macOS-only). If codex also unavailable → skip.
    - Returns `codex` → use Codex for plan review
    - Returns `none` → skip plan review
    - Returns `ASK` → auto-detect available tools:
      - `which rp-cli` succeeds → use RP
      - else `which codex` succeeds → use Codex
      - else → skip review

Output one line:
```
Research: repo-scout + rp(<mcp|cli|scout-fallback>) | Depth: <short|standard|deep> | Review: <rp|codex|none> (auto-detected)
```

### Explicit flag overrides

These flags override the corresponding AI decision without entering the analysis flow:
- `--research=rp|grep`, `--depth=short|standard|deep`, `--review=rp|codex|export|none`, `--plan-only`, `--no-capability-scan` (skip capability-scout in Step 1)
- `--interactive` — **opt-in** interview refinement. Before Context Analysis, invoke `/flow-code:interview` with the raw request text. The interview returns refined-spec markdown (Problem / Scope / Acceptance / Open Questions). Use that refined text as the effective request for Context Analysis and Step 1. When this flag is NOT passed, the plan flow is unchanged and the zero-interaction default (CLAUDE.md:99) is preserved. There is intentionally no auto-trigger heuristic and no `--no-interview` flag — interview is opt-in only.

Proceed to Step 1 immediately.

## Workflow

Read [steps.md](steps.md) and follow each step in order.

**CRITICAL — Step 1 (Research)**: You MUST launch ALL scouts listed in steps.md in ONE parallel Task call. Do NOT skip scouts or run them sequentially. Each scout provides unique signal.

If review was decided (rp/codex/export):
- rp or codex: run `/flow-code:plan-review` after Step 4, fix issues until it passes
- export: run `/flow-code:plan-review` with export mode after Step 4

## Output

All plans go into `.flow/`:
- Epic: `.flow/epics/fn-N-slug.json` + `.flow/specs/fn-N-slug.md`
- Tasks: `.flow/tasks/fn-N-slug.M.json` + `.flow/tasks/fn-N-slug.M.md`

**Never write plan files outside `.flow/`. Never use TodoWrite for task tracking.**

## Output rules

- Only create/update epics and tasks via flowctl
- No code changes
- No plan files outside `.flow/`

## Auto-Execute

**Steps.md Step 8 handles auto-execution.** After steps complete:
- Default: `/flow-code:work <epic-id> --no-review` invoked automatically (Step 8)
- `--plan-only`: shows plan summary and stops (Step 8)

**After work completes** (if auto-executed):
- All tasks done → Layer 3 adversarial review runs automatically (Phase 3j)
- Then auto push + draft PR (Phase 5)

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "This is obvious, skip scouts" | Scouts find patterns and reuse points you don't know about |
| "Only one task, no DAG needed" | Even single tasks need dependency analysis and acceptance criteria |
| "Requirements are clear, skip interview" | Your understanding ≠ user's intent. A 2-minute check prevents 2-hour rework |
| "This is too small to plan" | Small plans still need acceptance criteria. A 3-line spec is fine |
| "I'll refine the plan during implementation" | Implementation drift is the #1 cause of rework. Plan now |

## Red Flags

- Any task estimated >L size without being split
- Scout returns "Cannot analyze" but planning continues without investigation
- Task title contains "and" (likely two tasks combined)
- No acceptance criteria on any task
- Plan has zero file references from repo-scout
- 7+ tasks in a single epic (possible over-splitting)

## Verification

- [ ] Every task has ≥1 testable acceptance criterion
- [ ] Every task is ≤M size (L tasks split)
- [ ] File refs from repo-scout included in task specs
- [ ] DAG validated (`flowctl validate`)
- [ ] Quick commands section exists in epic spec
- [ ] No task title contains "and" without justification
