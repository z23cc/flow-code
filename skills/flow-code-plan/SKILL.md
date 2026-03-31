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
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl.py"
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

## FIRST: Parse Options or Ask Questions

Check configured backend:
```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```
Returns: `ASK` (not configured), or `rp`/`codex`/`none` (configured).

### Option Parsing (skip questions if found in arguments)

Parse the arguments for these patterns. If found, use them and skip questions:

**Research approach**:
- `--research=rp` or `--research rp` or "use rp" or "context-scout" or "use repoprompt" → context-scout (errors at runtime if rp-cli missing)
- `--research=grep` or `--research grep` or "use grep" or "repo-scout" or "fast" → repo-scout

**Review mode**:
- `--review=codex` or "review with codex" or "codex review" or "use codex" → Codex CLI (GPT 5.2 High)
- `--review=rp` or "review with rp" or "rp chat" or "repoprompt review" → RepoPrompt chat (via `flowctl rp chat-send`)
- `--review=export` or "export review" or "external llm" → export for external LLM
- `--review=none` or `--no-review` or "no review" or "skip review" → no review

**Execution mode** (default: execute):
- `--plan-only` or "just plan" or "don't execute" or "only plan" → create plan but do NOT auto-execute work
- Default (no flag): after plan is created, automatically invoke `/flow-code:work` to execute

### If options NOT found in arguments

**Plan depth** (parse from args or ask):
- `--depth=short` or "quick" or "minimal" → SHORT
- `--depth=standard` or "normal" → STANDARD
- `--depth=deep` or "comprehensive" or "detailed" → DEEP
- Default: SHORT (simpler is better)

**If REVIEW_BACKEND is rp, codex, or none** (already configured): Only ask research question. Show override hint:

```
Quick setup: Use RepoPrompt for deeper context?
a) Yes, context-scout (slower, thorough)
b) No, repo-scout (faster)

(Reply: "a", "b", or just tell me)
(Tip: --depth=short|standard|deep, --review=rp|codex|none)
```

**If REVIEW_BACKEND is ASK** (not configured): Ask all questions (do NOT use AskUserQuestion tool):

```
Quick setup before planning:

1. **Plan depth** — How detailed?
   a) Short — problem, acceptance, key context only
   b) Standard (default) — + approach, risks, test notes
   c) Deep — + phases, alternatives, rollout plan

2. **Research** — Use RepoPrompt for deeper context?
   a) Yes, context-scout (slower, thorough)
   b) No, repo-scout (faster)

3. **Review** — Run Carmack-level review after?
   a) Codex CLI
   b) RepoPrompt
   c) Export for external LLM
   d) None (configure later)

(Reply: "1a 2b 3d", or just tell me naturally)
```

Wait for response. Parse naturally — user may reply terse ("1a 2b") or ramble via voice.

**Defaults when empty/ambiguous:**
- Depth = `standard` (balanced detail)
- Research = `grep` (repo-scout)
- Review = configured backend if set, else `none`

## Workflow

Read [steps.md](steps.md) and follow each step in order.

**CRITICAL — Step 1 (Research)**: You MUST launch ALL scouts listed in steps.md in ONE parallel Task call. Do NOT skip scouts or run them sequentially. Each scout provides unique signal.

If user chose review:
- Option 2a: run `/flow-code:plan-review` after Step 4, fix issues until it passes
- Option 2b: run `/flow-code:plan-review` with export mode after Step 4

## Output

All plans go into `.flow/`:
- Epic: `.flow/epics/fn-N-slug.json` + `.flow/specs/fn-N-slug.md`
- Tasks: `.flow/tasks/fn-N-slug.M.json` + `.flow/tasks/fn-N-slug.M.md`

**Never write plan files outside `.flow/`. Never use TodoWrite for task tracking.**

## Output rules

- Only create/update epics and tasks via flowctl
- No code changes
- No plan files outside `.flow/`

## Auto-Execute (default behavior)

**After plan is created, automatically execute it — unless `--plan-only` was specified.**

```bash
# Check task count
TASK_COUNT=$($FLOWCTL tasks --epic <epic-id> --json | python3 -c "import json,sys; print(json.load(sys.stdin)['count'])")
```

**Scale-adaptive execution:**

- **≤ 10 tasks**: Invoke `/flow-code:work <epic-id>` directly in this session. Workers run as subagents with fresh context per task (no context overflow — main session only orchestrates).

- **> 10 tasks**: Print recommendation instead of auto-executing:
  ```
  Epic has N tasks — recommend using Ralph for fresh context per task:
    /flow-code:ralph-init

  Or execute in this session (may be slower for large epics):
    /flow-code:work <epic-id>
  ```

**If `--plan-only`**: Skip auto-execute, print:
```
Plan created: <epic-id> (N tasks)
Next: /flow-code:work <epic-id>
```

**After work completes** (if auto-executed):
- If all tasks done → suggest: `/flow-code:epic-review <epic-id>` or auto-invoke if review backend configured
- Print epic close command: `flowctl epic close <epic-id>`
