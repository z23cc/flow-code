# Skill Anatomy

Standard structure for flow-code skill files. Use this when creating or reviewing skills.

## File Location

Every skill lives in its own directory under `skills/`:

```
skills/flow-code-<name>/
  SKILL.md              # Required: The skill definition (<=500 lines)
  workflow.md           # Optional: Extended workflow if SKILL.md overflows
  templates/            # Optional: Scripts, config templates
```

Prefix all skill directories with `flow-code-`. Main file is always `SKILL.md` (uppercase).

## YAML Frontmatter (Required)

```yaml
---
name: flow-code-<name>
description: Use when [triggering conditions and symptoms only]
# --- Optional fields (all backward-compatible) ---
# allowed-tools:                     # Tool allowlist (allowlist, not denylist)
#   - Bash
#   - Read
#   - Edit
#   - Glob
#   - Grep
# version: 1.0.0                     # SemVer skill version
# model: sonnet                       # LLM model override (haiku|sonnet|opus|inherit)
# preamble-tier: 1                    # Startup cost: 1=none, 2=light, 3=heavy
# voice-triggers:                     # Speech-to-text aliases for discovery
#   - "flow plan"
#   - "make a plan"
# user-invocable: false               # Hide from / menu (background knowledge only)
# argument-hint: "<epic-id>"          # Autocomplete hint shown in / menu
# context: fork                       # Run in isolated subagent context
# agent: Explore                      # Subagent type when context: fork
# effort: medium                      # Model reasoning effort (low|medium|high|max)
# hooks: { pre-tool-call: ... }       # Lifecycle hooks scoped to this skill
# paths: "*.rs,*.toml"                # Glob patterns limiting auto-activation
# shell: bash                         # Shell for DCI blocks (bash|powershell)
---
```

### Required Fields

- `name`: Lowercase, hyphen-separated. Must match directory name. Always starts with `flow-code-`.
- `description`: Starts with "Use when...". Max 500 characters. Third person.
  - Include: triggering conditions, symptoms, contexts.
  - Exclude: workflow summary, process steps, what the skill does.

**Why:** Descriptions are injected into system prompts for skill discovery. If the description contains process steps, agents follow the summary and skip the actual skill content.

### Optional Fields Reference

All optional fields are backward-compatible. Omitting them changes nothing for existing skills.

| Field | Type | Description |
|-------|------|-------------|
| `allowed-tools` | list | Tool allowlist — tools permitted without prompts when skill is active. Valid: `Read`, `Write`, `Edit`, `Bash`, `Glob`, `Grep`, `WebFetch`, `WebSearch`, `Task`, `TodoWrite`, `NotebookEdit`, `AskUserQuestion`, `Skill`. Bash supports patterns: `Bash(cargo:*)` |
| `version` | string | SemVer version (e.g., `1.0.0`). Useful for tracking skill evolution and coordinating updates across plugins |
| `model` | string | LLM model override: `haiku`, `sonnet`, `opus`, or `inherit` (default). Use sparingly — most skills should inherit the session model |
| `preamble-tier` | integer | Startup cost indicator: `1` = no preamble (instant), `2` = light preamble (env detection), `3` = heavy preamble (network, builds). Helps agents estimate activation cost |
| `voice-triggers` | list | Speech-to-text aliases for discovery. Handles common STT misheard variants (e.g., `"flow plan"` for `/flow-code:plan`). Listed in the description at render time |
| `user-invocable` | boolean | Set `false` to hide from `/` menu. Skill becomes background knowledge only, intended for agent preloading |
| `argument-hint` | string | Autocomplete hint shown in the `/` menu (e.g., `[epic-id]`, `<file-path>`) |
| `context` | string | Set to `fork` to run the skill in an isolated subagent context |
| `agent` | string | Subagent type when `context: fork` is set (default: `general-purpose`) |
| `effort` | string | Override model reasoning effort: `low`, `medium`, `high`, `max` |
| `hooks` | object | Lifecycle hooks scoped to this skill (pre-tool-call, post-tool-call, etc.) |
| `paths` | string/list | Glob patterns limiting auto-activation. Accepts comma-separated string or YAML list |
| `shell` | string | Shell for DCI (`` !`command` ``) blocks: `bash` (default) or `powershell` |

**Key difference from agents:** Skills use `allowed-tools` (allowlist) while agents use `disallowedTools` (denylist). The `effort` and `maxTurns` fields originated as agent-only but `effort` is now available for skills too.

## Required Sections

```markdown
# Skill Title

## Overview
Core principle in 1-2 sentences. What this skill enforces and why it matters.

## When to Use
- Triggering conditions (symptoms, task types, failure patterns)
- **When NOT to use:** exclusions to prevent misapplication

## Core Process
The step-by-step workflow. Numbered phases or steps.
Include inline code examples. Use ASCII flowcharts for decision points.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Too simple to need this" | Simple things break too |
| "I'll do it properly later" | Later never comes |

## Red Flags
- Observable symptoms indicating the skill is being violated
- Patterns to watch for during self-check and review

## Verification
After completing the process, confirm:
- [ ] Checklist item with verifiable evidence
- [ ] Another checkpoint (test output, build result, etc.)
```

