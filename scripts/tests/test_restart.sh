#!/usr/bin/env bash
# Tests: restart command, status --interrupted, epic auto-execute
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== restart + status tests ===${NC}"

echo -e "\n${YELLOW}--- restart command ---${NC}"

# Setup: create epic + 3 tasks with deps: .1 -> .2 -> .3
RST_EPIC_JSON="$($FLOWCTL epic create --title "Restart test" --json)"
RST_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$RST_EPIC_JSON")"
$FLOWCTL task create --epic "$RST_EPIC" --title "Task 1" --json > /dev/null
$FLOWCTL task create --epic "$RST_EPIC" --title "Task 2" --deps "${RST_EPIC}.1" --json > /dev/null
$FLOWCTL task create --epic "$RST_EPIC" --title "Task 3" --deps "${RST_EPIC}.2" --json > /dev/null

# Complete tasks 1, 2, 3
$FLOWCTL start "${RST_EPIC}.1" --json > /dev/null
$FLOWCTL done "${RST_EPIC}.1" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
$FLOWCTL start "${RST_EPIC}.2" --json > /dev/null
$FLOWCTL done "${RST_EPIC}.2" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
$FLOWCTL start "${RST_EPIC}.3" --json > /dev/null
$FLOWCTL done "${RST_EPIC}.3" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null

# Test 1: restart --dry-run shows what would be reset
result="$($FLOWCTL restart "${RST_EPIC}.1" --dry-run --json)"
"$PYTHON_BIN" - "$result" "$RST_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
ep = sys.argv[2]
assert data.get("dry_run") == True, f"expected dry_run=True, got {data}"
assert f"{ep}.1" in data.get("would_reset", []), f"{ep}.1 not in would_reset: {data}"
assert f"{ep}.2" in data.get("would_reset", []), f"{ep}.2 not in would_reset: {data}"
assert f"{ep}.3" in data.get("would_reset", []), f"{ep}.3 not in would_reset: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restart --dry-run shows target + downstream"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restart --dry-run failed"
  FAIL=$((FAIL + 1))
fi

# Test 2: restart actually resets target + downstream
result="$($FLOWCTL restart "${RST_EPIC}.1" --json)"
"$PYTHON_BIN" - "$result" "$RST_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
ep = sys.argv[2]
assert data.get("success") == True, f"expected success, got {data}"
assert f"{ep}.1" in data.get("reset", []), f"{ep}.1 not in reset: {data}"
assert f"{ep}.2" in data.get("reset", []), f"{ep}.2 not in reset: {data}"
assert f"{ep}.3" in data.get("reset", []), f"{ep}.3 not in reset: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restart cascades to downstream dependents"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restart cascade failed"
  FAIL=$((FAIL + 1))
fi

# Test 3: verify tasks are back to todo
result="$($FLOWCTL show "${RST_EPIC}.1" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("status") == "todo", f"expected todo, got {data.get('status')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restarted task status is todo"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restarted task not todo"
  FAIL=$((FAIL + 1))
fi

# Test 4: restart already-todo is no-op
result="$($FLOWCTL restart "${RST_EPIC}.1" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("success") == True
assert len(data.get("reset", [])) == 0, f"expected empty reset, got {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restart already-todo is idempotent no-op"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restart idempotent check failed"
  FAIL=$((FAIL + 1))
fi

# Test 5: restart rejects in_progress without --force
$FLOWCTL start "${RST_EPIC}.1" --json > /dev/null
set +e
result="$($FLOWCTL restart "${RST_EPIC}.1" --json 2>&1)"
rc=$?
set -e
"$PYTHON_BIN" - "$result" "$rc" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
rc = int(sys.argv[2])
assert rc != 0, f"expected non-zero exit, got {rc}"
assert "in progress" in data.get("error", "").lower() or "in_progress" in str(data).lower(), f"expected in_progress error: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restart blocks on in_progress without --force"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restart should block in_progress"
  FAIL=$((FAIL + 1))
fi

# Test 6: restart --force overrides in_progress
result="$($FLOWCTL restart "${RST_EPIC}.1" --force --json)"
"$PYTHON_BIN" - "$result" "$RST_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
ep = sys.argv[2]
assert data.get("success") == True
assert f"{ep}.1" in data.get("reset", [])
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} restart --force overrides in_progress"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} restart --force failed"
  FAIL=$((FAIL + 1))
fi

# ── status --interrupted ──
echo -e "\n${YELLOW}=== status --interrupted ===${NC}"

