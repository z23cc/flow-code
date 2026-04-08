---
name: flow-code-interview
description: "Use when user wants to flesh out a spec, refine requirements, or clarify a feature before building. Triggers on /flow-code:interview with Flow IDs or file paths."
tier: 3
user-invocable: false
---

# Flow interview

Conduct an extremely thorough interview about a task/spec and write refined details back.

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
PLUGIN_JSON="$HOME/.codex/plugin.json"

PLUGIN_VER=$(jq -r '.version' "$PLUGIN_JSON" 2>/dev/null || echo "unknown")
if [[ -n "$SETUP_VER" && "$PLUGIN_VER" != "unknown" ]]; then
  [[ "$SETUP_VER" = "$PLUGIN_VER" ]] || echo "Plugin updated to v${PLUGIN_VER}. Run /flow-code:setup to refresh local scripts (current: v${SETUP_VER})."
fi
```
Continue regardless (non-blocking).

**Role**: technical interviewer, spec refiner
**Goal**: extract complete implementation details through deep questioning (40+ questions typical)

## Input

Full request: $ARGUMENTS

Accepts:
- **Flow epic ID** `fn-N-slug` (e.g., `fn-1-add-oauth`) or legacy `fn-N`/`fn-N-xxx`: Fetch with `flowctl show`, write back with `flowctl epic set-plan`
- **Flow task ID** `fn-N-slug.M` (e.g., `fn-1-add-oauth.2`) or legacy `fn-N.M`/`fn-N-xxx.M`: Fetch with `flowctl show`, write back with `flowctl task spec/set-acceptance`
- **File path** (e.g., `docs/spec.md`): Read file, interview, rewrite file
- **Empty**: Prompt for target

Examples:
- `/flow-code:interview fn-1-add-oauth`
- `/flow-code:interview fn-1-add-oauth.3`
- `/flow-code:interview fn-1` (legacy formats fn-1, fn-1-xxx still supported)
- `/flow-code:interview docs/oauth-spec.md`

If empty, ask: "What should I interview you about? Give me a Flow ID (e.g., fn-1-add-oauth) or file path (e.g., docs/spec.md)"

## Setup

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Detect Input Type

1. **Flow epic ID pattern**: matches `fn-\d+(-[a-z0-9-]+)?` (e.g., fn-1-add-oauth, fn-12, fn-2-fix-login-bug)
   - Fetch: `$FLOWCTL show <id> --json`
   - Read spec: `$FLOWCTL cat <id>`

2. **Flow task ID pattern**: matches `fn-\d+(-[a-z0-9-]+)?\.\d+` (e.g., fn-1-add-oauth.3, fn-12.5)
   - Fetch: `$FLOWCTL show <id> --json`
   - Read spec: `$FLOWCTL cat <id>`
   - Also get epic context: `$FLOWCTL cat <epic-id>`

3. **File path**: anything with a path-like structure or `.md` extension AND the file exists on disk
   - Read file contents
   - If file doesn't exist, fall through to raw-text mode (below)

4. **Raw text (plan-integration mode)**: first arg does NOT match a Flow ID pattern AND is NOT an existing file path
   - Treat entire `$ARGUMENTS` as the raw request text to refine
   - Conduct a focused refinement interview (see **Plan-integration mode** below)
   - Used by `/flow-code:plan --interactive` to refine a vague request before planning

## Interview Process

**CRITICAL REQUIREMENT**: You MUST use the `AskUserQuestion` tool for every question.

- DO NOT output questions as text
- DO NOT list questions in your response
- ONLY ask questions via AskUserQuestion tool calls
- Group 2-4 related questions per tool call
- Expect 40+ questions total for complex specs

**Anti-pattern (WRONG)**:
```
Question 1: What database should we use?
Options: a) PostgreSQL b) SQLite c) MongoDB
```

**Correct pattern**: Call AskUserQuestion tool with question and options.

## Question Categories

Read [questions.md](questions.md) for all question categories and interview guidelines.

## Plan-integration mode (raw-text input)

When invoked with raw request text (input type 4 above) — typically from `/flow-code:plan --interactive`:

- **Hard cap: 12 questions max.** This mode is a focused pre-plan refinement, not the full 40+ question spec interview. Prioritize questions that disambiguate scope, uncover missing acceptance criteria, and surface edge cases.
- Group 2-4 related questions per `AskUserQuestion` call (same pattern as standard mode).
- **Do NOT write to `.flow/`.** Do not create an epic, do not call `flowctl epic create`.
- **Output contract**: emit refined-spec markdown to stdout with exactly these four sections, then return control to the caller:

  ```markdown
  ## Problem
  <concise problem statement distilled from raw input + answers>

  ## Scope
  <what's in / what's out, key decisions made during interview>

  ## Acceptance
  - [ ] <testable criterion 1>
  - [ ] <testable criterion 2>

  ## Open Questions
  <unresolved items for planning research, or "None" if fully clarified>
  ```

The caller (e.g. `/flow-code:plan`) uses this refined markdown as the effective request text for its own Context Analysis and scout research.

## NOT in scope (defer to /flow-code:plan)

- Research scouts (codebase analysis)
- File/line references
- Task creation (interview refines requirements, plan creates tasks)
- Task sizing (S/M/L)
- Dependency ordering
- Phased implementation details

## Write Refined Spec

After interview complete, write everything back — **scope depends on input type**.

### For NEW IDEA (text input, no Flow ID)

Create epic with interview output. **DO NOT create tasks** — that's `/flow-code:plan`'s job.

```bash
$FLOWCTL epic create --title "..." --json
$FLOWCTL epic plan <id> --file - --json <<'EOF'
# Epic Title

