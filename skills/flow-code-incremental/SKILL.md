---
name: flow-code-incremental
description: "Use when implementing features in Worker Phase 5. Enforces vertical slicing, incremental commits, scope discipline, and the Implement-Test-Verify-Commit cycle."
tier: 2
user-invocable: false
---
<!-- SKILL_TAGS: implementation,slicing,scope,commits -->

# Incremental Implementation

## Overview

Build features as thin vertical slices. Each slice is a complete, tested, committed increment that leaves the system working. Never implement an entire feature at once — slice it, verify it, commit it, move on.

## When to Use

- Worker Phase 5 (Implement) — this is the core methodology
- Any multi-file feature implementation
- Refactoring that touches multiple modules

**When NOT to use:**
- Single-line config changes
- Documentation-only changes
- Trivial bug fixes (one file, one line)

## The Increment Cycle

```
┌──────────┐    ┌──────┐    ┌────────┐    ┌────────┐
│ IMPLEMENT│───>│ TEST │───>│ VERIFY │───>│ COMMIT │──┐
└──────────┘    └──────┘    └────────┘    └────────┘  │
     ^                                                 │
     └─────────────── next slice ─────────────────────┘
```

For EACH slice:
1. **IMPLEMENT** — Write the smallest piece that adds observable value
2. **TEST** — Write or update tests for this slice
3. **VERIFY** — Build compiles, tests pass, lint clean (`flowctl guard`)
4. **COMMIT** — Atomic commit with conventional message
5. **NEXT** — Move to the next slice

**The system must be in a working state after every cycle.**

## Slicing Strategies

### Vertical Slicing (preferred)

Cut through all layers for one behavior:

```
Slice 1: User can submit login form → handler + validation + DB check + response
Slice 2: Failed login shows error → error path + UI feedback
Slice 3: Rate limiting on login → middleware + config + tests
```

Each slice delivers a complete, testable behavior.

### Contract-First (for API/interface work)

1. Define the interface/types first → commit
2. Write tests against the interface → commit
3. Implement to satisfy the tests → commit

### Risk-First (for uncertain areas)

1. Implement the riskiest/least understood piece first
2. If it fails, you fail fast — before investing in easy parts
3. Reduces wasted work on dependent slices

## Scope Discipline

### The Scope Fence

Before each slice, state what you WILL and WON'T touch:

```
WILL: Add email validation to the signup handler
WON'T: Refactor the validation library, update other handlers, change error format
```

### Detecting Scope Creep

Stop immediately if you're about to:
- Fix an unrelated bug you noticed
- Refactor code that works but "could be better"
- Add a feature not in the task spec
- Update tests for code you didn't change
- "Clean up" imports or formatting in untouched files

**What to do instead:**
- Note it in the task outputs (Phase 9)
- Create a follow-up task: `flowctl task create --epic $EPIC_ID --title "Refactor X"`
- Return to the current slice

### The One-Thing Rule

Each slice does exactly one thing. If you can describe it with "X and Y", split it:

```
Bad:  "Add validation and update error messages"
Good: Slice 1: "Add email validation"
      Slice 2: "Update error messages for validation failures"
```

## Implementation Rules

### Keep It Compilable

After every edit, the code must compile. If a change breaks compilation:
- Fix it immediately before moving on
- Don't leave `// TODO: fix this` compile errors
- If you can't fix it quickly, revert the slice

### Safe Defaults

New features default to OFF or the most conservative setting:
```typescript
// Good: new feature disabled by default
const ENABLE_NEW_AUTH = process.env.ENABLE_NEW_AUTH === 'true';

// Bad: new feature enabled, old path unreachable
```

### Rollback-Friendly

Every slice must be individually revertable:
- No slice depends on being applied in a specific order
- Each commit is a valid state (not "part 1 of 3")
- `git revert <commit>` should produce a working system

### Minimize File Overlap

When multiple workers run in parallel:
- Each slice should touch distinct files when possible
- If two slices must touch the same file, make them sequential (add dependency)
- Use `--files` in task spec to declare ownership

## The Save Point Pattern

Treat commits as save points in a game:
- After each passing test → commit (save point)
- If the next slice goes wrong → revert to last save point
- Never lose more than one slice of work

```bash
# After each green test:
git add -A
git commit -m "feat(auth): add email validation to signup"

# If next slice breaks:
git stash  # or git reset --soft HEAD~1
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "I'll commit when the whole feature is done" | Large uncommitted changes are fragile. One mistake = lose everything. |
| "This small fix is related, might as well do it now" | Scope creep. Note it, create a task, stay focused. |
| "Splitting this into slices would take longer" | It feels slower but catches bugs earlier and produces revertable history. |
| "Tests slow me down" | Untested slices create debugging debt that costs 10x more later. |
| "I need to refactor first before I can add this" | Only refactor what's blocking the current slice. File a task for the rest. |

## Red Flags

- Uncommitted changes spanning 5+ files without a commit
- "Work in progress" commits with broken tests
- Single commit touching 10+ files across multiple concerns
- Refactoring mixed with feature work in the same slice
- Tests written after all implementation is done (not per-slice)
- `// TODO` comments for things that should be separate tasks
- Feature flags missing for partially-complete features

## Verification

After each slice:

- [ ] Code compiles without errors
- [ ] Tests pass (new + existing)
- [ ] Lint clean
- [ ] Committed with conventional message
- [ ] Slice does one thing (describable without "and")
- [ ] No scope creep (only task-spec work in this commit)
- [ ] System is in working state (could deploy this commit)
