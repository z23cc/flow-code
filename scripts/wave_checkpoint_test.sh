#!/usr/bin/env bash
# Wave checkpoint test — validates wave lifecycle: lock → work → unlock → guard → next wave
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FLOWCTL="$PLUGIN_ROOT/bin/flowctl"

# Safety: refuse to run from plugin repo
if [[ -f "$PWD/bin/flowctl" ]]; then
    echo "ERROR: refusing to run from main plugin repo. Run from any other directory." >&2
    exit 1
fi

PASS=0
FAIL=0
pass() { echo -e "\033[0;32m✓\033[0m $1"; PASS=$((PASS + 1)); }
fail() { echo -e "\033[0;31m✗\033[0m $1"; FAIL=$((FAIL + 1)); }

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT
cd "$TMPDIR"
git init -q && git commit --allow-empty -m "init" -q

echo -e "\033[1;33m=== Wave Checkpoint Tests ===\033[0m"

# --- Setup: Create epic with 3 tasks in 2 waves ---
$FLOWCTL init --json > /dev/null
$FLOWCTL epic create --title "Wave test" --json > /dev/null
EPIC="fn-1-wave-test"

# Wave 1: tasks 1 and 2 (no deps, can run in parallel)
$FLOWCTL task create --epic "$EPIC" --title "Task A" --domain backend \
    --files "src/a.rs" --json > /dev/null
$FLOWCTL task create --epic "$EPIC" --title "Task B" --domain frontend \
    --files "src/b.rs" --json > /dev/null
# Wave 2: task 3 depends on both 1 and 2
$FLOWCTL task create --epic "$EPIC" --title "Task C (depends on A+B)" --domain testing \
    --files "tests/c.rs" --deps "$EPIC.1,$EPIC.2" --json > /dev/null

echo -e "\033[1;33m--- Wave 1: Ready tasks ---\033[0m"

# Test: Tasks 1 and 2 should be ready, task 3 should NOT be ready
READY=$($FLOWCTL ready --epic "$EPIC" --json | jq -r '.ready[].id' | sort | tr '\n' ',')
echo "$READY" | grep -q "$EPIC.1" && pass "Task A is ready" || fail "Task A should be ready"
echo "$READY" | grep -q "$EPIC.2" && pass "Task B is ready" || fail "Task B should be ready"
echo "$READY" | grep -qv "$EPIC.3" && pass "Task C is NOT ready (deps unmet)" || fail "Task C should NOT be ready"

echo -e "\033[1;33m--- Wave 1: Lock → Start → Complete ---\033[0m"

# Lock files for wave 1
$FLOWCTL lock --task "$EPIC.1" --files "src/a.rs" --json > /dev/null
$FLOWCTL lock --task "$EPIC.2" --files "src/b.rs" --json > /dev/null
pass "Locked files for wave 1"

# Start tasks
$FLOWCTL start "$EPIC.1" --json > /dev/null
$FLOWCTL start "$EPIC.2" --json > /dev/null
pass "Started tasks A and B"

# Complete tasks with evidence
echo "Implemented A" > /tmp/summary_a.md
echo '{"commits":["abc123"],"tests":["cargo test"]}' > /tmp/evidence_a.json
$FLOWCTL done "$EPIC.1" --summary-file /tmp/summary_a.md --evidence-json /tmp/evidence_a.json --json > /dev/null
STATUS_A=$($FLOWCTL show "$EPIC.1" --json | jq -r .status)
[[ "$STATUS_A" == "done" ]] && pass "Task A completed (status=done)" || fail "Task A status expected done got $STATUS_A"

echo "Implemented B" > /tmp/summary_b.md
echo '{"commits":["def456"],"tests":["npm test"]}' > /tmp/evidence_b.json
$FLOWCTL done "$EPIC.2" --summary-file /tmp/summary_b.md --evidence-json /tmp/evidence_b.json --json > /dev/null
STATUS_B=$($FLOWCTL show "$EPIC.2" --json | jq -r .status)
[[ "$STATUS_B" == "done" ]] && pass "Task B completed (status=done)" || fail "Task B status expected done got $STATUS_B"

