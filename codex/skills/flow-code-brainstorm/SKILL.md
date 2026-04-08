---
name: flow-code-brainstorm
description: "Use when exploring requirements before planning. Pressure-tests ideas, generates approaches, and outputs a requirements doc for /flow-code:plan."
tier: 3
user-invocable: false
---

# Flow brainstorm

Explore and pressure-test an idea before committing to a plan. Outputs a requirements doc that feeds directly into `/flow-code:plan`.

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

**Role**: product strategist, requirements explorer
**Goal**: pressure-test ideas before planning to avoid wasted implementation effort

## Input

Full request: $ARGUMENTS

Accepts:
- Feature/bug description in natural language
- Empty: ask "What idea or problem should we brainstorm? Describe it in 1-5 sentences."

Examples:
- `/flow-code:brainstorm Add real-time collaboration to the editor`
- `/flow-code:brainstorm We keep getting auth token expiry bugs`
- `/flow-code:brainstorm migrate from REST to GraphQL`

## Phase 0: Complexity Assessment

Analyze the request and the codebase to gauge complexity:

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

Read relevant code, git log, and project structure to understand the scope.

Classify into one of three tiers:

### Trivial (1-2 files, clear fix, well-understood change)
- Skip brainstorm entirely.
- Tell the user: "This looks straightforward — skip brainstorm and go directly to planning."
- Suggest: `Run /flow-code:plan <their request>` and stop here.

### Medium (clear feature, moderate scope)
- Run a **quick brainstorm**: ask only the 3 pressure-test questions (Phase 1), then jump to Phase 2 with 2 approaches.

### Large (cross-cutting, vague requirements, multiple systems affected)
- Run the **full brainstorm**: all phases, 3 approaches.

Tell the user which tier you picked and why (one sentence).

## Phase 1: Pressure Test

Ask exactly 3 questions, **one at a time**, using `AskUserQuestion` for each.

**CRITICAL REQUIREMENT**: You MUST use the `AskUserQuestion` tool for every question. Do NOT output questions as plain text — they will be ignored.

Wait for each answer before asking the next question.

### Question 1: Who and why?
> Who uses this? What's the specific pain point or motivation?

### Question 2: Cost of inaction?
> What happens if we do nothing? What's the actual cost or risk?

### Question 3: Simpler framing?
> Is there a simpler version that delivers 80% of the value? What's the minimum viable version?

After all 3 answers, summarize the key insights in 2-3 bullets before proceeding.

## Phase 2: Approach Generation

Generate 2-3 concrete approaches based on the answers from Phase 1 and your codebase analysis.

For each approach, provide:

| Field | Format |
|-------|--------|
| **Name** | Short descriptive label |
| **Summary** | One sentence — what this approach does |
| **Effort** | S / M / L |
| **Risk** | Low / Med / High |
| **Pros** | 2-3 bullets |
| **Cons** | 2-3 bullets |

Present the approaches and ask the user (via `AskUserQuestion`):
> Which approach do you prefer? (1/2/3, or "combine" to mix elements)

## Phase 3: Requirements Output

Based on the chosen approach, write a requirements document:

```bash
# Generate slug from the idea
SLUG=$(echo "$IDEA" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-' | head -c 40)

# Ensure .flow/specs/ exists
mkdir -p .flow/specs

# Write requirements doc
```

Write to `.flow/specs/${SLUG}-requirements.md` with this format:

```markdown
# Requirements: <Title>

## Problem
<1-2 sentences from pressure test answers>

## Users
<Who uses this, from Q1>

## Chosen Approach
<Name and summary of selected approach>

## Requirements
- [ ] <Requirement 1>
- [ ] <Requirement 2>
- [ ] <Requirement 3>
...

## Non-Goals
- <What this explicitly does NOT include>

## Constraints
- <Technical or business constraints identified during brainstorm>

## Open Questions
- <Anything unresolved that /flow-code:plan should address>
```

After writing the file, tell the user:

```
Requirements written to .flow/specs/<slug>-requirements.md

Next step: Run /flow-code:plan .flow/specs/<slug>-requirements.md
```
