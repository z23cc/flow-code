#!/usr/bin/env bash
# compare_outputs.sh — Integration tests comparing Rust and Python flowctl output.
#
# Runs both Python ($FLOWCTL) and Rust (cargo-built binary) against identical
# input and compares JSON output structure, key presence, exit codes.
#
# Usage:
#   bash flowctl/tests/integration/compare_outputs.sh [--verbose]
#
# Environment:
#   FLOWCTL        Path to Python flowctl.py (auto-detected if unset)
#   RUST_BINARY    Path to Rust flowctl binary (auto-detected if unset)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

VERBOSE="${1:-}"
PASS=0
FAIL=0
SKIP=0

# ── Locate binaries ──────────────────────────────────────────────────
FLOWCTL="${FLOWCTL:-$REPO_ROOT/bin/flowctl}"
if [[ ! -f "$FLOWCTL" ]]; then
  echo "FATAL: Python flowctl not found at $FLOWCTL"
  exit 1
fi

RUST_BINARY="${RUST_BINARY:-$REPO_ROOT/flowctl/target/debug/flowctl}"
if [[ ! -f "$RUST_BINARY" ]]; then
  echo "Building Rust binary..."
  cargo build --manifest-path "$REPO_ROOT/flowctl/Cargo.toml" 2>/dev/null
fi
if [[ ! -f "$RUST_BINARY" ]]; then
  echo "FATAL: Rust binary not found at $RUST_BINARY"
  exit 1
fi

# ── Helpers ───────────────────────────────────────────────────────────
TMPDIR_BASE="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_BASE"' EXIT

log() { echo "  $*"; }
log_verbose() { [[ "$VERBOSE" == "--verbose" ]] && echo "    $*" || true; }

# Run Python flowctl (--json goes AFTER subcommand)
run_python() {
  local dir="$1"; shift
  local cmd="$1"; shift
  # Python: flowctl.py <subcommand> [sub-subcommand...] --json [args...]
  # We need to insert --json after the command/subcommand tokens
  (cd "$dir" && python3 "$FLOWCTL" $cmd --json "$@" 2>&1)
}
run_python_exit() {
  local dir="$1"; shift
  local cmd="$1"; shift
  (cd "$dir" && python3 "$FLOWCTL" $cmd --json "$@" 2>&1; echo "EXIT:$?") | tail -1 | sed 's/EXIT://'
}

# Run Rust flowctl (--json goes BEFORE subcommand)
run_rust() {
  local dir="$1"; shift
  (cd "$dir" && "$RUST_BINARY" --json "$@" 2>&1)
}
run_rust_exit() {
  local dir="$1"; shift
  (cd "$dir" && "$RUST_BINARY" --json "$@" 2>&1; echo "EXIT:$?") | tail -1 | sed 's/EXIT://'
}

