#!/usr/bin/env bash
# Tests: gap add/resolve/check, idempotency, priority filtering
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== gap tests ===${NC}"

# Create epic for gap tests
EPIC1_JSON="$($FLOWCTL epic create --title "Gap Epic" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC1" --title "Task 1" --json >/dev/null

echo -e "${YELLOW}--- gap commands ---${NC}"

# Test 1: gap add
gap_add_result="$($FLOWCTL gap add --epic "$EPIC1" --capability "Missing auth check" --priority required --source flow-gap-analyst --json)"
gap_created="$(echo "$gap_add_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("created", False))')"
if [[ "$gap_created" == "True" ]]; then
  echo -e "${GREEN}✓${NC} gap add creates new gap"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap add failed to create gap"
  FAIL=$((FAIL + 1))
fi

# Test 2: gap add idempotent
gap_dup_result="$($FLOWCTL gap add --epic "$EPIC1" --capability "Missing auth check" --priority required --json)"
gap_dup_created="$(echo "$gap_dup_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("created", False))')"
if [[ "$gap_dup_created" == "False" ]]; then
  echo -e "${GREEN}✓${NC} gap add idempotent (duplicate returns created=false)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap add not idempotent"
  FAIL=$((FAIL + 1))
fi

# Test 3: gap add nice-to-have
$FLOWCTL gap add --epic "$EPIC1" --capability "Optional caching" --priority nice-to-have --json >/dev/null

# Test 4: gap list
gap_list_count="$($FLOWCTL gap list --epic "$EPIC1" --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$gap_list_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} gap list returns correct count"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap list count wrong (expected 2, got $gap_list_count)"
  FAIL=$((FAIL + 1))
fi

# Test 5: gap list with status filter
gap_open_count="$($FLOWCTL gap list --epic "$EPIC1" --status open --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$gap_open_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} gap list --status open filter works"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap list --status filter wrong (expected 2, got $gap_open_count)"
  FAIL=$((FAIL + 1))
fi

# Test 6: gap check fails with open required gap
if ! $FLOWCTL gap check --epic "$EPIC1" --json >/dev/null 2>&1; then
  echo -e "${GREEN}✓${NC} gap check fails with open blocking gaps (exit 1)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check should fail with open blocking gaps"
  FAIL=$((FAIL + 1))
fi

# Test 7: gap check JSON has gate=fail
gap_check_gate="$($FLOWCTL gap check --epic "$EPIC1" --json 2>/dev/null || true)"
gap_gate_val="$(echo "$gap_check_gate" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("gate", ""))')"
if [[ "$gap_gate_val" == "fail" ]]; then
  echo -e "${GREEN}✓${NC} gap check gate=fail in JSON output"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check gate expected 'fail', got '$gap_gate_val'"
  FAIL=$((FAIL + 1))
fi

# Test 8: gap resolve
gap_resolve_result="$($FLOWCTL gap resolve --epic "$EPIC1" --capability "Missing auth check" --evidence "Added in auth.py:42" --json)"
gap_changed="$(echo "$gap_resolve_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("changed", False))')"
if [[ "$gap_changed" == "True" ]]; then
  echo -e "${GREEN}✓${NC} gap resolve marks gap as resolved"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap resolve failed"
  FAIL=$((FAIL + 1))
fi

# Test 9: gap resolve idempotent
gap_resolve_dup="$($FLOWCTL gap resolve --epic "$EPIC1" --capability "Missing auth check" --evidence "duplicate" --json)"
gap_dup_changed="$(echo "$gap_resolve_dup" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("changed", False))')"
if [[ "$gap_dup_changed" == "False" ]]; then
  echo -e "${GREEN}✓${NC} gap resolve idempotent (already resolved)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap resolve not idempotent"
  FAIL=$((FAIL + 1))
fi

# Test 10: gap check passes (only nice-to-have left)
if $FLOWCTL gap check --epic "$EPIC1" --json >/dev/null 2>&1; then
  echo -e "${GREEN}✓${NC} gap check passes (nice-to-have does not block)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check should pass with only nice-to-have gaps"
  FAIL=$((FAIL + 1))
fi

# Test 11: gap check gate=pass in JSON
gap_pass_gate="$($FLOWCTL gap check --epic "$EPIC1" --json)"
gap_pass_val="$(echo "$gap_pass_gate" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("gate", ""))')"
if [[ "$gap_pass_val" == "pass" ]]; then
  echo -e "${GREEN}✓${NC} gap check gate=pass in JSON output"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check gate expected 'pass', got '$gap_pass_val'"
  FAIL=$((FAIL + 1))
fi

print_results
