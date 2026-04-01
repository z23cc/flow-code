#!/usr/bin/env bash
# Teams mode e2e test — validates file locking, ownership, protocol flow
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
FLOWCTL="$SCRIPT_DIR/flowctl.py"

# Safety: refuse to run from plugin repo
if [[ -f "$PWD/scripts/flowctl.py" ]]; then
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

echo -e "\033[1;33m=== Teams e2e tests ===\033[0m"

# --- Setup ---
python3 "$FLOWCTL" init --json > /dev/null
python3 "$FLOWCTL" epic create --title "Teams e2e test" --json > /dev/null
EPIC="fn-1-teams-e2e-test"

# Create 2 tasks with non-overlapping files
python3 "$FLOWCTL" task create --epic "$EPIC" --title "Backend task" --domain backend \
    --files "api/views.py,api/urls.py" --json > /dev/null
python3 "$FLOWCTL" task create --epic "$EPIC" --title "Frontend task" --domain frontend \
    --files "frontend/App.tsx" --json > /dev/null
# Task 3 depends on task 1, overlaps one file
python3 "$FLOWCTL" task create --epic "$EPIC" --title "Integration test" --domain testing \
    --files "tests/test_api.py,api/views.py" --dep "$EPIC.1" --json > /dev/null

echo -e "\033[1;33m--- File ownership ---\033[0m"

# Test ownership map
RESULT=$(python3 "$FLOWCTL" files --epic "$EPIC" --json)
FILE_COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['file_count'])")
CONFLICT_COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['conflict_count'])")
[[ "$FILE_COUNT" -ge 4 ]] && pass "file count >= 4 (got $FILE_COUNT)" || fail "file count expected >= 4 got $FILE_COUNT"
[[ "$CONFLICT_COUNT" == "1" ]] && pass "conflict detected (api/views.py shared)" || fail "expected 1 conflict got $CONFLICT_COUNT"

echo -e "\033[1;33m--- File locking ---\033[0m"

# Lock files for task 1
RESULT=$(python3 "$FLOWCTL" lock --task "$EPIC.1" --files "api/views.py,api/urls.py" --json)
LOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['locked_count'])")
[[ "$LOCKED" == "2" ]] && pass "locked 2 files for task 1" || fail "expected 2 locked got $LOCKED"

# Lock files for task 2 (no conflict)
RESULT=$(python3 "$FLOWCTL" lock --task "$EPIC.2" --files "frontend/App.tsx" --json)
LOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['locked_count'])")
[[ "$LOCKED" == "1" ]] && pass "locked 1 file for task 2 (no conflict)" || fail "expected 1 locked got $LOCKED"

# Try to lock api/views.py for task 3 (should conflict with task 1)
RESULT=$(python3 "$FLOWCTL" lock --task "$EPIC.3" --files "api/views.py,tests/test_api.py" --json)
CONFLICT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['conflict_count'])")
LOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['locked_count'])")
[[ "$CONFLICT" == "1" ]] && pass "conflict detected when task 3 tries api/views.py" || fail "expected 1 conflict got $CONFLICT"
[[ "$LOCKED" == "1" ]] && pass "tests/test_api.py locked (no conflict)" || fail "expected 1 locked got $LOCKED"

echo -e "\033[1;33m--- Lock check ---\033[0m"

# Check specific file
RESULT=$(python3 "$FLOWCTL" lock-check --file "api/views.py" --json)
OWNER=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['owner'])")
LOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['locked'])")
[[ "$LOCKED" == "True" ]] && pass "api/views.py is locked" || fail "expected locked=True"
[[ "$OWNER" == "$EPIC.1" ]] && pass "owner is task 1" || fail "expected owner $EPIC.1 got $OWNER"

# Check unlocked file
RESULT=$(python3 "$FLOWCTL" lock-check --file "nonexistent.py" --json)
LOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['locked'])")
[[ "$LOCKED" == "False" ]] && pass "nonexistent file is not locked" || fail "expected locked=False"

# List all locks
RESULT=$(python3 "$FLOWCTL" lock-check --json)
COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])")
[[ "$COUNT" == "4" ]] && pass "4 total locks active" || fail "expected 4 locks got $COUNT"