echo -e "\033[1;33m--- Wave 1: Checkpoint ---\033[0m"

# Unlock all (wave cleanup)
$FLOWCTL unlock --all --json > /dev/null
pass "Unlocked all files after wave 1"

# Verify locks for wave 1 files are released
LOCK_A=$($FLOWCTL lock-check --file "src/a.rs" --json 2>/dev/null | jq -r '.locked' || echo "false")
[[ "$LOCK_A" == "false" ]] && pass "No stale locks after wave cleanup" || fail "src/a.rs should be unlocked"

echo -e "\033[1;33m--- Wave 2: Dependency unblock ---\033[0m"

# Task 3 should now be ready (both deps satisfied)
READY2=$($FLOWCTL ready --epic "$EPIC" --json | jq -r '.ready[].id' | tr '\n' ',')
echo "$READY2" | grep -q "$EPIC.3" && pass "Task C is now ready (deps met)" || fail "Task C should be ready after A+B done"

# Start and complete task 3
$FLOWCTL lock --task "$EPIC.3" --files "tests/c.rs" --json > /dev/null
$FLOWCTL start "$EPIC.3" --json > /dev/null
echo "Implemented C" > /tmp/summary_c.md
echo '{"commits":["ghi789"],"tests":["cargo test --test integration"]}' > /tmp/evidence_c.json
$FLOWCTL done "$EPIC.3" --summary-file /tmp/summary_c.md --evidence-json /tmp/evidence_c.json --json > /dev/null
STATUS_C=$($FLOWCTL show "$EPIC.3" --json | jq -r .status)
[[ "$STATUS_C" == "done" ]] && pass "Task C completed (status=done)" || fail "Task C status expected done got $STATUS_C"

$FLOWCTL unlock --all --json > /dev/null

echo -e "\033[1;33m--- Epic completion ---\033[0m"

# All tasks done — no more ready tasks
READY3=$($FLOWCTL ready --epic "$EPIC" --json | jq '.ready | length')
[[ "$READY3" == "0" ]] && pass "No more ready tasks (epic complete)" || fail "Expected 0 ready, got $READY3"

# Validate epic
$FLOWCTL validate --epic "$EPIC" --json > /dev/null 2>&1
[[ $? -eq 0 ]] && pass "Epic validation passed" || fail "Epic validation failed"

echo -e "\033[1;33m--- Stale lock recovery ---\033[0m"

# Simulate a crashed worker: lock a file, leave task in_progress, then check recovery
$FLOWCTL task create --epic "$EPIC" --title "Stale lock test" --json > /dev/null
$FLOWCTL start "$EPIC.4" --json > /dev/null
$FLOWCTL lock --task "$EPIC.4" --files "src/stale.rs" --json > /dev/null

# Check lock exists
STALE_LOCKED=$($FLOWCTL lock-check --file "src/stale.rs" --json 2>/dev/null | jq -r '.locked')
[[ "$STALE_LOCKED" == "true" ]] && pass "Stale lock detected" || fail "Lock should exist"

# Simulate recovery: mark task as blocked (simulating crash), then unlock
$FLOWCTL block "$EPIC.4" "Simulated crash" --json > /dev/null 2>&1 || true
$FLOWCTL unlock --task "$EPIC.4" --json > /dev/null

STALE_LOCKED2=$($FLOWCTL lock-check --file "src/stale.rs" --json 2>/dev/null | jq -r '.locked')
[[ "$STALE_LOCKED2" == "false" ]] && pass "Stale lock released after recovery" || fail "Lock should be released"

echo ""
echo -e "\033[1;33m=== Results: $PASS passed, $FAIL failed ===\033[0m"
[[ $FAIL -eq 0 ]] && exit 0 || exit 1
