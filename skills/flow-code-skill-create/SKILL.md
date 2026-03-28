---
name: flow-code-skill-create
description: Use when creating new flow-code skills, editing existing skills, or verifying skills work before deployment. TDD applied to documentation.
---

# Creating Flow-Code Skills

Writing skills IS Test-Driven Development applied to documentation. If you didn't watch an agent fail without the skill, you don't know if the skill teaches the right thing.

## When to Create

**Create when:** technique wasn't obvious, you'd reference it again, pattern applies broadly.
**Don't create for:** one-off solutions, project-specific conventions (use CLAUDE.md), things enforceable with automation.

## Skill Types

- **Technique** — concrete method with steps (flow-code-debug, flow-code-auto-improve)
- **Pattern** — way of thinking about problems (review verification, stack detection)
- **Reference** — API docs, tool docs (flowctl-reference)

## Directory Structure

```
skills/flow-code-<name>/
  SKILL.md              # Main reference (required)
  workflow.md           # Extended workflow (if needed)
  templates/            # Scripts, templates (if needed)
```

## SKILL.md Structure

```yaml
---
name: flow-code-<name>
description: Use when [triggering conditions and symptoms only — NEVER summarize workflow]
---
```

**CRITICAL: description = when to use, NOT what the skill does.** Testing shows that workflow summaries in description cause agents to follow the description and skip the actual skill content.

```markdown
# Skill Title

## Overview
Core principle in 1-2 sentences.

## When to Use
Symptoms, triggers, use cases.

## Core Process / Pattern
The actual workflow — inline code for simple patterns.

## Common Mistakes
What goes wrong + fixes.
```

## The Iron Law

```
NO SKILL WITHOUT A FAILING TEST FIRST
```

## RED-GREEN-REFACTOR Cycle

### RED: Baseline Test

1. Create a pressure scenario — a task where the agent would benefit from this skill
2. Run the scenario WITHOUT the skill (use a subagent)
3. Document exact behavior:
   - What choices did the agent make?
   - What rationalizations did it use?
   - Where did it go wrong?

### GREEN: Write Minimal Skill

1. Write the skill addressing those specific failures
2. Run the SAME scenario WITH the skill loaded
3. Verify the agent now complies

### REFACTOR: Close Loopholes

1. Agent found a new rationalization? Add explicit counter
2. Build rationalization table from all test iterations
3. Add red flags list for self-checking
4. Re-test until bulletproof

## Frontmatter Rules

- `name`: letters, numbers, hyphens only. Prefix with `flow-code-`
- `description`: start with "Use when...", max 500 chars, third person
  - Include: triggering conditions, symptoms, contexts
  - Exclude: workflow summary, process steps, what the skill does
- Keywords in body for discovery: error messages, symptoms, tool names

## Bulletproofing Discipline Skills

For skills that enforce rules (debugging, TDD, verification):

**Close every loophole explicitly:**
```markdown
# Bad
Write code before test? Delete it.

# Good
Write code before test? Delete it. Start over.
**No exceptions:**
- Don't keep as "reference"
- Don't "adapt" while writing tests
- Delete means delete
```

**Add rationalization table:**
```markdown
| Excuse | Reality |
|--------|---------|
| "Too simple to need this" | Simple things break too |
| "I'll do it properly later" | Later never comes |
```

## Integration with Flow-Code

- Skills live in `skills/flow-code-<name>/`
- Register in `.claude-plugin/plugin.json` (update description + count)
- If skill needs a flowctl command, add to `scripts/flowctl.py`
- Sync to plugin cache + marketplaces after deployment
- Add to README.md skills table

## Checklist

**RED:**
- [ ] Pressure scenario created
- [ ] Baseline behavior documented (without skill)
- [ ] Failure patterns identified

**GREEN:**
- [ ] SKILL.md with frontmatter, overview, process, mistakes
- [ ] Description starts with "Use when..." (no workflow summary)
- [ ] Scenario re-run with skill — agent complies

**REFACTOR:**
- [ ] New rationalizations countered
- [ ] Rationalization table added (if discipline skill)
- [ ] Red flags list added (if discipline skill)

**DEPLOY:**
- [ ] Committed to git
- [ ] Plugin copies synced (cache + marketplaces)
- [ ] README.md skills table updated
- [ ] plugin.json skill count updated
