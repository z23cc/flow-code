# Code Review Checklist

Quick reference for structured code review. Covers five axes: correctness, readability, architecture, security, and performance.

## Table of Contents

- [Before Reviewing](#before-reviewing)
- [Correctness](#correctness)
- [Readability](#readability)
- [Architecture](#architecture)
- [Security](#security)
- [Performance](#performance)
- [Testing](#testing)
- [Review Communication](#review-communication)
- [Common Review Anti-Patterns](#common-review-anti-patterns)

## Before Reviewing

- [ ] Understand the purpose (read PR description, linked issue, or spec)
- [ ] Check the diff size (>400 lines = request split unless justified)
- [ ] Run the code locally or verify CI passes
- [ ] Review commits in logical order (not just the final diff)

## Correctness

- [ ] Logic matches stated requirements and acceptance criteria
- [ ] Edge cases handled (empty inputs, nil/null, boundary values, overflow)
- [ ] Error paths return appropriate errors (not silently swallowed)
- [ ] Concurrent access is safe (race conditions, data races)
- [ ] State transitions are valid (no impossible states representable)
- [ ] Backward compatibility maintained (or breaking change documented)
- [ ] Database migrations are reversible and safe for rollback

## Readability

- [ ] Names are descriptive and consistent with codebase conventions
- [ ] Functions do one thing at one level of abstraction
- [ ] No dead code, commented-out code, or TODO without issue reference
- [ ] Complex logic has explanatory comments (why, not what)
- [ ] Magic numbers replaced with named constants
- [ ] Nesting depth <= 3 levels (extract functions or early-return)
- [ ] File length reasonable (<300 lines, split if larger)
- [ ] Public API has documentation (function signatures, types, constraints)

## Architecture

- [ ] Change is in the right layer (not business logic in HTTP handler)
- [ ] No unnecessary coupling between modules
- [ ] Dependency direction follows architecture (no upward dependencies)
- [ ] Abstractions are justified (no premature generalization)
- [ ] Existing patterns followed (or deviation justified in PR description)
- [ ] No duplication that should be extracted (DRY at module boundaries)
- [ ] Configuration is externalized (not hardcoded values)
- [ ] New dependencies justified and minimal

## Security

- [ ] User input validated at system boundaries
- [ ] No secrets, tokens, or credentials in code
- [ ] SQL/command injection prevented (parameterized queries, no shell interpolation)
- [ ] Authorization checked on every resource access
- [ ] Sensitive data not logged or exposed in error responses
- [ ] File paths validated (no path traversal)
- [ ] Deserialization is type-restricted (no arbitrary object creation)

## Performance

- [ ] No N+1 queries or unbounded fetches
- [ ] Algorithmic complexity appropriate for expected data size
- [ ] No unnecessary allocations in hot paths
- [ ] Database queries use indexes (check query plan for new queries)
- [ ] Caching has eviction strategy and bounded size
- [ ] No blocking I/O in async contexts
- [ ] Pagination used for list endpoints

## Testing

- [ ] New code has tests (unit and/or integration as appropriate)
- [ ] Tests cover happy path AND error/edge cases
- [ ] Tests are deterministic (no flaky time/order dependencies)
- [ ] Test names describe behavior, not implementation
- [ ] Mocks are at boundaries only (not testing internal wiring)
- [ ] Existing tests updated if behavior changed
- [ ] No test-only code paths in production code

## Review Communication

- [ ] Comments are specific and actionable ("rename X to Y" not "this is confusing")
- [ ] Distinguish blocking issues from suggestions (prefix: "nit:", "suggestion:", "blocking:")
- [ ] Ask questions before assuming intent ("Is this intentional?" not "This is wrong")
- [ ] Praise good patterns when you see them
- [ ] Limit review rounds (aim for <=2 rounds before approval)

## Common Review Anti-Patterns

| Anti-Pattern | Problem | Better Approach |
|---|---|---|
| Rubber-stamping | Bugs and debt slip through | Review every diff, even small ones |
| Style nitpicking only | Misses logic and design bugs | Use linters for style, review for logic |
| Blocking on preference | Stalls velocity without quality gain | Approve with suggestion, not block |
| Reviewing without running | Misses runtime issues | Run locally or check CI output |
| Rewriting in review | Demoralizing, scope creep | Suggest direction, let author implement |
| Ignoring test quality | Tests pass but don't verify anything | Review tests as carefully as production code |
| Delayed reviews (>24h) | Blocks author, context decays | Review within 4 hours during work hours |
| Drive-by partial review | Author thinks it's approved | Review entire PR or say what you skipped |