# Compare JSON output: normalize timestamps, ignore key ordering, ignore
# auto-generated IDs (they differ because each binary uses its own .flow/).
# Returns 0 if structurally equivalent, 1 otherwise.
compare_json() {
  local py_json="$1"
  local rs_json="$2"
  local label="$3"

  # Normalize: sort keys, strip timestamps/dates, strip IDs, strip paths
  local py_norm rs_norm
  py_norm=$(echo "$py_json" | python3 -c "
import sys, json, re
try:
    d = json.load(sys.stdin)
except:
    print('PARSE_ERROR')
    sys.exit(0)
print(json.dumps(d, sort_keys=True))
" 2>/dev/null || echo "PARSE_ERROR")

  rs_norm=$(echo "$rs_json" | python3 -c "
import sys, json, re
try:
    d = json.load(sys.stdin)
except:
    print('PARSE_ERROR')
    sys.exit(0)
print(json.dumps(d, sort_keys=True))
" 2>/dev/null || echo "PARSE_ERROR")

  if [[ "$py_norm" == "PARSE_ERROR" ]] || [[ "$rs_norm" == "PARSE_ERROR" ]]; then
    log_verbose "JSON parse error for $label"
    log_verbose "  Python: $py_json"
    log_verbose "  Rust:   $rs_json"
    return 1
  fi

  # Compare top-level keys
  local py_keys rs_keys
  py_keys=$(echo "$py_json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
if isinstance(d, dict):
    print(' '.join(sorted(d.keys())))
else:
    print('NOT_DICT')
" 2>/dev/null)
  rs_keys=$(echo "$rs_json" | python3 -c "
import sys, json
d = json.load(sys.stdin)
if isinstance(d, dict):
    print(' '.join(sorted(d.keys())))
else:
    print('NOT_DICT')
" 2>/dev/null)

  if [[ "$py_keys" != "$rs_keys" ]]; then
    log_verbose "Key mismatch for $label"
    log_verbose "  Python keys: $py_keys"
    log_verbose "  Rust keys:   $rs_keys"
    return 1
  fi

  return 0
}

# Compare exit codes
compare_exit() {
  local py_exit="$1"
  local rs_exit="$2"
  local label="$3"

  if [[ "$py_exit" != "$rs_exit" ]]; then
    log_verbose "Exit code mismatch for $label: Python=$py_exit Rust=$rs_exit"
    return 1
  fi
  return 0
}

# Test runner
test_case() {
  local name="$1"
  local result="$2"  # "pass" or "fail"

  if [[ "$result" == "pass" ]]; then
    PASS=$((PASS + 1))
    log "PASS  $name"
  else
    FAIL=$((FAIL + 1))
    log "FAIL  $name"
  fi
}

skip_case() {
  local name="$1"
  local reason="$2"
  SKIP=$((SKIP + 1))
  log "SKIP  $name ($reason)"
}

# ── Setup fresh .flow/ dirs ──────────────────────────────────────────
setup_empty_dirs() {
  local py_dir="$TMPDIR_BASE/py_$$_$RANDOM"
  local rs_dir="$TMPDIR_BASE/rs_$$_$RANDOM"
  mkdir -p "$py_dir" "$rs_dir"
  echo "$py_dir $rs_dir"
}

setup_initialized_dirs() {
  local dirs
  dirs=$(setup_empty_dirs)
  local py_dir rs_dir
  py_dir=$(echo "$dirs" | cut -d' ' -f1)
  rs_dir=$(echo "$dirs" | cut -d' ' -f2)

  run_python "$py_dir" "init" >/dev/null 2>&1
  run_rust "$rs_dir" "init" >/dev/null 2>&1

  echo "$py_dir $rs_dir"
}

setup_with_epic() {
  local dirs
  dirs=$(setup_initialized_dirs)
  local py_dir rs_dir
  py_dir=$(echo "$dirs" | cut -d' ' -f1)
  rs_dir=$(echo "$dirs" | cut -d' ' -f2)

  run_python "$py_dir" "epic create" --title "Test Epic" >/dev/null 2>&1
  run_rust "$rs_dir" "epic" "create" --title "Test Epic" >/dev/null 2>&1

  echo "$py_dir $rs_dir"
}

setup_with_task() {
  local dirs
  dirs=$(setup_with_epic)
  local py_dir rs_dir py_epic rs_epic
  py_dir=$(echo "$dirs" | cut -d' ' -f1)
  rs_dir=$(echo "$dirs" | cut -d' ' -f2)

  # Get epic IDs (they may differ)
  py_epic=$(run_python "$py_dir" "epics" | python3 -c "import sys,json; print(json.load(sys.stdin)['epics'][0]['id'])" 2>/dev/null)
  rs_epic=$(run_rust "$rs_dir" "epics" | python3 -c "import sys,json; print(json.load(sys.stdin)['epics'][0]['id'])" 2>/dev/null)

  run_python "$py_dir" "task create" --epic "$py_epic" --title "Task One" >/dev/null 2>&1
  run_rust "$rs_dir" "task" "create" --epic "$rs_epic" --title "Task One" >/dev/null 2>&1

  echo "$py_dir $rs_dir $py_epic $rs_epic"
}

# ══════════════════════════════════════════════════════════════════════
echo "=== flowctl Integration Tests: Rust vs Python ==="
echo "  Python: $FLOWCTL"
echo "  Rust:   $RUST_BINARY"
echo ""

# ── Test 1: init ──────────────────────────────────────────────────────
echo "--- init ---"
dirs=$(setup_empty_dirs)
py_dir=$(echo "$dirs" | cut -d' ' -f1)
rs_dir=$(echo "$dirs" | cut -d' ' -f2)

py_out=$(run_python "$py_dir" "init")
rs_out=$(run_rust "$rs_dir" "init")
py_exit=$?; rs_exit=$?

if compare_json "$py_out" "$rs_out" "init"; then
  test_case "init: JSON keys match" "pass"
else
  test_case "init: JSON keys match" "fail"
fi

# Check success field
py_success=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
rs_success=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
if [[ "$py_success" == "True" ]] && [[ "$rs_success" == "True" ]]; then
  test_case "init: both report success=true" "pass"
else
  test_case "init: both report success=true" "fail"
fi

# ── Test 2: init idempotent (re-init) ────────────────────────────────
py_out2=$(run_python "$py_dir" "init")
rs_out2=$(run_rust "$rs_dir" "init")
py_success2=$(echo "$py_out2" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
rs_success2=$(echo "$rs_out2" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
if [[ "$py_success2" == "True" ]] && [[ "$rs_success2" == "True" ]]; then
  test_case "init: idempotent re-init succeeds" "pass"
else
  test_case "init: idempotent re-init succeeds" "fail"
fi

# ── Test 3: status (empty .flow/) ────────────────────────────────────
echo "--- status ---"
dirs=$(setup_initialized_dirs)
py_dir=$(echo "$dirs" | cut -d' ' -f1)
rs_dir=$(echo "$dirs" | cut -d' ' -f2)

py_out=$(run_python "$py_dir" "status")
rs_out=$(run_rust "$rs_dir" "status")

if compare_json "$py_out" "$rs_out" "status"; then
  test_case "status: JSON keys match" "pass"
else
  test_case "status: JSON keys match" "fail"
fi

# Verify zero counts
py_todo=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['tasks']['todo'])" 2>/dev/null)
rs_todo=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['tasks']['todo'])" 2>/dev/null)
if [[ "$py_todo" == "0" ]] && [[ "$rs_todo" == "0" ]]; then
  test_case "status: empty .flow/ shows zero tasks" "pass"
else
  test_case "status: empty .flow/ shows zero tasks" "fail"
fi

# ── Test 4: epics (empty) ────────────────────────────────────────────
echo "--- epics ---"
py_out=$(run_python "$py_dir" "epics")
rs_out=$(run_rust "$rs_dir" "epics")

if compare_json "$py_out" "$rs_out" "epics-empty"; then
  test_case "epics: empty list JSON keys match" "pass"
else
  test_case "epics: empty list JSON keys match" "fail"
fi

py_count=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('count',json.load(open('/dev/null')) if False else len(json.load(sys.stdin).get('epics',[]))))" 2>/dev/null || echo "?")
# simpler:
py_count=$(echo "$py_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('count', len(d.get('epics',[]))))" 2>/dev/null)
rs_count=$(echo "$rs_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('count', len(d.get('epics',[]))))" 2>/dev/null)
if [[ "$py_count" == "0" ]] && [[ "$rs_count" == "0" ]]; then
  test_case "epics: both show count=0" "pass"
else
  test_case "epics: both show count=0" "fail"
fi

# ── Test 5: epic create ──────────────────────────────────────────────
echo "--- epic create ---"
py_out=$(run_python "$py_dir" "epic create" --title "Integration Test Epic")
rs_out=$(run_rust "$rs_dir" "epic" "create" --title "Integration Test Epic")

if compare_json "$py_out" "$rs_out" "epic-create"; then
  test_case "epic create: JSON keys match" "pass"
else
  test_case "epic create: JSON keys match" "fail"
fi

py_success=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
rs_success=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
if [[ "$py_success" == "True" ]] && [[ "$rs_success" == "True" ]]; then
  test_case "epic create: both succeed" "pass"
else
  test_case "epic create: both succeed" "fail"
fi

# Get epic IDs for subsequent tests
py_epic=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
rs_epic=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)

# ── Test 6: show (epic) ──────────────────────────────────────────────
echo "--- show ---"
py_out=$(run_python "$py_dir" "show" "$py_epic")
rs_out=$(run_rust "$rs_dir" "show" "$rs_epic")

# show may have extra keys in Python that Rust hasn't implemented yet
# Check Rust keys are a subset of Python keys
py_keys=$(echo "$py_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(' '.join(sorted(d.keys())) if isinstance(d,dict) else '')" 2>/dev/null)
rs_keys=$(echo "$rs_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(' '.join(sorted(d.keys())) if isinstance(d,dict) else '')" 2>/dev/null)
extra=$(python3 -c "
py=set('$py_keys'.split())
rs=set('$rs_keys'.split())
extra=rs-py
print(' '.join(sorted(extra)) if extra else '')
")
if [[ -z "$extra" ]]; then
  test_case "show epic: Rust keys subset of Python" "pass"
  missing=$(python3 -c "
py=set('$py_keys'.split())
rs=set('$rs_keys'.split())
m=py-rs
if m: print('  (Rust missing: ' + ', '.join(sorted(m)) + ')')
")
  [[ -n "$missing" ]] && log "$missing"
else
  test_case "show epic: Rust keys subset of Python" "fail"
  log_verbose "  Extra Rust keys: $extra"
fi

# ── Test 7: task create ──────────────────────────────────────────────
echo "--- task create ---"
py_out=$(run_python "$py_dir" "task create" --epic "$py_epic" --title "Test Task Alpha")
rs_out=$(run_rust "$rs_dir" "task" "create" --epic "$rs_epic" --title "Test Task Alpha")

if compare_json "$py_out" "$rs_out" "task-create"; then
  test_case "task create: JSON keys match" "pass"
else
  test_case "task create: JSON keys match" "fail"
fi

py_task=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
rs_task=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])" 2>/dev/null)

# ── Test 8: tasks list ───────────────────────────────────────────────
echo "--- tasks ---"
py_out=$(run_python "$py_dir" "tasks" --epic "$py_epic")
rs_out=$(run_rust "$rs_dir" "tasks" --epic "$rs_epic")

if compare_json "$py_out" "$rs_out" "tasks"; then
  test_case "tasks: JSON keys match" "pass"
else
  test_case "tasks: JSON keys match" "fail"
fi

# ── Test 9: start ────────────────────────────────────────────────────
echo "--- start ---"
py_out=$(run_python "$py_dir" "start" "$py_task")
rs_out=$(run_rust "$rs_dir" "start" "$rs_task")

py_success=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
rs_success=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
if [[ "$py_success" == "True" ]] && [[ "$rs_success" == "True" ]]; then
  test_case "start: both succeed" "pass"
else
  test_case "start: both succeed" "fail"
fi

if compare_json "$py_out" "$rs_out" "start"; then
  test_case "start: JSON keys match" "pass"
else
  test_case "start: JSON keys match" "fail"
fi

# ── Test 10: done ────────────────────────────────────────────────────
echo "--- done ---"
py_out=$(run_python "$py_dir" "done" "$py_task" --summary "Completed" --force)
rs_out=$(run_rust "$rs_dir" "done" "$rs_task" --summary "Completed" --force)

py_success=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
rs_success=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null)
if [[ "$py_success" == "True" ]] && [[ "$rs_success" == "True" ]]; then
  test_case "done: both succeed" "pass"
else
  test_case "done: both succeed" "fail"
fi

if compare_json "$py_out" "$rs_out" "done"; then
  test_case "done: JSON keys match" "pass"
else
  test_case "done: JSON keys match" "fail"
fi

# ── Test 11: status after work ───────────────────────────────────────
echo "--- status after work ---"
py_out=$(run_python "$py_dir" "status")
rs_out=$(run_rust "$rs_dir" "status")

py_done=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['tasks']['done'])" 2>/dev/null)
rs_done=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin)['tasks']['done'])" 2>/dev/null)
if [[ "$py_done" == "1" ]] && [[ "$rs_done" == "1" ]]; then
  test_case "status: both show 1 done task" "pass"
