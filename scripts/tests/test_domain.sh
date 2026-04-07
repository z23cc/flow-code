#!/usr/bin/env bash
# Tests: task domain tagging, epic archive/clean
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== domain + archive tests ===${NC}"

echo -e "${YELLOW}--- task domain tagging ---${NC}"

# Setup: create epic + tasks with domains
DOM_EPIC_JSON="$($FLOWCTL epic create --title "Domain test" --json)"
DOM_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$DOM_EPIC_JSON")"
$FLOWCTL task create --epic "$DOM_EPIC" --title "Build API" --domain backend --json > /dev/null
$FLOWCTL task create --epic "$DOM_EPIC" --title "Build UI" --domain frontend --json > /dev/null
$FLOWCTL task create --epic "$DOM_EPIC" --title "No domain" --json > /dev/null

# Test 1: domain stored in task JSON
result="$($FLOWCTL show "${DOM_EPIC}.1" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("domain") == "backend", f"expected backend, got {data.get('domain')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} task create stores domain"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} task create domain not stored"
  FAIL=$((FAIL + 1))
fi

# Test 2: task without domain has null domain
result="$($FLOWCTL show "${DOM_EPIC}.3" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("domain") is None, f"expected None, got {data.get('domain')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} task without domain is null"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} task without domain should be null"
  FAIL=$((FAIL + 1))
fi

# Test 3: tasks --domain filters correctly
result="$($FLOWCTL tasks --epic "$DOM_EPIC" --domain backend --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("count") == 1, f"expected 1, got {data.get('count')}"
assert data["tasks"][0]["domain"] == "backend"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} tasks --domain filters correctly"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} tasks --domain filter failed"
  FAIL=$((FAIL + 1))
fi

# Test 4: tasks without --domain shows all
result="$($FLOWCTL tasks --epic "$DOM_EPIC" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("count") == 3, f"expected 3, got {data.get('count')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} tasks without --domain shows all"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} tasks without --domain should show all"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- epic archive/clean ---${NC}"

# Setup: create + close an epic
ARC_EPIC_JSON="$($FLOWCTL epic create --title "Archive me" --json)"
ARC_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$ARC_EPIC_JSON")"
$FLOWCTL task create --epic "$ARC_EPIC" --title "Done task" --json > /dev/null
$FLOWCTL start "${ARC_EPIC}.1" --json > /dev/null
$FLOWCTL done "${ARC_EPIC}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
$FLOWCTL epic close "$ARC_EPIC" --json > /dev/null

# Test 1: archive moves files
result="$($FLOWCTL epic archive "$ARC_EPIC" --json)"
"$PYTHON_BIN" - "$result" "$ARC_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
ep = sys.argv[2]
assert data.get("success") == True, f"expected success: {data}"
assert data.get("count", 0) >= 3, f"expected >= 3 files moved, got {data.get('count')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} epic archive moves files to .archive/"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} epic archive failed"
  FAIL=$((FAIL + 1))
fi

# Test 2: archived epic no longer shows in list
result="$($FLOWCTL epics --json)"
"$PYTHON_BIN" - "$result" "$ARC_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
ep = sys.argv[2]
ids = [e["id"] for e in data.get("epics", [])]
assert ep not in ids, f"{ep} should not be in epics list: {ids}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} archived epic removed from epics list"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} archived epic still in list"
  FAIL=$((FAIL + 1))
fi

# Test 3: archive dir has the files
if [ -d ".flow/.archive/$ARC_EPIC" ]; then
  echo -e "${GREEN}✓${NC} .flow/.archive/$ARC_EPIC/ directory exists"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} archive directory missing"
  FAIL=$((FAIL + 1))
fi

# Test 4: epic clean archives all closed epics
CLEAN_EP1_JSON="$($FLOWCTL epic create --title "Clean1" --json)"
CLEAN_EP1="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$CLEAN_EP1_JSON")"
$FLOWCTL task create --epic "$CLEAN_EP1" --title "T1" --json > /dev/null
$FLOWCTL start "${CLEAN_EP1}.1" --json > /dev/null
$FLOWCTL done "${CLEAN_EP1}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
$FLOWCTL epic close "$CLEAN_EP1" --json > /dev/null

result="$($FLOWCTL epic clean --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("count", 0) >= 1, f"expected >= 1 archived, got {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} epic clean archives all closed epics"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} epic clean failed"
  FAIL=$((FAIL + 1))
fi

print_results
