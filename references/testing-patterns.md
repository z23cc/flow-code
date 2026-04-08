# Testing Patterns

Quick reference for `flow-code-tdd` and code review. Behavior-driven, pyramid-shaped.

## Test Pyramid

```
        /  E2E  \          ~5%   Full user journeys, real browser
       /─────────\
      / Integration\       ~15%  Real DB/API, module boundaries
     /──────────────\
    /     Unit       \     ~80%  Single function, fast, isolated
   /──────────────────\
```

## Arrange-Act-Assert (AAA)

```typescript
test('calculates order total with tax', () => {
  // Arrange
  const items = [{ price: 100, qty: 2 }, { price: 50, qty: 1 }];
  const taxRate = 0.1;

  // Act
  const total = calculateTotal(items, taxRate);

  // Assert
  expect(total).toBe(275); // (200 + 50) * 1.1
});
```

## Test Naming

Test names should read like specs:

```
Good:
  test('user with expired token receives 401')
  test('empty cart shows zero total')
  test('duplicate email returns validation error')

Bad:
  test('test1')
  test('it works')
  test('calculateTotal')
```

## What to Test

| Priority | Category | Example |
|----------|----------|---------|
| **Always** | Happy path | Valid input produces correct output |
| **Always** | Error path | Invalid input returns proper error |
| **Always** | Boundary | Empty list, zero, max int, null |
| **Important** | Edge cases | Unicode, very long strings, concurrent access |
| **Important** | Integration | DB queries return expected data |
| **If applicable** | E2E | User can complete checkout flow |

## Mock/Stub/Fake Decision

```
Is the dependency fast and deterministic?
  YES → Use real implementation (preferred)
  NO  → Is it an external service (payment, email, third-party API)?
    YES → Use a fake (in-memory implementation of the interface)
    NO  → Is it non-deterministic (time, random, network)?
      YES → Use a stub (return fixed values)
      NO  → Use real implementation
```

**Avoid mocking internal modules** — couples test to implementation.

## Test Anti-Patterns

| Anti-Pattern | Problem | Fix |
|---|---|---|
| **The Giant** | 50+ lines, tests 10 things | Split into focused tests |
| **The Flickerer** | Passes sometimes, fails sometimes | Find race condition or time dependency |
| **The Inspector** | Tests private methods | Test public behavior instead |
| **The Mockery** | More mock setup than actual test | Use real implementations |
| **The Optimist** | Only tests happy path | Add error and boundary cases |
| **The Liar** | Name doesn't match assertion | Rename to match actual behavior |
| **The Sleeper** | Uses `sleep()` / `setTimeout` | Use async/await or test timers |

## DAMP Over DRY

Tests should be **D**escriptive **A**nd **M**eaningful **P**hrases. Repeat setup in each test for readability:

```typescript
// Good: each test is self-contained
test('admin can delete any post', () => {
  const admin = createUser({ role: 'admin' });
  const post = createPost({ author: createUser() });
  expect(canDelete(admin, post)).toBe(true);
});

test('author can delete own post', () => {
  const author = createUser({ role: 'user' });
  const post = createPost({ author });
  expect(canDelete(author, post)).toBe(true);
});
```

## Prove-It Pattern (Bug Fixes)

```
1. Write test that demonstrates the bug → run → FAILS (proves bug exists)
2. Fix the code
3. Run test again → PASSES (proves fix works)
4. Commit test + fix together (regression guard)
```

## Test Utilities

### Factory Functions

```typescript
function createUser(overrides?: Partial<User>): User {
  return {
    id: randomId(),
    name: 'Test User',
    email: 'test@example.com',
    role: 'user',
    ...overrides,
  };
}
```

### Custom Matchers

```typescript
expect.extend({
  toBeWithinRange(received, floor, ceiling) {
    const pass = received >= floor && received <= ceiling;
    return { pass, message: () => `expected ${received} to be within [${floor}, ${ceiling}]` };
  },
});
```

## Framework-Specific

### Jest / Vitest
```bash
npx vitest run              # Run all tests
npx vitest run --coverage   # With coverage
npx vitest watch            # Watch mode
```

### pytest
```bash
pytest                      # Run all tests
pytest --cov=src            # With coverage
pytest -x                   # Stop on first failure
```

### Cargo test
```bash
cargo test                  # Run all tests
cargo test -- --nocapture   # Show stdout
cargo test test_name        # Run specific test
```