echo -e "\033[1;33m--- Unlock flow ---\033[0m"

# Unlock task 1 files (simulates task completion)
RESULT=$(python3 "$FLOWCTL" unlock --task "$EPIC.1" --json)
UNLOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])")
[[ "$UNLOCKED" == "2" ]] && pass "unlocked 2 files for completed task 1" || fail "expected 2 unlocked got $UNLOCKED"

# Now task 3 can lock api/views.py
RESULT=$(python3 "$FLOWCTL" lock --task "$EPIC.3" --files "api/views.py" --json)
CONFLICT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['conflict_count'])")
[[ "$CONFLICT" == "0" ]] && pass "task 3 locks api/views.py after task 1 unlocked" || fail "expected 0 conflicts got $CONFLICT"

echo -e "\033[1;33m--- Clear all locks ---\033[0m"

RESULT=$(python3 "$FLOWCTL" unlock --all --json)
CLEARED=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['cleared'])")
[[ "$CLEARED" -ge 1 ]] && pass "cleared all locks ($CLEARED)" || fail "expected >0 cleared got $CLEARED"

# Verify empty
RESULT=$(python3 "$FLOWCTL" lock-check --json)
COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])")
[[ "$COUNT" == "0" ]] && pass "no locks remaining after clear" || fail "expected 0 locks got $COUNT"

echo -e "\033[1;33m--- Domain filtering ---\033[0m"

RESULT=$(python3 "$FLOWCTL" tasks --epic "$EPIC" --domain backend --json)
COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])")
[[ "$COUNT" == "1" ]] && pass "domain filter: 1 backend task" || fail "expected 1 backend got $COUNT"

RESULT=$(python3 "$FLOWCTL" tasks --epic "$EPIC" --domain testing --json)
COUNT=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['count'])")
[[ "$COUNT" == "1" ]] && pass "domain filter: 1 testing task" || fail "expected 1 testing got $COUNT"

echo -e "\033[1;33m--- Ready tasks respect dependencies ---\033[0m"

RESULT=$(python3 "$FLOWCTL" ready --epic "$EPIC" --json)
READY=$(echo "$RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['ready']))")
BLOCKED=$(echo "$RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['blocked']))")
[[ "$READY" == "2" ]] && pass "2 ready tasks (T1, T2)" || fail "expected 2 ready got $READY"
[[ "$BLOCKED" == "1" ]] && pass "1 blocked task (T3 depends on T1)" || fail "expected 1 blocked got $BLOCKED"

echo -e "\033[1;33m--- Task lifecycle with evidence ---\033[0m"

python3 "$FLOWCTL" start "$EPIC.1" --json > /dev/null
sleep 1
RESULT=$(python3 "$FLOWCTL" done "$EPIC.1" --summary "Implemented API" --evidence-json '{"tests_passed":true}' --json)
STATUS=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])")
DURATION=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('duration_seconds',0))")
[[ "$STATUS" == "done" ]] && pass "task 1 completed" || fail "expected done got $STATUS"
[[ "$DURATION" -ge 1 ]] && pass "duration tracked (${DURATION}s)" || fail "expected duration >= 1 got $DURATION"

# Task 3 should now be unblocked
RESULT=$(python3 "$FLOWCTL" ready --epic "$EPIC" --json)
READY=$(echo "$RESULT" | python3 -c "import sys,json; r=json.load(sys.stdin)['ready']; print(' '.join(t['id'] for t in r))")
echo "$READY" | grep -q "$EPIC.3" && pass "task 3 unblocked after task 1 done" || fail "task 3 not in ready list"

echo ""
echo -e "\033[1;33m=== Results ===\033[0m"
echo -e "Passed: \033[0;32m$PASS\033[0m"
echo -e "Failed: \033[0;31m$FAIL\033[0m"
echo ""
if [[ "$FAIL" -gt 0 ]]; then
    echo -e "\033[0;31mSome tests failed!\033[0m"
    exit 1
else
    echo -e "\033[0;32mAll tests passed!\033[0m"
fi
