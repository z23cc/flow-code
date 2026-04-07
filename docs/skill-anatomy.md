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
---
```

**Rules:**
- `name`: Lowercase, hyphen-separated. Must match directory name. Always starts with `flow-code-`.
- `description`: Starts with "Use when...". Max 500 characters. Third person.
  - Include: triggering conditions, symptoms, contexts.
  - Exclude: workflow summary, process steps, what the skill does.

**Why:** Descriptions are injected into system prompts for skill discovery. If the description contains process steps, agents follow the summary and skip the actual skill content.

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

For flow-code skills, phases follow the convention: Phase 1, Phase 2, Phase 3, etc. (always integers). Reference `flowctl` commands where applicable:
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
- Phase 5: Implement
- Phase 6: Verify & Fix
- Phase 7: Commit
- Phase 8: Review
- Phase 10: Complete

Phase IDs are always integers. Skills that define their own phases should use sequential integers (Phase 1, Phase 2, Phase 3) for consistency with the worker pipeline.

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
- [ ] All six required sections present (Overview, When to Use, Core Process, Common Rationalizations, Red Flags, Verification)
- [ ] Rationalizations table has 3+ entries with factual rebuttals
- [ ] Red flags list has observable symptoms (not vague advice)
- [ ] Verification checklist items are evidence-backed
- [ ] SKILL.md is under 500 lines
- [ ] Registered in plugin.json and README.md skills table
