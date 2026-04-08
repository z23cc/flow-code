---
name: flow-code-code-review
description: "Use when reviewing code changes — self-review in Worker Phase 6, impl-review, or PR review. Applies five-axis scoring with severity labels."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: review,quality,correctness,security -->

# Five-Axis Code Review

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

## flowctl Setup

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Overview

Structured code review across five orthogonal axes. Every finding gets a severity label. Every review ends with a clear verdict. This replaces ad-hoc "looks good" reviews with systematic quality gates.

## When to Use

- Worker Phase 6 (self-review before commit)
- Implementation review (impl-review phase)
- PR review before merge
- Manual code review requests

**When NOT to use:**
- Plan review (that checks spec alignment, not code quality)
- Architecture design review (use flow-code-api-design instead)

## The Five Axes

Review every diff across these five dimensions. Each axis is independent — a change can score well on correctness but poorly on security.

### Axis 1: Correctness

- Does the code do what the spec says?
- Are edge cases handled? (null, empty, boundary values, overflow)
- Are errors caught and handled appropriately?
- Do tests cover the actual behavior, not just happy path?
- Are race conditions possible under concurrent access?

### Axis 2: Readability & Simplicity

- Can a new team member understand this code without explanation?
- Are names clear and specific? (`processData` → `validateUserInput`)
- Is the logic straightforward or unnecessarily clever?
- Are functions under 40 lines? Files under 300 lines?
- Is there dead code, commented-out code, or TODOs that should be tickets?

### Axis 3: Architecture

- Does the change follow existing project patterns?
- Are module boundaries respected? (no cross-layer imports)
- Is the abstraction level appropriate? (not too deep, not too shallow)
- Would this change make future modifications harder?
- Are dependencies flowing in the right direction?

### Axis 4: Security

- Is all user input validated at the boundary?
- Are queries parameterized?
- Are auth and authorization checks present where needed?
- Are secrets, tokens, or PII protected?
- See `flow-code-security` skill for full checklist.

### Axis 5: Performance

- Are there N+1 query patterns?
- Are there unbounded loops or data fetches?
- Could this block the main thread or event loop?
- Are expensive operations cached where appropriate?
- Are there unnecessary re-renders (React) or recomputations?

## Severity Labels

Every finding MUST have a severity label:

| Label | Meaning | Action Required |
|-------|---------|-----------------|
| **Critical** | Blocks merge. Bug, security hole, data loss risk. | Must fix before proceeding. |
| **Important** | Should fix. Correctness concern or significant quality issue. | Fix unless strong justification. |
| **Suggestion** | Consider this. Improvement opportunity. | Author decides. |
| **Nit** | Minor style or naming preference. | Author may ignore. |
| **FYI** | Informational. No action needed. | For awareness only. |

## Change Sizing

| Lines Changed | Rating | Action |
|---------------|--------|--------|
| ~100 | Good | Easy to review thoroughly |
| ~300 | Acceptable | Review in one sitting |
| ~500 | Large | Consider splitting |
| ~1000+ | Too large | Split into smaller changes |

If a diff exceeds 500 lines, flag it: "This change is large (~N lines). Consider splitting into focused commits."

## Review Process

### Step 1: Understand Context

Before looking at code:
- Read the task spec / PR description
- Understand WHAT should have changed and WHY
- Check which files are expected to change

### Step 2: Review Tests First

- Do tests exist for the change?
- Do they test behavior, not implementation?
- Are edge cases covered?
- Would the tests catch a regression if someone reverted the key logic?

### Step 3: Review Implementation

Walk through the diff with all five axes active:
- Read file-by-file in dependency order (utilities → services → handlers → tests)
- For each file, check all five axes
- Note findings with severity labels

### Step 4: Categorize Findings

Group findings by severity:
```
## Critical (must fix)
- [Correctness] Missing null check on user.email (line 42) — NPE in production
- [Security] SQL built with template literal (line 89) — injection risk

## Important (should fix)
- [Architecture] Direct DB access from handler — should go through service layer
- [Performance] Fetching all users to count them — use COUNT query

## Suggestions
- [Readability] Rename `processData` to `validateOrderInput` for clarity
- [Readability] Extract lines 120-145 into a named function

## Nits
- [Readability] Inconsistent spacing in object literal (line 67)
```

### Step 5: Verdict

| Verdict | Criteria |
|---------|----------|
| **SHIP** | Zero Critical, zero Important, all Suggestions are minor |
| **NEEDS_WORK** | Has Critical or Important findings that need fixes |
| **MAJOR_RETHINK** | Fundamental approach is wrong — need to redesign |

## Multi-Model Review Pattern

For highest quality, use cross-model review:
1. Model A (Claude) writes the code
2. Model B (Codex/different model) reviews independently
3. Model A addresses findings
4. Human approves final result

This catches blind spots that same-model review misses.

## Multi-Persona Review Mode

For high-stakes reviews (impl-review, pre-merge) where broader coverage matters. Not needed for quick self-reviews in Worker Phase 6.

### Persona Selection

**Always-on** (every multi-persona review):
- `correctness-reviewer` — logic errors, contract violations, edge cases
- `testing-reviewer` — coverage gaps, assertion quality, missing scenarios
- `maintainability-reviewer` — readability, naming, dead code, complexity

**Conditional** (activate based on diff content):

| Persona | Activate when diff touches... |
|---------|-------------------------------|
| `security-reviewer` | Auth, endpoints, input handling, permissions, secrets |
| `performance-reviewer` | Queries, data transforms, caching, async, hot loops |
| `architecture-reviewer` | Module boundaries, new dependencies, public API surface |