## Section Purposes

### Overview
The elevator pitch. Answers: what does this skill enforce, and why should an agent follow it?

### When to Use
Helps agents decide if this skill applies. Include both positive triggers ("Use when X") and negative exclusions ("NOT for Y"). The flow-code-debug exemplar:
> Test failures, bugs, unexpected behavior, performance problems, build failures.
> **Especially when:** under time pressure, "quick fix" seems obvious.

### Core Process
The heart of the skill. Step-by-step workflow the agent follows. Must be specific and actionable.

**Good:** "Run `cargo test --all` and verify zero failures"
**Bad:** "Make sure the tests work"

For flow-code skills, phases follow the convention: Phase 1, Phase 2, Phase 2.5 (verify), Phase 3 (commit), etc. Reference `flowctl` commands where applicable:
```bash
$FLOWCTL guard              # Run all guards
$FLOWCTL invariants check   # Check architecture invariants
```

### Common Rationalizations
The most distinctive section. Excuses agents use to skip important steps, paired with factual rebuttals. Think of every time an agent said "I'll add tests later" or "This is simple enough to skip" -- those go here.

This is the core anti-rationalization principle. Every skip-worthy step needs a counter-argument. Without this section, agents reliably talk themselves out of following the process.

### Red Flags
Observable signs the skill is being violated. Phrased as quotes or behaviors:
- "Quick fix for now, investigate later"
- "I don't fully understand but this might work"
- Proposing solutions before completing investigation
- Each fix reveals a new problem in a different place

### Verification
Exit criteria as a checkbox checklist. Every item must be verifiable with evidence (test output, build result, git diff, flowctl output). No subjective items like "code looks good."

## Supporting Files

Create supporting files only when:
- SKILL.md exceeds 500 lines (overflow to `workflow.md` or similar)
- Reusable scripts or templates are needed
- Long checklists justify separate files

Keep patterns and principles inline when under ~50 lines. Most skills need only SKILL.md.

## Writing Principles

1. **Process over knowledge.** Skills are workflows, not reference docs. Steps, not facts.
2. **Specific over general.** `$FLOWCTL guard` beats "verify the code works."
3. **Evidence over assumption.** Every verification checkbox requires proof.
4. **Anti-rationalization is core.** Every skip-worthy step needs a counter-argument in the rationalizations table. This is what separates effective skills from advice.
5. **Token-conscious.** Every section must justify its inclusion. If removing it wouldn't change agent behavior, remove it.
6. **Progressive disclosure.** SKILL.md is the entry point. Supporting files load on demand.

## Flow-Code Specifics

### flowctl Integration
Skills that enforce process should reference flowctl commands:
- `$FLOWCTL guard` for verification gates
- `$FLOWCTL done --summary-file --evidence-json` for evidence-based completion
- `$FLOWCTL invariants check` for architecture invariant enforcement

### Evidence Requirements
Flow-code workers produce evidence JSON. Skills should specify what evidence their process generates:
```json
{"commits": ["abc123"], "tests": ["cargo test --all"], "prs": []}
```

### Phase Naming
Follow the worker agent convention:
- Phase 1: Re-anchor (read spec)
- Phase 2: Implement
- Phase 2.5: Verify & Fix
- Phase 3: Commit
- Phase 4: Review
- Phase 5: Complete

Skills that define their own phases should use this numbering style (Phase 1, Phase 1.5, Phase 2) for consistency with the worker pipeline.

### Cross-Skill References
Reference other skills by name, don't duplicate:
```markdown
If the build breaks, use the `flow-code-debug` skill.
For test-first development, see `flow-code-tdd`.
```

## Exemplar

The `flow-code-debug` skill (`skills/flow-code-debug/SKILL.md`) is the reference implementation. It demonstrates all required sections including the Common Rationalizations table (lines 137-151) and Red Flags list (lines 128-135).

## Checklist for New Skills

- [ ] Directory created as `skills/flow-code-<name>/`
- [ ] SKILL.md has valid YAML frontmatter with `name` and `description`
- [ ] Description starts with "Use when..." (no workflow summary)
- [ ] Optional fields (if used) are valid: `allowed-tools` lists real tools, `version` is SemVer, `model` is a known alias
- [ ] All six required sections present (Overview, When to Use, Core Process, Common Rationalizations, Red Flags, Verification)
- [ ] Rationalizations table has 3+ entries with factual rebuttals
- [ ] Red flags list has observable symptoms (not vague advice)
- [ ] Verification checklist items are evidence-backed
- [ ] SKILL.md is under 500 lines
- [ ] Registered in plugin.json and README.md skills table
