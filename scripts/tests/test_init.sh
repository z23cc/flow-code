#!/usr/bin/env bash
# Tests: idempotent init, config upgrade, config set/get, planSync config
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== init + config tests ===${NC}"

echo -e "${YELLOW}--- idempotent init ---${NC}"

# Test 1: Re-run init (no changes)
init_result="$($FLOWCTL init --json)"
init_actions="$(echo "$init_result" | "$PYTHON_BIN" -c 'import json,sys; print(len(json.load(sys.stdin).get("actions", [])))')"
if [[ "$init_actions" == "0" ]]; then
  echo -e "${GREEN}✓${NC} init idempotent (no changes on re-run)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init idempotent: expected 0 actions, got $init_actions"
  FAIL=$((FAIL + 1))
fi

# Test 2: Config upgrade (old config without planSync)
echo '{"memory":{"enabled":true}}' > .flow/config.json
init_upgrade="$($FLOWCTL init --json)"
upgrade_msg="$(echo "$init_upgrade" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("message", ""))')"
if [[ "$upgrade_msg" == *"upgraded config.json"* ]]; then
  echo -e "${GREEN}✓${NC} init upgrades config (adds missing keys)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init upgrade: expected 'upgraded config.json' in message, got: $upgrade_msg"
  FAIL=$((FAIL + 1))
fi

# Test 3: Verify existing values preserved after upgrade
memory_val="$($FLOWCTL config get memory.enabled --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("value"))')"
if [[ "$memory_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} init preserves existing config values"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init preserve: expected memory.enabled=True, got $memory_val"
  FAIL=$((FAIL + 1))
fi

# Test 4: Verify new defaults added (memory + planSync now default to True)
plansync_val="$($FLOWCTL config get planSync.enabled --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("value"))')"
if [[ "$plansync_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} init adds new default keys"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init defaults: expected planSync.enabled=True, got $plansync_val"
  FAIL=$((FAIL + 1))
fi

# Reset config for remaining tests
$FLOWCTL config set memory.enabled false --json >/dev/null

echo -e "${YELLOW}--- config set/get ---${NC}"
$FLOWCTL config set memory.enabled true --json >/dev/null
config_json="$($FLOWCTL config get memory.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] == True, f"Expected True, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} config set/get"
PASS=$((PASS + 1))

$FLOWCTL config set memory.enabled false --json >/dev/null
config_json="$($FLOWCTL config get memory.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] == False, f"Expected False, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} config toggle"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- planSync config ---${NC}"
$FLOWCTL config set planSync.enabled true --json >/dev/null
config_json="$($FLOWCTL config get planSync.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] is True, f"Expected True, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} planSync config set/get"
PASS=$((PASS + 1))

$FLOWCTL config set planSync.enabled false --json >/dev/null
config_json="$($FLOWCTL config get planSync.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] is False, f"Expected False, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} planSync config toggle"
PASS=$((PASS + 1))

print_results
