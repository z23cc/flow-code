---
name: flow-code-debug
description: Use when encountering any bug, test failure, or unexpected behavior, before proposing fixes
tier: 2
---
<!-- AUTO-GENERATED from SKILL.md.tmpl — DO NOT EDIT DIRECTLY -->

# Systematic Debugging

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

## flowctl Setup

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

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
   $FLOWCTL guard
   ```

5. **Gather evidence in multi-component systems:**
   - Log what ENTERS each component
   - Log what EXITS each component
   - Find WHERE it breaks BEFORE investigating WHY

6. **Trace data flow** — where does the bad value originate? Trace backward through the call chain to the source. Fix at source, not at symptom.

## Phase 1.5: RP Deep Investigation (optional)

**After Phase 1, before Pattern Analysis.** Uses RepoPrompt to gather cross-file context around the bug. Three-tier fallback — skip entirely if RP is unavailable.

```bash
# Detect RP tier (pass --mcp-hint if mcp__RepoPrompt__context_builder is in your tool list)
RP_TIER=$($FLOWCTL rp tier)  # or: $FLOWCTL rp tier --mcp-hint
```

- **If RP_TIER is `mcp`**: Call `context_builder(instructions: "Investigate bug: <symptoms>. Hypotheses: <hypotheses>. Trace data flow, find related code paths, identify likely root cause.", response_type: "question")`. Timeout 120s.
- **If RP_TIER is `cli`**: Run `timeout 120 rp-cli -e 'builder "Investigate bug: <symptoms>..." --response-type question'`
- **If RP_TIER is `none`**: Skip Phase 1.5 entirely — proceed to Phase 2.

**Use RP findings to guide Phase 2**: RP may surface related code, similar patterns, or architectural context that informs your pattern analysis. Feed these findings into Phase 2 as additional evidence alongside your own investigation.

## Phase 2: Pattern Analysis

1. **Find working examples** — similar working code in same codebase
2. **Compare completely** — list EVERY difference between working and broken
3. **Understand dependencies** — what config, environment, assumptions?

## Phase 3: Hypothesis and Testing

1. **Form single hypothesis** — "I think X is root cause because Y"
2. **Test minimally** — smallest possible change, one variable at a time
3. **Verify** — did it work? Yes → Phase 4. No → form NEW hypothesis. Don't stack fixes.

## Phase 4: Implementation

### Prove-It Pattern (mandatory for bug fixes)

1. **Write reproduction test** — a test that demonstrates the bug (MUST FAIL)
2. **Confirm RED** — run the test, verify it actually fails. If it passes, your test doesn't reproduce the bug
3. **Fix root cause** — implement the fix (not a workaround)
4. **Confirm GREEN** — run the test, verify it now passes
5. **Run full suite** — check for regressions: `$FLOWCTL guard`

**If the test passes on step 2, your test is wrong.** Go back to Phase 1 and refine your understanding of the bug.

### Fix Discipline

- **Implement single fix** — address root cause, ONE change, no "while I'm here" improvements.
  **No bundling:** Do NOT fix multiple things at once. If you're tempted to "also fix this other thing", STOP — commit the single fix first, verify, then address the next issue separately.

4. **If fix doesn't work — failure escalation:**

   **Track your attempt count.** Each failed fix escalates the response:

   | Attempt | Level | Forced Action |
   |---------|-------|---------------|
   | 2nd | L1 — Switch approach | Use a **fundamentally different** method. Tweaking the same logic doesn't count. |
   | 3rd | L2 — Deep investigation | Search online + read source code + list 3 distinct hypotheses before trying anything. |
   | 4th | L3 — 7-point checklist | Complete ALL items below. Skipping any = you're still guessing. |
   | 5th+ | L4 — Architecture review | **STOP.** Discuss with user. This is not a bug — it's a design problem. |

   ### 7-Point Checklist (mandatory at L3+)

   - [ ] Read the error message character-by-character? (not skimming)
   - [ ] Used tools to search the core problem? (grep, web search, docs)
   - [ ] Read 50+ lines of context around the failure location?
   - [ ] Verified ALL assumptions with tools? (versions, paths, permissions, deps)
   - [ ] Tried the **opposite** assumption? (if "problem is in A" failed, try "problem is NOT in A")
   - [ ] Can reproduce in minimal scope? (smallest possible repro case)
   - [ ] Switched tools/method/angle? (different debugger, different approach, different layer)

   **All 7 must be checked before attempting another fix at L3+.**

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
| "Tried everything" | Did you search? Read source? Complete the 7-point checklist? |
| "Probably an environment issue" | Did you verify that? Unverified attribution = guessing |
| "Need more context" | You have tools. Search first, ask only what's truly unavailable |
| "Suggest handling manually" | This is your bug. Own it. Exhaust all options first |
| Same logic, different parameters | Tweaking parameters is NOT a different approach. Change the method. |
| "Bug is too simple for a test" | Simple bugs regress. The test takes 2 minutes. The re-diagnosis takes 2 hours. |

## Quick Reference

| Phase | Key Activities | Done When |
|-------|---------------|-----------|
| 1. Root Cause | Read errors, reproduce, check changes, trace data | Understand WHAT and WHY |
| 1.5 RP Investigate | context_builder(question) with symptoms + hypotheses | Cross-file context gathered (or skipped if no RP) |
| 2. Pattern | Find working examples, compare differences | Identified the delta |
| 3. Hypothesis | Form theory, test ONE variable | Confirmed or new hypothesis |
| 4. Implement | Prove-It: RED test → fix root cause → GREEN test → full suite | Bug resolved, guards pass |

## After Fix

```
Bug fixed. Next:
1) Review the fix: `/flow-code:impl-review --base <pre-fix-commit>`
2) Continue current work: `/flow-code:work <epic-id>`
```