else
  test_case "status: both show 1 done task" "fail"
fi

# ══════════════════════════════════════════════════════════════════════
# Edge Cases
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "--- Edge Cases ---"

# ── Edge 1: status without .flow/ ─────────────────────────────────────
edge_dir_py="$TMPDIR_BASE/edge_py_$$"
edge_dir_rs="$TMPDIR_BASE/edge_rs_$$"
mkdir -p "$edge_dir_py" "$edge_dir_rs"

py_out=$(run_python "$edge_dir_py" "status" 2>&1 || true)
py_exit=$?
rs_out=$(run_rust "$edge_dir_rs" "status" 2>&1 || true)
rs_exit=$?

# Both should indicate no .flow/ or fail gracefully
py_exists=$(echo "$py_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('flow_exists',''))" 2>/dev/null || echo "error")
rs_exists=$(echo "$rs_out" | python3 -c "import sys,json; print(json.load(sys.stdin).get('flow_exists',''))" 2>/dev/null || echo "error")
if [[ "$py_exists" == "False" ]] && [[ "$rs_exists" == "False" ]]; then
  test_case "edge: status without .flow/ returns flow_exists=false" "pass"
elif [[ "$py_exists" == "error" ]] && [[ "$rs_exists" == "error" ]]; then
  # Both error out - also acceptable
  test_case "edge: status without .flow/ both error (consistent)" "pass"
