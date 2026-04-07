#!/usr/bin/env bash
# Tests: memory init/add/list, memory verify + staleness, epic close retro suggestion
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== memory tests ===${NC}"

echo -e "${YELLOW}--- memory commands ---${NC}"
$FLOWCTL config set memory.enabled true --json >/dev/null
$FLOWCTL memory init --json >/dev/null
if [[ -d ".flow/memory/entries" ]]; then
  echo -e "${GREEN}✓${NC} memory init creates entries dir"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory init creates entries dir"
  FAIL=$((FAIL + 1))
fi

add_result="$($FLOWCTL memory add pitfall "Test pitfall entry" --json)"
add_ok="$(echo "$add_result" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); print(d.get("success",False) and d.get("type")=="pitfall")')"
if [[ "$add_ok" == "True" ]]; then
  echo -e "${GREEN}✓${NC} memory add pitfall"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory add pitfall"
  FAIL=$((FAIL + 1))
fi

$FLOWCTL memory add convention "Test convention" --json >/dev/null
$FLOWCTL memory add decision "Test decision" --json >/dev/null
list_json="$($FLOWCTL memory list --json)"
"$PYTHON_BIN" - <<'PY' "$list_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["success"] == True
counts = data["counts"]
assert counts.get("pitfall", 0) >= 1
assert counts.get("convention", 0) >= 1
assert counts.get("decision", 0) >= 1
assert data["total"] >= 3
PY
echo -e "${GREEN}✓${NC} memory list"
PASS=$((PASS + 1))

echo -e "\n${YELLOW}--- memory verify + staleness ---${NC}"

# Add entry for verify test
$FLOWCTL memory add pitfall "Test pitfall for verify" --json > /dev/null

# Test 1: memory verify updates last_verified
result="$($FLOWCTL memory verify 1 --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("id") == 1
assert "last_verified" in data
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} memory verify updates last_verified"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory verify failed"
  FAIL=$((FAIL + 1))
fi

# Test 2: memory list includes last_verified and stale flag in JSON
result="$($FLOWCTL memory list --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
entry = data["index"][0]
assert "last_verified" in entry, f"missing last_verified: {entry}"
assert "stale" in entry, f"missing stale flag: {entry}"
assert entry["stale"] == False, f"newly verified should not be stale: {entry}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} memory list shows last_verified + stale flag"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory list missing staleness fields"
  FAIL=$((FAIL + 1))
fi

# Test 3: epic close includes retro_suggested
EPC_EPIC_JSON="$($FLOWCTL epic create --title "Retro prompt test" --json)"
EPC_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$EPC_EPIC_JSON")"
$FLOWCTL task create --epic "$EPC_EPIC" --title "Done task" --json > /dev/null
$FLOWCTL start "${EPC_EPIC}.1" --json > /dev/null
$FLOWCTL done "${EPC_EPIC}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
result="$($FLOWCTL epic close "$EPC_EPIC" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("retro_suggested") == True, f"missing retro_suggested: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} epic close suggests retro"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} epic close missing retro suggestion"
  FAIL=$((FAIL + 1))
fi

print_results