## Problem
Clear problem statement

## Key Decisions
Decisions made during interview (e.g., "Use OAuth not SAML", "Support mobile + web")

## Edge Cases
- Edge case 1
- Edge case 2

## Open Questions
Unresolved items that need research during planning

## Acceptance
- [ ] Criterion 1
- [ ] Criterion 2
EOF
```

Then suggest: "Run `/flow-code:plan fn-N` to research best practices and create tasks."

### For EXISTING EPIC (fn-N that already has tasks)

**First check if tasks exist:**
```bash
$FLOWCTL tasks --epic <id> --json
```

**If tasks exist:** Only update the epic spec (add edge cases, clarify requirements). **Do NOT touch task specs** — plan already created them.

**If no tasks:** Update epic spec, then suggest `/flow-code:plan`.

```bash
$FLOWCTL epic plan <id> --file - --json <<'EOF'
# Epic Title

## Problem
Clear problem statement

## Key Decisions
Decisions made during interview

## Edge Cases
- Edge case 1
- Edge case 2

## Open Questions
Unresolved items

## Acceptance
- [ ] Criterion 1
- [ ] Criterion 2
EOF
```

### For Flow Task ID (fn-N.M)

**First check if task has existing spec from planning:**
```bash
$FLOWCTL cat <id>
```

**If task has substantial planning content** (description with file refs, sizing, approach):
- **Do NOT overwrite** — planning detail would be lost
- Only ADD new acceptance criteria discovered in interview:
  ```bash
  # Read existing acceptance, append new criteria
  $FLOWCTL task spec <id> --file /tmp/acc.md --json
  ```
- Or suggest interviewing the epic instead: `/flow-code:interview <epic-id>`

**If task is minimal** (just title, empty or stub description):
- Update task with interview findings
- Focus on **requirements**, not implementation details

```bash
$FLOWCTL task spec <id> --desc /tmp/desc.md --accept /tmp/acc.md --json
```

Description should capture:
- What needs to be accomplished (not how)
- Edge cases discovered in interview
- Constraints and requirements

Do NOT add: file/line refs, sizing, implementation approach — that's plan's job.

### For File Path

Rewrite the file with refined spec:
- Preserve any existing structure/format
- Add sections for areas covered in interview
- Include edge cases, acceptance criteria
- Keep it requirements-focused (what, not how)

This is typically a pre-epic doc. After interview, suggest `/flow-code:plan <file>` to create epic + tasks.

## Completion

Show summary:
- Number of questions asked
- Key decisions captured
- What was written (Flow ID updated / file rewritten)

Suggest next step based on input type:
- New idea / epic without tasks → `/flow-code:plan fn-N`
- Epic with tasks → `/flow-code:work fn-N` (or more interview on specific tasks)
- Task → `/flow-code:work fn-N.M`
- File → `/flow-code:plan <file>`

## Notes

- This process should feel thorough - user should feel they've thought through everything
- Quality over speed - don't rush to finish