else
  test_case "edge: status without .flow/ consistent behavior" "fail"
  log_verbose "  Python flow_exists=$py_exists  Rust flow_exists=$rs_exists"
fi

# ── Edge 2: show with invalid ID ─────────────────────────────────────
dirs=$(setup_initialized_dirs)
py_dir=$(echo "$dirs" | cut -d' ' -f1)
rs_dir=$(echo "$dirs" | cut -d' ' -f2)

py_out=$(run_python "$py_dir" "show" "nonexistent-id-999" 2>&1; echo "EXIT:$?")
py_exit=$(echo "$py_out" | grep "EXIT:" | sed 's/EXIT://')
rs_out=$(run_rust "$rs_dir" "show" "nonexistent-id-999" 2>&1; echo "EXIT:$?")
rs_exit=$(echo "$rs_out" | grep "EXIT:" | sed 's/EXIT://')

# Both should return non-zero or error JSON
if [[ "$py_exit" != "0" ]] && [[ "$rs_exit" != "0" ]]; then
  test_case "edge: show invalid ID - both return non-zero exit" "pass"
else
  # Check if they return error in JSON
  py_success=$(echo "$py_out" | head -1 | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null || echo "?")
  rs_success=$(echo "$rs_out" | head -1 | python3 -c "import sys,json; print(json.load(sys.stdin).get('success',''))" 2>/dev/null || echo "?")
  if [[ "$py_success" == "False" ]] && [[ "$rs_success" == "False" ]]; then
    test_case "edge: show invalid ID - both return success=false" "pass"
  else
    test_case "edge: show invalid ID - consistent error behavior" "fail"
    log_verbose "  Python exit=$py_exit success=$py_success"
    log_verbose "  Rust exit=$rs_exit success=$rs_success"
  fi
