# Testing Patterns Reference

Quick reference for testing patterns in flow-code. Rust backend, Bash scripts, Markdown skills.

## Test Pyramid

| Layer | Coverage | What to Test | Tools |
|-------|----------|--------------|-------|
| Unit | ~80% | Pure functions, logic, parsing, serialization | `cargo test`, `#[test]` |
| Integration | ~15% | DB queries, CLI commands, cross-crate calls | `cargo test --test`, temp dirs |
| E2E | ~5% | Full workflows (smoke tests, ralph loops) | `bash scripts/smoke_test.sh` |

## Test Structure (Arrange-Act-Assert)

```rust
#[test]
fn creates_task_with_default_status() {
    // Arrange: set up test data
    let input = TaskInput { title: "Test".into(), ..Default::default() };

    // Act: perform the action
    let result = create_task(input);

    // Assert: verify outcome
    assert_eq!(result.status, Status::Todo);
    assert!(!result.id.is_empty());
}
```

## Rust Test Patterns

| Pattern | Usage | Example |
|---------|-------|---------|
| `#[test]` | Basic unit test | `fn test_parse() { assert_eq!(parse("x"), Ok("x")); }` |
| `#[should_panic]` | Expected panic | `#[should_panic(expected = "empty")] fn test_empty()` |
| `#[tokio::test]` | Async test | `async fn test_db_query() { ... }` |
| `assert_eq!` | Equality check | `assert_eq!(actual, expected)` |
| `assert!` | Boolean check | `assert!(result.is_ok())` |
| `assert_ne!` | Inequality check | `assert_ne!(id1, id2)` |
| Test modules | Organize tests | `#[cfg(test)] mod tests { use super::*; }` |
| `#[ignore]` | Skip slow tests | Run with `cargo test -- --ignored` |

## Running Tests

```bash
# All tests
cd flowctl && cargo test --all

# Specific crate
cargo test -p flowctl-core
cargo test -p flowctl-db
cargo test -p flowctl-cli

# Single test by name
cargo test test_create_task

# With output
cargo test -- --nocapture

# Bash smoke/e2e tests
bash scripts/smoke_test.sh
bash scripts/ci_test.sh
bash scripts/teams_e2e_test.sh
```

## Mock Boundaries

| Mock These | Don't Mock These |
|------------|-----------------|
| Database calls (libSQL) | Internal utility functions |
| HTTP/network requests | Business logic |
| File system operations | Data transformations |
| External API calls | Validation functions |
| Time/Date (`SystemTime`) | Pure functions |
| Environment variables | Parsing/serialization |

## Prove-It Pattern (Bug Fixes)

| Step | Action | Gate |
|------|--------|------|
| 1. Red | Write a test that reproduces the bug | Test MUST fail |
| 2. Confirm | Run test suite, verify only new test fails | No other regressions |
| 3. Fix | Implement the minimal fix | Keep scope tight |
| 4. Green | Run test suite, verify all tests pass | New test now passes |
| 5. Commit | Include test + fix in same commit | Evidence-based fix |

## Test Naming Conventions

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Pattern: action_condition_expected_result
    #[test]
    fn parse_valid_input_returns_ok() { ... }

    #[test]
    fn parse_empty_string_returns_error() { ... }

    #[test]
    fn create_task_assigns_unique_id() { ... }
}
```

## Integration Test Structure

```rust
// tests/integration_test.rs (separate file in tests/ directory)
use tempfile::TempDir;

#[tokio::test]
async fn full_task_lifecycle() {
    // Setup: temp directory for .flow/ state
    let tmp = TempDir::new().unwrap();
    let db = Database::open(tmp.path()).await.unwrap();

    // Create
    let task = db.create_task("Test task").await.unwrap();
    assert_eq!(task.status, "todo");

    // Transition
    db.start_task(&task.id).await.unwrap();
    let updated = db.get_task(&task.id).await.unwrap();
    assert_eq!(updated.status, "in_progress");

    // Complete
    db.complete_task(&task.id, "Done").await.unwrap();
    let done = db.get_task(&task.id).await.unwrap();
    assert_eq!(done.status, "done");
}
```

## Bash Test Patterns

```bash
# Temp directory isolation
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Assert command succeeds
$FLOWCTL create-epic "test" --json || { echo "FAIL: create-epic"; exit 1; }

# Assert output contains expected string
OUTPUT=$($FLOWCTL show fn-1 --json)
echo "$OUTPUT" | grep -q '"status":"todo"' || { echo "FAIL: status"; exit 1; }

# Assert command fails (expected)
if $FLOWCTL done nonexistent 2>/dev/null; then
    echo "FAIL: should have failed"; exit 1
fi
```

## Test Anti-Patterns

| Anti-Pattern | Problem | Better Approach |
|---|---|---|
| Testing implementation details | Breaks on refactor | Test inputs/outputs only |
| Shared mutable state between tests | Tests pollute each other | Use temp dirs, fresh DB per test |
| No assertions (test runs = passes) | False confidence | Every test needs `assert!` |
| Hardcoded paths in tests | Breaks on other machines | Use `TempDir`, relative paths |
| Skipping tests to pass CI | Hides real bugs | Fix or delete the test |
| Testing third-party code | Wastes time | Mock the boundary |
| Overly broad assertions (`is_ok()`) | Doesn't catch regressions | Assert specific values |
| Giant test functions (50+ lines) | Hard to diagnose failures | One assertion per concept |
| Test depends on execution order | Flaky in parallel | Each test self-contained |
| Sleeping for async (`thread::sleep`) | Slow and flaky | Use proper async awaiting |
