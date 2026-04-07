# Testing Patterns Reference

Quick reference for testing patterns across stacks. Stack-agnostic — examples in Rust, Python, Go, TypeScript.

## Table of Contents

- [Test Structure (Arrange-Act-Assert)](#test-structure-arrange-act-assert)
- [Test Naming Conventions](#test-naming-conventions)
- [Mocking Patterns](#mocking-patterns)
- [Property-Based Testing](#property-based-testing)
- [Integration Testing](#integration-testing)
- [Test Isolation](#test-isolation)
- [Test Anti-Patterns](#test-anti-patterns)

## Test Structure (Arrange-Act-Assert)

Every test follows the same three-phase pattern regardless of language:

```
// Arrange: Set up test data and preconditions
// Act:     Perform the action being tested
// Assert:  Verify the outcome
```

- [ ] Each test has exactly one reason to fail
- [ ] Test name describes expected behavior, not implementation
- [ ] Arrange phase uses factory functions, not inline construction
- [ ] Assert phase checks outcomes, not intermediate state

## Test Naming Conventions

```
Pattern: [unit]_[expected_behavior]_[condition]

Rust:    #[test] fn parse_config_returns_error_when_file_missing()
Python:  def test_parse_config_returns_error_when_file_missing():
Go:      func TestParseConfig_ReturnsError_WhenFileMissing(t *testing.T)
TS:      it('returns error when config file is missing', () => {})
```

- [ ] Names read as sentences describing behavior
- [ ] No `test1`, `test_thing`, or `it_works` names
- [ ] Failure message makes the bug obvious without reading test code

## Mocking Patterns

### Mock at Boundaries Only

```
Mock these:                    Don't mock these:
+-- Database calls             +-- Internal utility functions
+-- HTTP/gRPC clients          +-- Business logic
+-- File system operations     +-- Data transformations
+-- External API calls         +-- Validation functions
+-- Clock/time (when needed)   +-- Pure functions
```

- [ ] Mocks verify interactions at system boundaries, not internal calls
- [ ] Each mock has explicit expectations (not just "was called")
- [ ] Mock setup is extracted into helpers when reused across tests
- [ ] Prefer fakes (in-memory implementations) over mocks for complex interfaces

### Language-Specific Patterns

| Language | Mock Library | Injection Pattern |
|----------|-------------|-------------------|
| Rust | `mockall`, test modules | Trait objects, generics |
| Python | `unittest.mock`, `pytest-mock` | Dependency injection, `monkeypatch` |
| Go | `gomock`, `testify/mock` | Interface parameters |
| TypeScript | `jest.fn()`, `vitest` | Constructor injection, module mocks |

## Property-Based Testing

Use when: inputs have wide ranges, edge cases are non-obvious, or round-trip invariants exist.

- [ ] Encode invariants as properties (e.g., `serialize(deserialize(x)) == x`)
- [ ] Test commutativity, associativity, idempotency where applicable
- [ ] Use shrinking to find minimal failing cases
- [ ] Combine with example-based tests for known edge cases

| Language | Library |
|----------|---------|
| Rust | `proptest`, `quickcheck` |
| Python | `hypothesis` |
| Go | `gopter`, `rapid` |
| TypeScript | `fast-check` |

## Integration Testing

- [ ] Tests use real dependencies (database, filesystem) where practical
- [ ] Each test creates and tears down its own state (no shared fixtures)
- [ ] Tests run in parallel-safe isolation (unique DB schemas, temp dirs)
- [ ] Network calls to external services are recorded/replayed (VCR pattern)
- [ ] CI runs integration tests in a separate stage from unit tests

### Database Testing Patterns

- [ ] Use transactions that roll back after each test (fastest isolation)
- [ ] Or: create per-test schemas/databases and drop after
- [ ] Seed minimal data per test, not shared global fixtures
- [ ] Assert on query results, not SQL strings

## Test Isolation

- [ ] No test depends on another test's execution or ordering
- [ ] No shared mutable state between tests (global variables, singletons)
- [ ] Temp files use unique directories per test, cleaned up after
- [ ] Environment variables restored after tests that modify them
- [ ] Parallel execution is the default; serial only when justified

## Test Anti-Patterns

| Anti-Pattern | Problem | Better Approach |
|---|---|---|
| Testing implementation details | Breaks on refactor | Test inputs/outputs only |
| Snapshot everything | No one reviews snapshot diffs | Assert specific values |
| Shared mutable state | Tests pollute each other | Setup/teardown per test |
| Testing third-party code | Wastes time, not your bug | Mock the boundary |
| `skip`/`ignore` permanently | Dead code hiding bugs | Remove or fix the test |
| Overly broad assertions | Doesn't catch regressions | Be specific about expectations |
| No async error handling | Swallowed errors, false passes | Always await async tests |
| Flaky time-dependent tests | Random CI failures | Use fake clocks, not `sleep` |
| Giant test fixtures | Hard to understand failures | Minimal data per test |
| Testing private internals | Couples tests to structure | Test through public API |
