#!/usr/bin/env bash
# Tests: next plan/work/none, priority scheduling, artifact file resilience
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== scheduling tests ===${NC}"

echo -e "${YELLOW}--- next: plan/work/none + priority ---${NC}"
# Capture epic ID from create output (fn-N-xxx format)
EPIC1_JSON="$($FLOWCTL epic create --title "Epic One" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC1" --title "Low pri" --priority 5 --json >/dev/null
$FLOWCTL task create --epic "$EPIC1" --title "High pri" --priority 1 --json >/dev/null

plan_json="$($FLOWCTL next --require-plan-review --json)"
"$PYTHON_BIN" - "$plan_json" "$EPIC1" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_epic = sys.argv[2]
assert data["status"] == "plan"
assert data["epic"] == expected_epic, f"Expected {expected_epic}, got {data['epic']}"
PY
echo -e "${GREEN}✓${NC} next plan"
PASS=$((PASS + 1))

$FLOWCTL epic review "$EPIC1" ship --json >/dev/null
work_json="$($FLOWCTL next --json)"
"$PYTHON_BIN" - "$work_json" "$EPIC1" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_epic = sys.argv[2]
assert data["status"] == "work"
assert data["task"] == f"{expected_epic}.2", f"Expected {expected_epic}.2, got {data['task']}"
PY
echo -e "${GREEN}✓${NC} next work priority"
PASS=$((PASS + 1))

$FLOWCTL start "${EPIC1}.2" --json >/dev/null
$FLOWCTL done "${EPIC1}.2" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
$FLOWCTL start "${EPIC1}.1" --json >/dev/null
$FLOWCTL done "${EPIC1}.1" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
none_json="$($FLOWCTL next --json)"
"$PYTHON_BIN" - <<'PY' "$none_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["status"] == "none"
PY
echo -e "${GREEN}✓${NC} next none"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- artifact files in tasks dir (GH-21) ---${NC}"
# Create artifact files that match glob but aren't valid task files
# This simulates Claude writing evidence/summary files to .flow/tasks/
cat > ".flow/tasks/${EPIC1}.1-evidence.json" << 'EOF'
{"commits":["abc123"],"tests":["npm test"],"prs":[]}
EOF
cat > ".flow/tasks/${EPIC1}.1-summary.json" << 'EOF'
{"summary":"Task completed successfully"}
EOF
# Test that next still works with artifact files present
set +e
next_result="$($FLOWCTL next --json 2>&1)"
next_rc=$?
set -e
if [[ "$next_rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} next ignores artifact files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} next crashes on artifact files: $next_result"
  FAIL=$((FAIL + 1))
fi
# Test that list still works
set +e
list_result="$($FLOWCTL list --json 2>&1)"
list_rc=$?
set -e
if [[ "$list_rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} list ignores artifact files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} list crashes on artifact files: $list_result"
  FAIL=$((FAIL + 1))
fi
# Test that ready still works
set +e
ready_result="$($FLOWCTL ready --epic "$EPIC1" --json 2>&1)"
ready_rc=$?
set -e
if [[ "$ready_rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} ready ignores artifact files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} ready crashes on artifact files: $ready_result"
  FAIL=$((FAIL + 1))
fi
# Test that show (with tasks) still works
set +e
show_result="$($FLOWCTL show "$EPIC1" --json 2>&1)"
show_rc=$?
set -e
if [[ "$show_rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} show ignores artifact files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} show crashes on artifact files: $show_result"
  FAIL=$((FAIL + 1))
fi
# Test that validate still works
set +e
validate_result="$($FLOWCTL validate --epic "$EPIC1" --json 2>&1)"
validate_rc=$?
set -e
if [[ "$validate_rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} validate ignores artifact files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} validate crashes on artifact files: $validate_result"
  FAIL=$((FAIL + 1))
fi
# Cleanup artifact files
rm -f ".flow/tasks/${EPIC1}.1-evidence.json" ".flow/tasks/${EPIC1}.1-summary.json"

print_results