# Create a second epic with todo tasks to test interrupted detection
EPIC_INT_JSON="$($FLOWCTL epic create --title "Interrupted test epic" --json)"
EPIC_INT="$(echo "$EPIC_INT_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC_INT" --title "Interrupted task 1" --json > /dev/null
$FLOWCTL task create --epic "$EPIC_INT" --title "Interrupted task 2" --json > /dev/null

# Test --interrupted --json detects epic with todo tasks
int_json="$($FLOWCTL status --interrupted --json)"
int_count="$(echo "$int_json" | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
epics = data.get("interrupted", [])
matching = [e for e in epics if e["id"] == "'"$EPIC_INT"'"]
print(len(matching))
')"
if [[ "$int_count" == "1" ]]; then
  echo -e "${GREEN}✓${NC} status --interrupted detects epic with todo tasks"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} status --interrupted did not detect epic (found $int_count)"
  FAIL=$((FAIL + 1))
fi

# Verify suggested command is included
int_suggested="$(echo "$int_json" | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
epics = data.get("interrupted", [])
matching = [e for e in epics if e["id"] == "'"$EPIC_INT"'"]
print(matching[0].get("suggested", "") if matching else "")
')"
if [[ "$int_suggested" == "/flow-code:work $EPIC_INT" ]]; then
  echo -e "${GREEN}✓${NC} status --interrupted includes suggested resume command"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} status --interrupted wrong suggested (got: $int_suggested)"
  FAIL=$((FAIL + 1))
fi

# Verify task counts in interrupted output
int_todo="$(echo "$int_json" | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
epics = data.get("interrupted", [])
matching = [e for e in epics if e["id"] == "'"$EPIC_INT"'"]
print(matching[0].get("todo", 0) if matching else 0)
')"
if [[ "$int_todo" == "2" ]]; then
  echo -e "${GREEN}✓${NC} status --interrupted reports correct todo count"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} status --interrupted wrong todo count (expected 2, got $int_todo)"
  FAIL=$((FAIL + 1))
fi

# ── epic set-auto-execute ──
echo -e "\n${YELLOW}=== epic set-auto-execute ===${NC}"

# Create an epic with tasks for auto-execute testing
EPIC_AE_JSON="$($FLOWCTL epic create --title "Auto execute test" --json)"
EPIC_AE="$(echo "$EPIC_AE_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC_AE" --title "AE task 1" --json > /dev/null
$FLOWCTL task create --epic "$EPIC_AE" --title "AE task 2" --json > /dev/null

# Set pending marker
ae_pending="$($FLOWCTL epic auto-exec "$EPIC_AE" --pending --json)"
ae_pending_val="$(echo "$ae_pending" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["auto_execute_pending"])')"
if [[ "$ae_pending_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} set-auto-execute --pending sets marker"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-auto-execute --pending: expected True, got $ae_pending_val"
  FAIL=$((FAIL + 1))
fi

# Verify --interrupted shows it with reason "planned_not_started"
ae_int_json="$($FLOWCTL status --interrupted --json)"
ae_reason="$(echo "$ae_int_json" | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
epics = data.get("interrupted", [])
matching = [e for e in epics if e["id"] == "'"$EPIC_AE"'"]
print(matching[0].get("reason", "") if matching else "")
')"
if [[ "$ae_reason" == "planned_not_started" ]]; then
  echo -e "${GREEN}✓${NC} --interrupted shows planned_not_started reason for pending epic"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} --interrupted wrong reason (expected planned_not_started, got: $ae_reason)"
  FAIL=$((FAIL + 1))
fi

# Clear marker with --done
ae_done="$($FLOWCTL epic auto-exec "$EPIC_AE" --done --json)"
ae_done_val="$(echo "$ae_done" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["auto_execute_pending"])')"
if [[ "$ae_done_val" == "False" ]]; then
  echo -e "${GREEN}✓${NC} set-auto-execute --done clears marker"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-auto-execute --done: expected False, got $ae_done_val"
  FAIL=$((FAIL + 1))
fi

# Verify --interrupted now shows "partially_complete" reason (marker cleared)
ae_int2_json="$($FLOWCTL status --interrupted --json)"
ae_reason2="$(echo "$ae_int2_json" | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
epics = data.get("interrupted", [])
matching = [e for e in epics if e["id"] == "'"$EPIC_AE"'"]
print(matching[0].get("reason", "") if matching else "")
')"
if [[ "$ae_reason2" == "partially_complete" ]]; then
  echo -e "${GREEN}✓${NC} --interrupted shows partially_complete after marker cleared"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} --interrupted wrong reason after clear (expected partially_complete, got: $ae_reason2)"
  FAIL=$((FAIL + 1))
fi

print_results
