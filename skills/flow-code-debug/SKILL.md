---
name: flow-code-debug
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
---

# Systematic Debugging

## The Iron Law

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

If you haven't completed Phase 1, you cannot propose fixes.

## When to Use

- Test failures, bugs, unexpected behavior, performance problems, build failures
- **Especially when:** under time pressure, "quick fix" seems obvious, already tried multiple fixes, previous fix didn't work

## Phase 1: Root Cause Investigation

**BEFORE attempting ANY fix:**

1. **Read error messages completely** — stack traces, line numbers, error codes. Don't skip.

2. **Reproduce consistently** — exact steps, every time. If not reproducible, **STOP** — gather more data (logs, environment, timing). Do NOT proceed to Phase 2 without reproduction. Guessing without reproduction = symptom fixing.

3. **Check recent changes:**
   ```bash
   git log --oneline -10
   git diff HEAD~3
   ```

4. **Run guards to establish baseline:**
   ```bash
   <FLOWCTL> guard
   ```

5. **Gather evidence in multi-component systems:**
   - Log what ENTERS each component
   - Log what EXITS each component
   - Find WHERE it breaks BEFORE investigating WHY

6. **Trace data flow** — where does the bad value originate? Trace backward through the call chain to the source. Fix at source, not at symptom.

## Phase 2: Pattern Analysis

1. **Find working examples** — similar working code in same codebase
2. **Compare completely** — list EVERY difference between working and broken
3. **Understand dependencies** — what config, environment, assumptions?

## Phase 3: Hypothesis and Testing

1. **Form single hypothesis** — "I think X is root cause because Y"
2. **Test minimally** — smallest possible change, one variable at a time
3. **Verify** — did it work? Yes → Phase 4. No → form NEW hypothesis. Don't stack fixes.

## Phase 4: Implementation

1. **Write failing test** (if TDD mode or test framework available):
   ```bash
   # Test must fail, proving the bug exists
   <FLOWCTL> guard --layer <affected-layer>
   ```

2. **Implement single fix** — address root cause, ONE change, no "while I'm here" improvements.
   **No bundling:** Do NOT fix multiple things at once. If you're tempted to "also fix this other thing", STOP — commit the single fix first, verify, then address the next issue separately.

3. **Verify fix:**
   ```bash
   <FLOWCTL> guard
   ```

4. **If fix doesn't work:**
   - < 3 attempts: return to Phase 1, re-analyze
   - **>= 3 attempts: STOP — question the architecture.** 3+ failures = architectural problem, not hypothesis problem. Discuss with user before attempting more fixes.

## Red Flags — STOP and Return to Phase 1

- "Quick fix for now, investigate later"
- "Just try changing X and see"
- "I don't fully understand but this might work"
- Proposing solutions before tracing data flow
- "One more fix attempt" (after 2+ failures)
- Each fix reveals new problem in different place

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Issue is simple, don't need process" | Simple issues have root causes too |
| "Emergency, no time" | Systematic is FASTER than guess-and-check |
| "Multiple fixes at once saves time" | Can't isolate what worked; causes new bugs |
| "I see the problem, let me fix it" | Seeing symptoms != understanding root cause |
| "One more fix attempt" (after 2+) | 3+ failures = architectural problem |

## Quick Reference

| Phase | Key Activities | Done When |
|-------|---------------|-----------|
| 1. Root Cause | Read errors, reproduce, check changes, trace data | Understand WHAT and WHY |
| 2. Pattern | Find working examples, compare differences | Identified the delta |
| 3. Hypothesis | Form theory, test ONE variable | Confirmed or new hypothesis |
| 4. Implement | Write test, fix root cause, verify | Bug resolved, guards pass |

## After Fix

```
Bug fixed. Next:
1) Review the fix: `/flow-code:impl-review --base <pre-fix-commit>`
2) Continue current work: `/flow-code:work <epic-id>`
```
