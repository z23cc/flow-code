---
name: flow-code-tdd
description: "Use when implementing with test-first methodology in Worker Phase 4, fixing bugs with Prove-It Pattern, or establishing test coverage strategy."
tier: 2
user-invocable: false
---
<!-- SKILL_TAGS: testing,tdd,quality,bugs -->

# Test-Driven Development

## Overview

Write the test first, watch it fail, make it pass, then clean up. TDD is not about testing — it's about design. Tests written first produce better APIs, clearer boundaries, and fewer bugs. The Prove-It Pattern extends TDD to bug fixes.

## When to Use

- Worker Phase 4 (when `--tdd` flag is set)
- Bug fixes (always use Prove-It Pattern)
- New public APIs or interfaces
- Complex business logic
- Any code where correctness matters more than speed

**When NOT to use:**
- Exploratory prototyping (write tests after)
- UI layout adjustments (visual testing is more appropriate)
- Configuration changes

## The TDD Cycle

```
┌───────┐    ┌───────┐    ┌──────────┐
│  RED  │───>│ GREEN │───>│ REFACTOR │──┐
│ Write │    │ Make  │    │ Clean up │  │
│ test  │    │ pass  │    │ code     │  │
└───────┘    └───────┘    └──────────┘  │
     ^                                   │
     └──────── next behavior ────────────┘
```

### RED: Write a Failing Test

- Write ONE test that describes the behavior you want
- Run it — it MUST fail (if it passes, your test is wrong or the feature exists)
- The test name should read like a spec: `test_user_with_expired_token_gets_401`

```typescript
test('rejects login with invalid email format', () => {
  const result = validateLogin({ email: 'not-an-email', password: 'valid123' });
  expect(result.success).toBe(false);
  expect(result.errors).toContain('Invalid email format');
});
```

### GREEN: Make It Pass

- Write the MINIMUM code to make the test pass
- Don't optimize. Don't refactor. Don't handle other cases yet.
- If you're writing more than ~20 lines, you're doing too much.

### REFACTOR: Clean Up

- Now that tests are green, improve the code
- Remove duplication, improve names, extract functions
- Run tests after each refactor step — they must stay green
- Don't add new behavior in this step

## The Prove-It Pattern (Bug Fixes)

For every bug fix, PROVE the bug exists before fixing it:

```
1. Write a test that demonstrates the bug
2. Run it — confirm it FAILS (proves the bug exists)
3. Fix the code
4. Run it — confirm it PASSES (proves the fix works)
5. Commit test + fix together (regression guard)
```

**Why this matters:** Without Step 1-2, you might fix the wrong thing or "fix" something that wasn't broken. The test becomes a permanent regression guard.

```typescript
// Step 1: Write the bug-proving test
test('handles negative quantities without crashing', () => {
  // This was crashing in production (ticket #123)
  expect(() => calculateTotal({ quantity: -1, price: 10 })).not.toThrow();
  expect(calculateTotal({ quantity: -1, price: 10 })).toBe(0);
});

// Step 2: Run — should fail (proves the bug)
// Step 3: Fix calculateTotal to handle negative quantities
// Step 4: Run — should pass (proves the fix)
```

## The Test Pyramid

```
        /  E2E  \          ~5%   Slow, fragile, high confidence
       /─────────\
      / Integration\       ~15%  Real dependencies, medium speed
     /──────────────\
    /     Unit       \     ~80%  Fast, isolated, focused
   /──────────────────\
```

| Level | Scope | Speed | Dependencies | When to Write |
|-------|-------|-------|-------------|---------------|
| **Unit** | Single function/class | <10ms | None (mocks OK) | Every behavior |
| **Integration** | Module boundaries | <1s | Real DB/API | Boundary interactions |
| **E2E** | User journey | <30s | Full system | Critical paths only |

### Unit Test Rules

- Test behavior, not implementation
- One concept per test
- Use descriptive names that read like specs
- Arrange → Act → Assert (AAA pattern)
- Prefer real implementations over mocks when fast enough

```typescript
// Good: tests behavior
test('expired token returns unauthorized', () => {
  const token = createToken({ expiresAt: pastDate });
  const result = validateToken(token);
  expect(result.valid).toBe(false);
  expect(result.reason).toBe('expired');
});

// Bad: tests implementation
test('calls jwt.verify with correct args', () => {
  validateToken(token);
  expect(jwt.verify).toHaveBeenCalledWith(token, SECRET);
});
```

### Integration Test Rules

- Test real database queries (not mocked)
- Test real HTTP calls between services
- Use test fixtures/factories, not production data
- Clean up after each test (transactions, truncate)

### When to Mock

```
PREFER real implementations (fast, accurate)
USE fakes for slow external services (payment APIs, email)
USE stubs for non-deterministic behavior (time, random)
AVOID mocking internal modules (couples test to implementation)
```

## Writing Good Tests

### DAMP Over DRY

Tests should be **D**escriptive **A**nd **M**eaningful **P**hrases — readability beats reuse:

```typescript
// Good: DAMP — each test is self-contained and readable
test('admin can delete any post', () => {
  const admin = createUser({ role: 'admin' });
  const post = createPost({ author: createUser() });
  expect(deletePost(admin, post.id)).resolves.toBe(true);
});

test('regular user cannot delete others posts', () => {
  const user = createUser({ role: 'user' });
  const post = createPost({ author: createUser() });
  expect(deletePost(user, post.id)).rejects.toThrow('Forbidden');
});

// Bad: DRY — shared setup obscures test intent
let admin, user, post;
beforeEach(() => { admin = createUser({role:'admin'}); /* ... */ });
```

### Test Anti-Patterns

- **The Giant Test**: Tests 10 things in one test function → split
- **The Flickering Test**: Passes sometimes, fails sometimes → find the race condition
- **The Inspector**: Tests private methods → test public behavior instead
- **The Mockery**: More mock setup than actual test → use real implementations
- **The Optimist**: Only tests happy path → add error cases
- **The Liar**: Test name doesn't match what it tests → rename

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "I'll add tests later" | Later never comes. Tests written after code test what you built, not what you should have built. |
| "This is too simple to test" | Simple code is the easiest to test. Start the habit. |
| "Tests slow me down" | Tests save 10x debugging time. The slowdown is an investment. |
| "Mocks are necessary for unit tests" | Most code can be tested with real implementations. Mocks test your assumptions, not your code. |
| "100% coverage is the goal" | Coverage measures lines executed, not correctness. Focus on behavior coverage. |

## Red Flags

- Tests that pass when the implementation is wrong
- Test files longer than implementation files (over-mocking)
- No failing test before a bug fix
- Tests that depend on execution order
- Assertions on implementation details (mock call counts)
- Commented-out tests
- Tests that access private members or internal state

## Verification

After TDD cycle:

- [ ] Every new behavior has a test that was RED before GREEN
- [ ] Every bug fix has a Prove-It test (fail → fix → pass)
- [ ] Test names read like specs (describe behavior, not implementation)
- [ ] Unit tests run in <10 seconds total
- [ ] No flickering tests (run 3 times, same result)
- [ ] Test pyramid respected (~80% unit, ~15% integration, ~5% E2E)
- [ ] No mocks of internal modules (only external boundaries)

**See also:** [Testing Patterns](../../references/testing-patterns.md) for AAA, factories, anti-patterns, and framework commands.