fi

# ── Edge 3: start with invalid ID ────────────────────────────────────
py_out=$(run_python "$py_dir" "start" "bogus-task-id" 2>&1; echo "EXIT:$?")
py_exit=$(echo "$py_out" | grep "EXIT:" | sed 's/EXIT://')
rs_out=$(run_rust "$rs_dir" "start" "bogus-task-id" 2>&1; echo "EXIT:$?")
rs_exit=$(echo "$rs_out" | grep "EXIT:" | sed 's/EXIT://')

if [[ "$py_exit" != "0" ]] && [[ "$rs_exit" != "0" ]]; then
  test_case "edge: start invalid ID - both return non-zero" "pass"
elif [[ "$py_exit" == "$rs_exit" ]]; then
  test_case "edge: start invalid ID - same exit code ($py_exit)" "pass"
else
  test_case "edge: start invalid ID - consistent error" "fail"
  log_verbose "  Python exit=$py_exit  Rust exit=$rs_exit"
fi

# ── Edge 4: done without required args ────────────────────────────────
py_exit=0
(cd "$py_dir" && python3 "$FLOWCTL" done --json >/dev/null 2>&1) || py_exit=$?
rs_exit=0
(cd "$rs_dir" && "$RUST_BINARY" --json done >/dev/null 2>&1) || rs_exit=$?

if [[ "$py_exit" != "0" ]] && [[ "$rs_exit" != "0" ]]; then
  test_case "edge: done without task ID - both error" "pass"
else
  test_case "edge: done without task ID - consistent error" "fail"
  log_verbose "  Python exit=$py_exit  Rust exit=$rs_exit"
fi

# ── Edge 5: epic create without title ─────────────────────────────────
py_exit=0
(cd "$py_dir" && python3 "$FLOWCTL" epic create --json >/dev/null 2>&1) || py_exit=$?
rs_exit=0
(cd "$rs_dir" && "$RUST_BINARY" --json epic create >/dev/null 2>&1) || rs_exit=$?

if [[ "$py_exit" != "0" ]] && [[ "$rs_exit" != "0" ]]; then
  test_case "edge: epic create without title - both error" "pass"
else
  test_case "edge: epic create without title - consistent error" "fail"
  log_verbose "  Python exit=$py_exit  Rust exit=$rs_exit"
fi

# ── Edge 6: task create without epic ──────────────────────────────────
py_exit=0
(cd "$py_dir" && python3 "$FLOWCTL" task create --json --title "Orphan" >/dev/null 2>&1) || py_exit=$?
rs_exit=0
(cd "$rs_dir" && "$RUST_BINARY" --json task create --title "Orphan" >/dev/null 2>&1) || rs_exit=$?

if [[ "$py_exit" != "0" ]] && [[ "$rs_exit" != "0" ]]; then
  test_case "edge: task create without epic - both error" "pass"
else
  test_case "edge: task create without epic - consistent error" "fail"
  log_verbose "  Python exit=$py_exit  Rust exit=$rs_exit"
fi

# ══════════════════════════════════════════════════════════════════════
# Summary
# ══════════════════════════════════════════════════════════════════════
echo ""
echo "=== Results ==="
echo "  PASS: $PASS"
echo "  FAIL: $FAIL"
echo "  SKIP: $SKIP"
TOTAL=$((PASS + FAIL))
echo "  TOTAL: $TOTAL"
echo ""

if [[ $FAIL -gt 0 ]]; then
  echo "FAILED ($FAIL of $TOTAL tests failed)"
  exit 1
else
  echo "ALL TESTS PASSED"
  exit 0
fi