All persona agents live in `agents/review/`. Each returns a JSON array of findings conforming to the schema in `docs/findings-schema.md`.

### Dispatch Protocol

```
Step 1: Analyze the diff to determine which conditional reviewers to activate.
Step 2: Spawn all selected reviewers in parallel using Agent tool.
        Each reviewer receives: the diff, the file list, and its persona instructions.
        Each returns: a JSON array of findings per docs/findings-schema.md.
Step 3: Collect all reviewer outputs into temporary JSON files (one per reviewer).
Step 4: Run:  flowctl review merge --files "r1.json,r2.json,..." --json
Step 5: Present merged findings grouped by severity (P0 → P3).
Step 6: Apply safe_auto fixes from fixer_queue automatically.
Step 7: Present gated_auto and manual findings for human decision.
```

### Confidence Calibration

The merge pipeline applies these calibration rules before presenting findings:

- **Suppress threshold**: Drop findings below 0.60 confidence. Exception: P0 findings are kept at 0.50+.
- **Cross-reviewer boost**: When two or more reviewers flag the same issue (same fingerprint), confidence increases by +0.10 (capped at 1.0) and the highest severity wins.
- **Conservative routing**: When reviewers disagree on `autofix_class` for the same finding, the merge keeps the most restrictive class (e.g. if one says `safe_auto` and another says `gated_auto`, the result is `gated_auto`).

### Output Format

Merged output follows the canonical findings schema documented in `docs/findings-schema.md`. Each finding includes `severity`, `category`, `description`, `confidence`, `autofix_class`, `owner`, and the `reviewer` field indicating which persona produced it. When findings are boosted by cross-reviewer agreement, the `evidence` array includes entries from all agreeing reviewers.

The merge pipeline deduplicates using a three-part fingerprint (file + line bucket + normalized description). See the Fingerprinting section in `docs/findings-schema.md` for details.

## Zero-Findings Rule

Reviewers MUST produce at least 3 findings (any severity level). Zero findings is NOT an acceptable review output -- it indicates insufficient analysis, not perfect code.

If the first pass yields zero findings, halt and re-analyze from these angles:

1. **Concurrency/race conditions** -- shared state, lock ordering, TOCTOU, async gaps
2. **Boundary conditions and input edges** -- empty, null, overflow, max-length, unicode, negative
3. **Error propagation paths** -- swallowed errors, missing rollback, partial failure states
4. **Performance degradation scenarios** -- N+1 queries, unbounded growth, hot loops, missing pagination
5. **Security attack surface** -- injection, auth bypass, information leakage, insecure defaults

If after re-analysis there are truly no Critical or Important issues, report at least 3 Suggestion or Nit improvements. Every codebase has room for improvement in naming, structure, documentation, or test coverage.

## Three-Layer Parallel Review

When performing impl_review or epic-level code review, spawn 3 independent review agents in parallel:

### Layer 1: Blind Hunter
- **Input**: Git diff only (`git diff main...HEAD`)
- **Context**: NONE — no spec, no project access, no docs
- **Purpose**: Find issues visible purely from code quality (bugs, logic errors, style)
- **Prompt**: See `prompts/blind-hunter.md`

### Layer 2: Edge Case Hunter
- **Input**: Git diff + read-only project access (Grep/Glob/Read)
- **Context**: Can explore project structure, dependencies, related code
- **Purpose**: Boundary conditions, error propagation, hidden assumptions, race conditions
- **Prompt**: See `prompts/edge-case-hunter.md`

### Layer 3: Acceptance Auditor
- **Input**: Git diff + task spec + project-context.md (if exists)
- **Context**: Full spec and project standards
- **Purpose**: Verify every acceptance criterion is met, no spec drift
- **Prompt**: See `prompts/acceptance-auditor.md`

### Orchestration
1. Spawn all 3 agents in ONE parallel Agent call (use subagent_type "Code Reviewer")
2. Each agent returns findings in structured format
3. Merge findings: deduplicate by file:line, boost severity if multiple layers flag same issue
4. Apply zero-findings rule to merged results
5. Final verdict: SHIP if no Critical/Important findings, NEEDS_WORK otherwise

### When to use
- Always for impl_review phase
- Optional for worker Phase 6 self-review (single-layer Blind Hunter only for speed)
- Skip for --quick path (guard-only)

## Dead Code Hygiene

During review, flag:
- Commented-out code blocks (delete or create a ticket)
- Unused imports, variables, functions
- Feature flags for features that shipped months ago
- `TODO` comments older than 30 days without tickets

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "It works, so it's fine" | Working code can still have security holes, performance issues, and maintenance debt. |
| "We'll clean it up later" | Later never comes. Fix it now or create a tracked ticket. |
| "It's just a nit" | Accumulated nits become a maintenance burden. Fix the pattern, not individual instances. |
| "The tests pass" | Tests only catch what they test for. Review the untested paths. |
| "I don't have context on this area" | Say so. Ask questions. Don't rubber-stamp code you don't understand. |

## Red Flags

- No tests added for new behavior
- Error handling that swallows exceptions silently
- Magic numbers or strings without constants
- Functions with more than 4 parameters
- Deeply nested conditionals (3+ levels)
- Mixed concerns in a single commit (feature + refactor + config change)
- TODOs without ticket references
- Changes to generated files

## Verification

After completing a review:

- [ ] All five axes evaluated (correctness, readability, architecture, security, performance)
- [ ] Every finding has a severity label
- [ ] Critical/Important findings have specific fix guidance
- [ ] Clear SHIP / NEEDS_WORK / MAJOR_RETHINK verdict given
- [ ] Change size noted if >500 lines
- [ ] Tests reviewed before implementation
- [ ] No rubber-stamping — every approval reflects genuine review
