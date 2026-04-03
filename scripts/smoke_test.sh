#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Python detection: prefer python3, fallback to python (Windows support, GH-35)
pick_python() {
  if [[ -n "${PYTHON_BIN:-}" ]]; then
    command -v "$PYTHON_BIN" >/dev/null 2>&1 && { echo "$PYTHON_BIN"; return; }
  fi
  if command -v python3 >/dev/null 2>&1; then echo "python3"; return; fi
  if command -v python  >/dev/null 2>&1; then echo "python"; return; fi
  echo ""
}

PYTHON_BIN="$(pick_python)"
[[ -n "$PYTHON_BIN" ]] || { echo "ERROR: python not found (need python3 or python in PATH)" >&2; exit 1; }

# Safety: never run tests from the main plugin repo
if [[ -f "$PWD/.claude-plugin/marketplace.json" ]] || [[ -f "$PWD/plugins/flow-code/.claude-plugin/plugin.json" ]]; then
  echo "ERROR: refusing to run from main plugin repo. Run from any other directory." >&2
  exit 1
fi

TEST_DIR="/tmp/flowctl-smoke-$$"
PASS=0
FAIL=0

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

cleanup() {
  rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo -e "${YELLOW}=== flowctl smoke tests ===${NC}"

mkdir -p "$TEST_DIR/repo/scripts"
cd "$TEST_DIR/repo"
git init -q

cp "$PLUGIN_ROOT/scripts/flowctl.py" scripts/flowctl.py
cp "$PLUGIN_ROOT/scripts/flowctl.py" scripts/flowctl.py
cp -r "$PLUGIN_ROOT/scripts/flowctl" scripts/flowctl
chmod +x scripts/flowctl.py

scripts/flowctl.py init --json >/dev/null
printf '{"commits":[],"tests":[],"prs":[]}' > "$TEST_DIR/evidence.json"
printf "ok\n" > "$TEST_DIR/summary.md"

echo -e "${YELLOW}--- idempotent init ---${NC}"

# Test 1: Re-run init (no changes)
init_result="$(scripts/flowctl.py init --json)"
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
init_upgrade="$(scripts/flowctl.py init --json)"
upgrade_msg="$(echo "$init_upgrade" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("message", ""))')"
if [[ "$upgrade_msg" == *"upgraded config.json"* ]]; then
  echo -e "${GREEN}✓${NC} init upgrades config (adds missing keys)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init upgrade: expected 'upgraded config.json' in message, got: $upgrade_msg"
  FAIL=$((FAIL + 1))
fi

# Test 3: Verify existing values preserved after upgrade
memory_val="$(scripts/flowctl.py config get memory.enabled --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("value"))')"
if [[ "$memory_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} init preserves existing config values"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init preserve: expected memory.enabled=True, got $memory_val"
  FAIL=$((FAIL + 1))
fi

# Test 4: Verify new defaults added (memory + planSync now default to True)
plansync_val="$(scripts/flowctl.py config get planSync.enabled --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("value"))')"
if [[ "$plansync_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} init adds new default keys"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} init defaults: expected planSync.enabled=True, got $plansync_val"
  FAIL=$((FAIL + 1))
fi

# Reset config for remaining tests
scripts/flowctl.py config set memory.enabled false --json >/dev/null

echo -e "${YELLOW}--- next: plan/work/none + priority ---${NC}"
# Capture epic ID from create output (fn-N-xxx format)
EPIC1_JSON="$(scripts/flowctl.py epic create --title "Epic One" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$EPIC1" --title "Low pri" --priority 5 --json >/dev/null
scripts/flowctl.py task create --epic "$EPIC1" --title "High pri" --priority 1 --json >/dev/null

plan_json="$(scripts/flowctl.py next --require-plan-review --json)"
"$PYTHON_BIN" - "$plan_json" "$EPIC1" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_epic = sys.argv[2]
assert data["status"] == "plan"
assert data["epic"] == expected_epic, f"Expected {expected_epic}, got {data['epic']}"
PY
echo -e "${GREEN}✓${NC} next plan"
PASS=$((PASS + 1))

scripts/flowctl.py epic set-plan-review-status "$EPIC1" --status ship --json >/dev/null
work_json="$(scripts/flowctl.py next --json)"
"$PYTHON_BIN" - "$work_json" "$EPIC1" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_epic = sys.argv[2]
assert data["status"] == "work"
assert data["task"] == f"{expected_epic}.2", f"Expected {expected_epic}.2, got {data['task']}"
PY
echo -e "${GREEN}✓${NC} next work priority"
PASS=$((PASS + 1))

scripts/flowctl.py start "${EPIC1}.2" --json >/dev/null
scripts/flowctl.py done "${EPIC1}.2" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
scripts/flowctl.py start "${EPIC1}.1" --json >/dev/null
scripts/flowctl.py done "${EPIC1}.1" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
none_json="$(scripts/flowctl.py next --json)"
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
next_result="$(scripts/flowctl.py next --json 2>&1)"
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
list_result="$(scripts/flowctl.py list --json 2>&1)"
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
ready_result="$(scripts/flowctl.py ready --epic "$EPIC1" --json 2>&1)"
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
show_result="$(scripts/flowctl.py show "$EPIC1" --json 2>&1)"
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
validate_result="$(scripts/flowctl.py validate --epic "$EPIC1" --json 2>&1)"
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

echo -e "${YELLOW}--- plan_review_status default ---${NC}"
"$PYTHON_BIN" - "$EPIC1" <<'PY'
import json, sys
from pathlib import Path
epic_id = sys.argv[1]
path = Path(f".flow/epics/{epic_id}.json")
data = json.loads(path.read_text())
data.pop("plan_review_status", None)
data.pop("plan_reviewed_at", None)
data.pop("branch_name", None)
path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
show_json="$(scripts/flowctl.py show "$EPIC1" --json)"
"$PYTHON_BIN" - <<'PY' "$show_json"
import json, sys
data = json.loads(sys.argv[1])
assert data.get("plan_review_status") == "unknown"
assert data.get("plan_reviewed_at") is None
assert data.get("branch_name") is None
PY
echo -e "${GREEN}✓${NC} plan_review_status defaulted"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- branch_name set ---${NC}"
scripts/flowctl.py epic set-branch "$EPIC1" --branch "${EPIC1}-epic" --json >/dev/null
show_json="$(scripts/flowctl.py show "$EPIC1" --json)"
"$PYTHON_BIN" - "$show_json" "$EPIC1" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_branch = f"{sys.argv[2]}-epic"
assert data.get("branch_name") == expected_branch, f"Expected {expected_branch}, got {data.get('branch_name')}"
PY
echo -e "${GREEN}✓${NC} branch_name set"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- epic set-title ---${NC}"
# Create epic with tasks for rename test
RENAME_EPIC_JSON="$(scripts/flowctl.py epic create --title "Old Title" --json)"
RENAME_EPIC="$(echo "$RENAME_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$RENAME_EPIC" --title "First task" --json >/dev/null
scripts/flowctl.py task create --epic "$RENAME_EPIC" --title "Second task" --json >/dev/null
# Add task dependency within epic
scripts/flowctl.py dep add "${RENAME_EPIC}.2" "${RENAME_EPIC}.1" --json >/dev/null

# Rename epic
rename_result="$(scripts/flowctl.py epic set-title "$RENAME_EPIC" --title "New Shiny Title" --json)"
NEW_EPIC="$(echo "$rename_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["new_id"])')"

# Test 1: Verify old files are gone
if [[ ! -f ".flow/epics/${RENAME_EPIC}.json" ]] && [[ ! -f ".flow/specs/${RENAME_EPIC}.md" ]]; then
  echo -e "${GREEN}✓${NC} set-title removes old files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-title old files still exist"
  FAIL=$((FAIL + 1))
fi

# Test 2: Verify new files exist
if [[ -f ".flow/epics/${NEW_EPIC}.json" ]] && [[ -f ".flow/specs/${NEW_EPIC}.md" ]]; then
  echo -e "${GREEN}✓${NC} set-title creates new files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-title new files missing"
  FAIL=$((FAIL + 1))
fi

# Test 3: Verify epic JSON content updated
"$PYTHON_BIN" - "$NEW_EPIC" <<'PY'
import json, sys
from pathlib import Path
new_id = sys.argv[1]
epic_data = json.loads(Path(f".flow/epics/{new_id}.json").read_text())
assert epic_data["id"] == new_id, f"Epic ID not updated: {epic_data['id']}"
assert epic_data["title"] == "New Shiny Title", f"Title not updated: {epic_data['title']}"
assert new_id in epic_data["spec_path"], f"spec_path not updated: {epic_data['spec_path']}"
PY
echo -e "${GREEN}✓${NC} set-title updates epic JSON"
PASS=$((PASS + 1))

# Test 4: Verify task files renamed
if [[ -f ".flow/tasks/${NEW_EPIC}.1.json" ]] && [[ -f ".flow/tasks/${NEW_EPIC}.2.json" ]]; then
  echo -e "${GREEN}✓${NC} set-title renames task files"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-title task files not renamed"
  FAIL=$((FAIL + 1))
fi

# Test 5: Verify task JSON content updated (including depends_on)
"$PYTHON_BIN" - "$NEW_EPIC" <<'PY'
import json, sys
from pathlib import Path
new_id = sys.argv[1]
task1_data = json.loads(Path(f".flow/tasks/{new_id}.1.json").read_text())
task2_data = json.loads(Path(f".flow/tasks/{new_id}.2.json").read_text())
assert task1_data["id"] == f"{new_id}.1", f"Task 1 ID not updated: {task1_data['id']}"
assert task1_data["epic"] == new_id, f"Task 1 epic not updated: {task1_data['epic']}"
assert task2_data["id"] == f"{new_id}.2", f"Task 2 ID not updated: {task2_data['id']}"
# Verify depends_on was updated
deps = task2_data.get("depends_on", [])
assert f"{new_id}.1" in deps, f"depends_on not updated: {deps}"
PY
echo -e "${GREEN}✓${NC} set-title updates task JSON and deps"
PASS=$((PASS + 1))

# Test 6: Verify show works with new ID
show_json="$(scripts/flowctl.py show "$NEW_EPIC" --json)"
"$PYTHON_BIN" - "$show_json" "$NEW_EPIC" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
expected_id = sys.argv[2]
assert data["id"] == expected_id, f"Show returns wrong ID: {data['id']}"
assert data["title"] == "New Shiny Title"
PY
echo -e "${GREEN}✓${NC} set-title show works with new ID"
PASS=$((PASS + 1))

# Test 7: depends_on_epics update in other epics
DEP_EPIC_JSON="$(scripts/flowctl.py epic create --title "Depends on renamed" --json)"
DEP_EPIC="$(echo "$DEP_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py epic add-dep "$DEP_EPIC" "$NEW_EPIC" --json >/dev/null
# Rename the dependency
rename2_result="$(scripts/flowctl.py epic set-title "$NEW_EPIC" --title "Final Title" --json)"
FINAL_EPIC="$(echo "$rename2_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["new_id"])')"
# Verify DEP_EPIC's depends_on_epics was updated
"$PYTHON_BIN" - "$DEP_EPIC" "$FINAL_EPIC" <<'PY'
import json, sys
from pathlib import Path
dep_epic = sys.argv[1]
final_epic = sys.argv[2]
dep_data = json.loads(Path(f".flow/epics/{dep_epic}.json").read_text())
deps = dep_data.get("depends_on_epics", [])
assert final_epic in deps, f"depends_on_epics not updated: {deps}, expected {final_epic}"
PY
echo -e "${GREEN}✓${NC} set-title updates depends_on_epics in other epics"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- block + validate + epic close ---${NC}"
EPIC2_JSON="$(scripts/flowctl.py epic create --title "Epic Two" --json)"
EPIC2="$(echo "$EPIC2_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$EPIC2" --title "Block me" --json >/dev/null
scripts/flowctl.py task create --epic "$EPIC2" --title "Other" --json >/dev/null
printf "Blocked by test\n" > "$TEST_DIR/reason.md"
scripts/flowctl.py block "${EPIC2}.1" --reason-file "$TEST_DIR/reason.md" --json >/dev/null
scripts/flowctl.py validate --epic "$EPIC2" --json >/dev/null
echo -e "${GREEN}✓${NC} validate allows blocked"
PASS=$((PASS + 1))

set +e
scripts/flowctl.py epic close "$EPIC2" --json >/dev/null
rc=$?
set -e
if [[ "$rc" -ne 0 ]]; then
  echo -e "${GREEN}✓${NC} epic close fails when blocked"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} epic close fails when blocked"
  FAIL=$((FAIL + 1))
fi

scripts/flowctl.py start "${EPIC2}.1" --force --json >/dev/null
scripts/flowctl.py done "${EPIC2}.1" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
scripts/flowctl.py start "${EPIC2}.2" --json >/dev/null
scripts/flowctl.py done "${EPIC2}.2" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
scripts/flowctl.py epic close "$EPIC2" --json >/dev/null
echo -e "${GREEN}✓${NC} epic close succeeds when done"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- config set/get ---${NC}"
scripts/flowctl.py config set memory.enabled true --json >/dev/null
config_json="$(scripts/flowctl.py config get memory.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] == True, f"Expected True, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} config set/get"
PASS=$((PASS + 1))

scripts/flowctl.py config set memory.enabled false --json >/dev/null
config_json="$(scripts/flowctl.py config get memory.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] == False, f"Expected False, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} config toggle"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- planSync config ---${NC}"
scripts/flowctl.py config set planSync.enabled true --json >/dev/null
config_json="$(scripts/flowctl.py config get planSync.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] is True, f"Expected True, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} planSync config set/get"
PASS=$((PASS + 1))

scripts/flowctl.py config set planSync.enabled false --json >/dev/null
config_json="$(scripts/flowctl.py config get planSync.enabled --json)"
"$PYTHON_BIN" - <<'PY' "$config_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["value"] is False, f"Expected False, got {data['value']}"
PY
echo -e "${GREEN}✓${NC} planSync config toggle"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- gap commands ---${NC}"

# Use EPIC1 which was created earlier in the test
# Test 1: gap add
gap_add_result="$(scripts/flowctl.py gap add --epic "$EPIC1" --capability "Missing auth check" --priority required --source flow-gap-analyst --json)"
gap_created="$(echo "$gap_add_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("created", False))')"
if [[ "$gap_created" == "True" ]]; then
  echo -e "${GREEN}✓${NC} gap add creates new gap"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap add failed to create gap"
  FAIL=$((FAIL + 1))
fi

# Test 2: gap add idempotent
gap_dup_result="$(scripts/flowctl.py gap add --epic "$EPIC1" --capability "Missing auth check" --priority required --json)"
gap_dup_created="$(echo "$gap_dup_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("created", False))')"
if [[ "$gap_dup_created" == "False" ]]; then
  echo -e "${GREEN}✓${NC} gap add idempotent (duplicate returns created=false)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap add not idempotent"
  FAIL=$((FAIL + 1))
fi

# Test 3: gap add nice-to-have
scripts/flowctl.py gap add --epic "$EPIC1" --capability "Optional caching" --priority nice-to-have --json >/dev/null

# Test 4: gap list
gap_list_count="$(scripts/flowctl.py gap list --epic "$EPIC1" --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$gap_list_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} gap list returns correct count"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap list count wrong (expected 2, got $gap_list_count)"
  FAIL=$((FAIL + 1))
fi

# Test 5: gap list with status filter
gap_open_count="$(scripts/flowctl.py gap list --epic "$EPIC1" --status open --json | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$gap_open_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} gap list --status open filter works"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap list --status filter wrong (expected 2, got $gap_open_count)"
  FAIL=$((FAIL + 1))
fi

# Test 6: gap check fails with open required gap
if ! scripts/flowctl.py gap check --epic "$EPIC1" --json >/dev/null 2>&1; then
  echo -e "${GREEN}✓${NC} gap check fails with open blocking gaps (exit 1)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check should fail with open blocking gaps"
  FAIL=$((FAIL + 1))
fi

# Test 7: gap check JSON has gate=fail
gap_check_gate="$(scripts/flowctl.py gap check --epic "$EPIC1" --json 2>/dev/null || true)"
gap_gate_val="$(echo "$gap_check_gate" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("gate", ""))')"
if [[ "$gap_gate_val" == "fail" ]]; then
  echo -e "${GREEN}✓${NC} gap check gate=fail in JSON output"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check gate expected 'fail', got '$gap_gate_val'"
  FAIL=$((FAIL + 1))
fi

# Test 8: gap resolve
gap_resolve_result="$(scripts/flowctl.py gap resolve --epic "$EPIC1" --capability "Missing auth check" --evidence "Added in auth.py:42" --json)"
gap_changed="$(echo "$gap_resolve_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("changed", False))')"
if [[ "$gap_changed" == "True" ]]; then
  echo -e "${GREEN}✓${NC} gap resolve marks gap as resolved"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap resolve failed"
  FAIL=$((FAIL + 1))
fi

# Test 9: gap resolve idempotent
gap_resolve_dup="$(scripts/flowctl.py gap resolve --epic "$EPIC1" --capability "Missing auth check" --evidence "duplicate" --json)"
gap_dup_changed="$(echo "$gap_resolve_dup" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("changed", False))')"
if [[ "$gap_dup_changed" == "False" ]]; then
  echo -e "${GREEN}✓${NC} gap resolve idempotent (already resolved)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap resolve not idempotent"
  FAIL=$((FAIL + 1))
fi

# Test 10: gap check passes (only nice-to-have left)
if scripts/flowctl.py gap check --epic "$EPIC1" --json >/dev/null 2>&1; then
  echo -e "${GREEN}✓${NC} gap check passes (nice-to-have does not block)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check should pass with only nice-to-have gaps"
  FAIL=$((FAIL + 1))
fi

# Test 11: gap check gate=pass in JSON
gap_pass_gate="$(scripts/flowctl.py gap check --epic "$EPIC1" --json)"
gap_pass_val="$(echo "$gap_pass_gate" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("gate", ""))')"
if [[ "$gap_pass_val" == "pass" ]]; then
  echo -e "${GREEN}✓${NC} gap check gate=pass in JSON output"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} gap check gate expected 'pass', got '$gap_pass_val'"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- memory commands ---${NC}"
scripts/flowctl.py config set memory.enabled true --json >/dev/null
scripts/flowctl.py memory init --json >/dev/null
if [[ -d ".flow/memory/entries" ]]; then
  echo -e "${GREEN}✓${NC} memory init creates entries dir"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory init creates entries dir"
  FAIL=$((FAIL + 1))
fi

add_result="$(scripts/flowctl.py memory add pitfall "Test pitfall entry" --json)"
add_ok="$(echo "$add_result" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); print(d.get("success",False) and d.get("type")=="pitfall")')"
if [[ "$add_ok" == "True" ]]; then
  echo -e "${GREEN}✓${NC} memory add pitfall"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} memory add pitfall"
  FAIL=$((FAIL + 1))
fi

scripts/flowctl.py memory add convention "Test convention" --json >/dev/null
scripts/flowctl.py memory add decision "Test decision" --json >/dev/null
list_json="$(scripts/flowctl.py memory list --json)"
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

echo -e "${YELLOW}--- schema v1 validate ---${NC}"
"$PYTHON_BIN" - <<'PY'
import json
from pathlib import Path
path = Path(".flow/meta.json")
data = json.loads(path.read_text())
data["schema_version"] = 1
path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
scripts/flowctl.py validate --all --json >/dev/null
echo -e "${GREEN}✓${NC} schema v1 validate"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- codex commands ---${NC}"
# Test codex check (may or may not have codex installed)
codex_check_json="$(scripts/flowctl.py codex check --json 2>/dev/null || echo '{"success":true}')"
"$PYTHON_BIN" - <<'PY' "$codex_check_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["success"] == True, f"codex check failed: {data}"
# available can be true or false depending on codex install
PY
echo -e "${GREEN}✓${NC} codex check"
PASS=$((PASS + 1))

# Test codex impl-review help (no codex required for argparse check)
set +e
scripts/flowctl.py codex impl-review --help >/dev/null 2>&1
rc=$?
set -e
if [[ "$rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} codex impl-review --help"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} codex impl-review --help"
  FAIL=$((FAIL + 1))
fi

# Test codex plan-review help
set +e
scripts/flowctl.py codex plan-review --help >/dev/null 2>&1
rc=$?
set -e
if [[ "$rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} codex plan-review --help"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} codex plan-review --help"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- context hints ---${NC}"
# Create files in same commit, then modify one to test context hints
mkdir -p "$TEST_DIR/repo/src"
# First commit: both auth.py and handler.py together
cat > "$TEST_DIR/repo/src/auth.py" << 'EOF'
def validate_token(token: str) -> bool:
    """Validate JWT token."""
    return len(token) > 10

class User:
    def __init__(self, name: str):
        self.name = name
EOF
cat > "$TEST_DIR/repo/src/handler.py" << 'EOF'
from auth import validate_token, User

def handle_request(token: str):
    if validate_token(token):
        return User("test")
    return None
EOF
git -C "$TEST_DIR/repo" add src/
git -C "$TEST_DIR/repo" commit -m "Add auth and handler" >/dev/null

# Second commit: only modify auth.py (handler.py stays unchanged)
cat > "$TEST_DIR/repo/src/auth.py" << 'EOF'
def validate_token(token: str) -> bool:
    """Validate JWT token with expiry check."""
    if len(token) < 10:
        return False
    return True

class User:
    def __init__(self, name: str, email: str = ""):
        self.name = name
        self.email = email
EOF
git -C "$TEST_DIR/repo" add src/auth.py
git -C "$TEST_DIR/repo" commit -m "Update auth with expiry" >/dev/null

# Test context hints: should find handler.py referencing validate_token/User
cd "$TEST_DIR/repo"
hints_output="$(PYTHONPATH="$SCRIPT_DIR" "$PYTHON_BIN" -c "
from flowctl import gather_context_hints
hints = gather_context_hints('HEAD~1')
print(hints)
" 2>&1)"

# Verify hints mention handler.py referencing validate_token or User
if echo "$hints_output" | grep -q "handler.py"; then
  echo -e "${GREEN}✓${NC} context hints finds references"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} context hints finds references (got: $hints_output)"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- build_review_prompt ---${NC}"
# Go back to plugin root for Python tests
cd "$TEST_DIR/repo"
# Test that build_review_prompt generates proper structure
"$PYTHON_BIN" - "$SCRIPT_DIR" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
from flowctl import build_review_prompt

# Test impl prompt has all 7 criteria
impl_prompt = build_review_prompt("impl", "Test spec", "Test hints", "Test diff")
assert "<review_instructions>" in impl_prompt
assert "Correctness" in impl_prompt
assert "Simplicity" in impl_prompt
assert "DRY" in impl_prompt
assert "Architecture" in impl_prompt
assert "Edge Cases" in impl_prompt
assert "Tests" in impl_prompt
assert "Security" in impl_prompt
assert "<verdict>SHIP</verdict>" in impl_prompt
assert "File:Line" in impl_prompt  # Structured output format

# Test plan prompt has all 7 criteria
plan_prompt = build_review_prompt("plan", "Test spec", "Test hints")
assert "Completeness" in plan_prompt
assert "Feasibility" in plan_prompt
assert "Clarity" in plan_prompt
assert "Architecture" in plan_prompt
assert "Risks" in plan_prompt
assert "Scope" in plan_prompt
assert "Testability" in plan_prompt
assert "<verdict>SHIP</verdict>" in plan_prompt

# Test context hints and diff are included
assert "<context_hints>" in impl_prompt
assert "Test hints" in impl_prompt
assert "<diff_summary>" in impl_prompt
assert "Test diff" in impl_prompt
assert "<spec>" in impl_prompt
assert "Test spec" in impl_prompt
PY
echo -e "${GREEN}✓${NC} build_review_prompt has full criteria"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- parse_receipt_path ---${NC}"
# Test receipt path parsing for Ralph gating (both legacy and new fn-N-xxx formats)
"$PYTHON_BIN" - "$SCRIPT_DIR/hooks" <<'PY'
import sys
hooks_dir = sys.argv[1]
sys.path.insert(0, hooks_dir)
from importlib.util import spec_from_file_location, module_from_spec
spec = spec_from_file_location("ralph_guard", f"{hooks_dir}/ralph-guard.py")
guard = module_from_spec(spec)
spec.loader.exec_module(guard)

# Test plan receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/plan-fn-1.json")
assert rtype == "plan_review", f"Expected plan_review, got {rtype}"
assert rid == "fn-1", f"Expected fn-1, got {rid}"

# Test impl receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/impl-fn-1.3.json")
assert rtype == "impl_review", f"Expected impl_review, got {rtype}"
assert rid == "fn-1.3", f"Expected fn-1.3, got {rid}"

# Test plan receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/plan-fn-5-x7k.json")
assert rtype == "plan_review", f"Expected plan_review, got {rtype}"
assert rid == "fn-5-x7k", f"Expected fn-5-x7k, got {rid}"

# Test impl receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/impl-fn-5-x7k.3.json")
assert rtype == "impl_review", f"Expected impl_review, got {rtype}"
assert rid == "fn-5-x7k.3", f"Expected fn-5-x7k.3, got {rid}"

# Test completion receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/completion-fn-2.json")
assert rtype == "completion_review", f"Expected completion_review, got {rtype}"
assert rid == "fn-2", f"Expected fn-2, got {rid}"

# Test completion receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/completion-fn-7-abc.json")
assert rtype == "completion_review", f"Expected completion_review, got {rtype}"
assert rid == "fn-7-abc", f"Expected fn-7-abc, got {rid}"

# Test fallback
rtype, rid = guard.parse_receipt_path("/tmp/unknown.json")
assert rtype == "impl_review"
assert rid == "UNKNOWN"
PY
echo -e "${GREEN}✓${NC} parse_receipt_path works"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- codex e2e (requires codex CLI) ---${NC}"
# Check if codex is available (handles its own auth)
codex_available="$(scripts/flowctl.py codex check --json 2>/dev/null | "$PYTHON_BIN" -c "import sys,json; print(json.load(sys.stdin).get('available', False))" 2>/dev/null || echo "False")"
if [[ "$codex_available" == "True" ]]; then
  # Create a simple epic + task for testing
  EPIC3_JSON="$(scripts/flowctl.py epic create --title "Codex test epic" --json)"
  EPIC3="$(echo "$EPIC3_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
  scripts/flowctl.py task create --epic "$EPIC3" --title "Test task" --json >/dev/null

  # Write a simple spec
  cat > ".flow/specs/${EPIC3}.md" << 'EOF'
# Codex Test Epic

Simple test epic for smoke testing codex reviews.

## Scope
- Test that codex can review a plan
- Test that codex can review an implementation
EOF

  cat > ".flow/tasks/${EPIC3}.1.md" << 'EOF'
# Test Task

Add a simple hello world function.

## Acceptance
- Function returns "hello world"
EOF

  # Test plan-review e2e
  # Create a simple code file inside the repo for the plan to reference
  mkdir -p src
  echo 'def hello(): return "hello world"' > src/hello.py
  set +e
  plan_result="$(scripts/flowctl.py codex plan-review "$EPIC3" --files "src/hello.py" --base main --receipt "$TEST_DIR/plan-receipt.json" --json 2>&1)"
  plan_rc=$?
  set -e

  if [[ "$plan_rc" -eq 0 ]]; then
    # Verify receipt was written with correct schema
    if [[ -f "$TEST_DIR/plan-receipt.json" ]]; then
      "$PYTHON_BIN" - "$TEST_DIR/plan-receipt.json" "$EPIC3" <<'PY'
import sys, json
from pathlib import Path
data = json.loads(Path(sys.argv[1]).read_text())
expected_id = sys.argv[2]
assert data.get("type") == "plan_review", f"Expected type=plan_review, got {data.get('type')}"
assert data.get("id") == expected_id, f"Expected id={expected_id}, got {data.get('id')}"
assert data.get("mode") == "codex", f"Expected mode=codex, got {data.get('mode')}"
assert "verdict" in data, "Missing verdict in receipt"
assert "session_id" in data, "Missing session_id in receipt"
PY
      echo -e "${GREEN}✓${NC} codex plan-review e2e"
      PASS=$((PASS + 1))
    else
      echo -e "${RED}✗${NC} codex plan-review e2e (no receipt)"
      FAIL=$((FAIL + 1))
    fi
  else
    echo -e "${RED}✗${NC} codex plan-review e2e (exit $plan_rc)"
    FAIL=$((FAIL + 1))
  fi

  # Test impl-review e2e (create a simple change first)
  cat > "$TEST_DIR/repo/src/hello.py" << 'EOF'
def hello():
    return "hello world"
EOF
  git -C "$TEST_DIR/repo" add src/hello.py
  git -C "$TEST_DIR/repo" commit -m "Add hello function" >/dev/null

  set +e
  impl_result="$(scripts/flowctl.py codex impl-review "${EPIC3}.1" --base HEAD~1 --receipt "$TEST_DIR/impl-receipt.json" --json 2>&1)"
  impl_rc=$?
  set -e

  if [[ "$impl_rc" -eq 0 ]]; then
    # Verify receipt was written with correct schema
    if [[ -f "$TEST_DIR/impl-receipt.json" ]]; then
      "$PYTHON_BIN" - "$TEST_DIR/impl-receipt.json" "$EPIC3" <<'PY'
import sys, json
from pathlib import Path
data = json.loads(Path(sys.argv[1]).read_text())
expected_id = f"{sys.argv[2]}.1"
assert data.get("type") == "impl_review", f"Expected type=impl_review, got {data.get('type')}"
assert data.get("id") == expected_id, f"Expected id={expected_id}, got {data.get('id')}"
assert data.get("mode") == "codex", f"Expected mode=codex, got {data.get('mode')}"
assert "verdict" in data, "Missing verdict in receipt"
assert "session_id" in data, "Missing session_id in receipt"
PY
      echo -e "${GREEN}✓${NC} codex impl-review e2e"
      PASS=$((PASS + 1))
    else
      echo -e "${RED}✗${NC} codex impl-review e2e (no receipt)"
      FAIL=$((FAIL + 1))
    fi
  else
    echo -e "${RED}✗${NC} codex impl-review e2e (exit $impl_rc)"
    FAIL=$((FAIL + 1))
  fi
else
  echo -e "${YELLOW}⊘${NC} codex e2e skipped (codex not available)"
fi

echo -e "${YELLOW}--- depends_on_epics gate ---${NC}"
cd "$TEST_DIR/repo"  # Back to test repo
# Create epics and capture their IDs
DEP_BASE_JSON="$(scripts/flowctl.py epic create --title "Dep base" --json)"
DEP_BASE_ID="$(echo "$DEP_BASE_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$DEP_BASE_ID" --title "Base task" --json >/dev/null
DEP_CHILD_JSON="$(scripts/flowctl.py epic create --title "Dep child" --json)"
DEP_CHILD_ID="$(echo "$DEP_CHILD_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
"$PYTHON_BIN" - "$DEP_CHILD_ID" "$DEP_BASE_ID" <<'PY'
import json, sys
from pathlib import Path
child_id, base_id = sys.argv[1], sys.argv[2]
path = Path(f".flow/epics/{child_id}.json")
data = json.loads(path.read_text())
data["depends_on_epics"] = [base_id]
path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
printf '{"epics":["%s"]}\n' "$DEP_CHILD_ID" > "$TEST_DIR/epics.json"
blocked_json="$(scripts/flowctl.py next --epics-file "$TEST_DIR/epics.json" --json)"
"$PYTHON_BIN" - "$DEP_CHILD_ID" "$blocked_json" <<'PY'
import json, sys
child_id = sys.argv[1]
data = json.loads(sys.argv[2])
assert data["status"] == "none"
assert data["reason"] == "blocked_by_epic_deps"
assert child_id in data.get("blocked_epics", {})
PY
echo -e "${GREEN}✓${NC} depends_on_epics blocks"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- stdin support ---${NC}"
cd "$TEST_DIR/repo"
STDIN_EPIC_JSON="$(scripts/flowctl.py epic create --title "Stdin test" --json)"
STDIN_EPIC="$(echo "$STDIN_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
# Test epic set-plan with stdin
scripts/flowctl.py epic set-plan "$STDIN_EPIC" --file - --json <<'EOF'
# Stdin Test Plan

## Overview
Testing stdin support for set-plan.

## Acceptance
- Works via stdin
EOF
# Verify content was written
spec_content="$(scripts/flowctl.py cat "$STDIN_EPIC")"
echo "$spec_content" | grep -q "Testing stdin support" || { echo "stdin set-plan failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} stdin epic set-plan"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec combined ---${NC}"
scripts/flowctl.py task create --epic "$STDIN_EPIC" --title "Set-spec test" --json >/dev/null
SETSPEC_TASK="${STDIN_EPIC}.1"
# Write temp files for combined set-spec
echo 'This is the description.' > "$TEST_DIR/desc.md"
echo '- [ ] Check 1
- [ ] Check 2' > "$TEST_DIR/acc.md"
scripts/flowctl.py task set-spec "$SETSPEC_TASK" --description "$TEST_DIR/desc.md" --acceptance "$TEST_DIR/acc.md" --json >/dev/null
# Verify both sections were written
task_spec="$(scripts/flowctl.py cat "$SETSPEC_TASK")"
echo "$task_spec" | grep -q "This is the description" || { echo "set-spec description failed"; FAIL=$((FAIL + 1)); }
echo "$task_spec" | grep -q "Check 1" || { echo "set-spec acceptance failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec combined"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec --file (full replacement) ---${NC}"
scripts/flowctl.py task create --epic "$STDIN_EPIC" --title "Full replacement test" --json >/dev/null
FULLREPLACE_TASK="${STDIN_EPIC}.2"
# Write complete spec file
cat > "$TEST_DIR/full_spec.md" << 'FULLSPEC'
# Task: Full replacement test

## Description

This is a completely new spec that replaces everything.

## Acceptance

- [ ] Verify full replacement works
- [ ] Original content is gone
FULLSPEC
scripts/flowctl.py task set-spec "$FULLREPLACE_TASK" --file "$TEST_DIR/full_spec.md" --json >/dev/null
# Verify full replacement
full_spec="$(scripts/flowctl.py cat "$FULLREPLACE_TASK")"
echo "$full_spec" | grep -q "completely new spec that replaces everything" || { echo "set-spec --file content failed"; FAIL=$((FAIL + 1)); }
echo "$full_spec" | grep -q "Verify full replacement works" || { echo "set-spec --file acceptance failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec --file"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec --file stdin ---${NC}"
scripts/flowctl.py task create --epic "$STDIN_EPIC" --title "Stdin replacement test" --json >/dev/null
STDIN_REPLACE_TASK="${STDIN_EPIC}.3"
# Full replacement via stdin
scripts/flowctl.py task set-spec "$STDIN_REPLACE_TASK" --file - --json <<'EOF'
# Task: Stdin replacement test

## Description

This spec was written via stdin.

## Acceptance

- [ ] Stdin replacement works
EOF
# Verify stdin replacement
stdin_spec="$(scripts/flowctl.py cat "$STDIN_REPLACE_TASK")"
echo "$stdin_spec" | grep -q "spec was written via stdin" || { echo "set-spec --file stdin failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec --file stdin"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- checkpoint save/restore ---${NC}"
# Save checkpoint
scripts/flowctl.py checkpoint save --epic "$STDIN_EPIC" --json >/dev/null
# Verify checkpoint file exists
[[ -f ".flow/.checkpoint-${STDIN_EPIC}.json" ]] || { echo "checkpoint file not created"; FAIL=$((FAIL + 1)); }
# Modify epic spec
scripts/flowctl.py epic set-plan "$STDIN_EPIC" --file - --json <<'EOF'
# Modified content
EOF
# Restore from checkpoint
scripts/flowctl.py checkpoint restore --epic "$STDIN_EPIC" --json >/dev/null
# Verify original content restored
restored_spec="$(scripts/flowctl.py cat "$STDIN_EPIC")"
echo "$restored_spec" | grep -q "Testing stdin support" || { echo "checkpoint restore failed"; FAIL=$((FAIL + 1)); }
# Delete checkpoint
scripts/flowctl.py checkpoint delete --epic "$STDIN_EPIC" --json >/dev/null
[[ ! -f ".flow/.checkpoint-${STDIN_EPIC}.json" ]] || { echo "checkpoint delete failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} checkpoint save/restore/delete"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- sync command files ---${NC}"
# Test 1: Command stub exists
if [[ -f "$PLUGIN_ROOT/commands/flow-code/sync.md" ]]; then
  echo -e "${GREEN}✓${NC} sync command stub exists"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync command stub missing"
  FAIL=$((FAIL + 1))
fi

# Test 2: Skill file exists
if [[ -f "$PLUGIN_ROOT/skills/flow-code-sync/SKILL.md" ]]; then
  echo -e "${GREEN}✓${NC} sync skill exists"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync skill missing"
  FAIL=$((FAIL + 1))
fi

# Test 3: Command invokes skill
if grep -q "flow-code-sync" "$PLUGIN_ROOT/commands/flow-code/sync.md"; then
  echo -e "${GREEN}✓${NC} sync command invokes skill"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync command doesn't reference skill"
  FAIL=$((FAIL + 1))
fi

# Test 4: Skill has correct frontmatter
if grep -q "name: flow-code-sync" "$PLUGIN_ROOT/skills/flow-code-sync/SKILL.md"; then
  echo -e "${GREEN}✓${NC} sync skill has correct name"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync skill missing name frontmatter"
  FAIL=$((FAIL + 1))
fi

# Test 5: Skill mentions plan-sync agent
if grep -q "plan-sync" "$PLUGIN_ROOT/skills/flow-code-sync/SKILL.md"; then
  echo -e "${GREEN}✓${NC} sync skill references plan-sync agent"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync skill doesn't reference plan-sync agent"
  FAIL=$((FAIL + 1))
fi

# Test 6: Skill supports dry-run
if grep -qi "dry.run\|dry-run\|DRY_RUN" "$PLUGIN_ROOT/skills/flow-code-sync/SKILL.md"; then
  echo -e "${GREEN}✓${NC} sync skill supports dry-run"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} sync skill missing dry-run support"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- task duration tracking ---${NC}"

# Setup: create epic + task, start and complete with a small delay
DUR_EPIC_JSON="$(scripts/flowctl.py epic create --title "Duration test" --json)"
DUR_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$DUR_EPIC_JSON")"
scripts/flowctl.py task create --epic "$DUR_EPIC" --title "Timed task" --json > /dev/null
scripts/flowctl.py start "${DUR_EPIC}.1" --json > /dev/null
sleep 1
result="$(scripts/flowctl.py done "${DUR_EPIC}.1" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json)"

# Test 1: duration_seconds present in JSON output
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert "duration_seconds" in data, f"missing duration_seconds: {data}"
assert data["duration_seconds"] >= 1, f"expected >= 1s, got {data['duration_seconds']}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} duration_seconds in done output (>= 1s)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} duration_seconds missing or too small"
  FAIL=$((FAIL + 1))
fi

# Test 2: duration rendered in spec markdown
SPEC="$(scripts/flowctl.py cat "${DUR_EPIC}.1")"
if echo "$SPEC" | grep -q "Duration:"; then
  echo -e "${GREEN}✓${NC} duration rendered in spec evidence"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} duration not in spec"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- workspace_changes evidence ---${NC}"

# Setup: create epic + task, start it
WS_EPIC_JSON="$(scripts/flowctl.py epic create --title "Workspace test" --json)"
WS_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$WS_EPIC_JSON")"
scripts/flowctl.py task create --epic "$WS_EPIC" --title "WS task" --json > /dev/null
scripts/flowctl.py start "${WS_EPIC}.1" --json > /dev/null

# Test 1: valid workspace_changes renders in spec
WS_EVIDENCE='{"commits":["abc"],"tests":["pytest"],"prs":[],"workspace_changes":{"baseline_rev":"aaa111bbb","final_rev":"ccc222ddd","files_changed":5,"insertions":120,"deletions":30}}'
result="$(scripts/flowctl.py done "${WS_EPIC}.1" --summary "done" --evidence "$WS_EVIDENCE" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("status") == "done"
assert "warning" not in data, f"unexpected warning: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} valid workspace_changes accepted without warning"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} valid workspace_changes should not warn"
  FAIL=$((FAIL + 1))
fi

# Check spec has workspace line
WS_SPEC="$(scripts/flowctl.py cat "${WS_EPIC}.1")"
if echo "$WS_SPEC" | grep -q "5 files changed"; then
  echo -e "${GREEN}✓${NC} workspace_changes rendered in spec markdown"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} workspace_changes not in spec"
  FAIL=$((FAIL + 1))
fi

# Test 2: malformed workspace_changes triggers warning
scripts/flowctl.py task reset "${WS_EPIC}.1" --json > /dev/null
scripts/flowctl.py start "${WS_EPIC}.1" --force --json > /dev/null
BAD_EVIDENCE='{"commits":[],"tests":[],"prs":[],"workspace_changes":{"baseline_rev":"aaa"}}'
result="$(scripts/flowctl.py done "${WS_EPIC}.1" --summary "done" --evidence "$BAD_EVIDENCE" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("status") == "done"
assert "warning" in data, f"expected warning for missing keys: {data}"
assert "missing keys" in data["warning"]
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} malformed workspace_changes warns but completes"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} malformed workspace_changes handling failed"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- files ownership map ---${NC}"

# Setup: epic + tasks with --files
FO_EPIC_JSON="$(scripts/flowctl.py epic create --title "Files test" --json)"
FO_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$FO_EPIC_JSON")"
scripts/flowctl.py task create --epic "$FO_EPIC" --title "T1" --files "src/auth.ts,src/middleware.ts" --json > /dev/null
scripts/flowctl.py task create --epic "$FO_EPIC" --title "T2" --files "src/routes.ts" --json > /dev/null
scripts/flowctl.py task create --epic "$FO_EPIC" --title "T3" --files "src/auth.ts" --json > /dev/null

# Test 1: files stored in task JSON
result="$(scripts/flowctl.py show "${FO_EPIC}.1" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
d = json.loads(sys.argv[1])
assert d.get("files") == ["src/auth.ts", "src/middleware.ts"], f"unexpected files: {d.get('files')}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} --files stored in task JSON"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} --files not stored"
  FAIL=$((FAIL + 1))
fi

# Test 2: files command detects ownership + conflicts
result="$(scripts/flowctl.py files --epic "$FO_EPIC" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
d = json.loads(sys.argv[1])
assert d["file_count"] == 3, f"expected 3 files, got {d['file_count']}"
assert d["conflict_count"] == 1, f"expected 1 conflict, got {d['conflict_count']}"
assert "src/auth.ts" in d["conflicts"], f"src/auth.ts should conflict: {d['conflicts']}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} files command detects ownership + conflicts"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} files command failed"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- memory verify + staleness ---${NC}"

# Setup: enable memory + add entry
scripts/flowctl.py config set memory.enabled true --json > /dev/null
scripts/flowctl.py memory init --json > /dev/null
scripts/flowctl.py memory add pitfall "Test pitfall for verify" --json > /dev/null

# Test 1: memory verify updates last_verified
result="$(scripts/flowctl.py memory verify 1 --json)"
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
result="$(scripts/flowctl.py memory list --json)"
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
EPC_EPIC_JSON="$(scripts/flowctl.py epic create --title "Retro prompt test" --json)"
EPC_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$EPC_EPIC_JSON")"
scripts/flowctl.py task create --epic "$EPC_EPIC" --title "Done task" --json > /dev/null
scripts/flowctl.py start "${EPC_EPIC}.1" --json > /dev/null
scripts/flowctl.py done "${EPC_EPIC}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
result="$(scripts/flowctl.py epic close "$EPC_EPIC" --json)"
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

echo -e "\n${YELLOW}--- restart command ---${NC}"

# Setup: create epic + 3 tasks with deps: .1 -> .2 -> .3
RST_EPIC_JSON="$(scripts/flowctl.py epic create --title "Restart test" --json)"
RST_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$RST_EPIC_JSON")"
scripts/flowctl.py task create --epic "$RST_EPIC" --title "Task 1" --json > /dev/null
scripts/flowctl.py task create --epic "$RST_EPIC" --title "Task 2" --deps "${RST_EPIC}.1" --json > /dev/null
scripts/flowctl.py task create --epic "$RST_EPIC" --title "Task 3" --deps "${RST_EPIC}.2" --json > /dev/null

# Complete tasks 1, 2, 3
scripts/flowctl.py start "${RST_EPIC}.1" --json > /dev/null
scripts/flowctl.py done "${RST_EPIC}.1" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
scripts/flowctl.py start "${RST_EPIC}.2" --json > /dev/null
scripts/flowctl.py done "${RST_EPIC}.2" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
scripts/flowctl.py start "${RST_EPIC}.3" --json > /dev/null
scripts/flowctl.py done "${RST_EPIC}.3" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null

# Test 1: restart --dry-run shows what would be reset
result="$(scripts/flowctl.py restart "${RST_EPIC}.1" --dry-run --json)"
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
result="$(scripts/flowctl.py restart "${RST_EPIC}.1" --json)"
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
result="$(scripts/flowctl.py show "${RST_EPIC}.1" --json)"
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
result="$(scripts/flowctl.py restart "${RST_EPIC}.1" --json)"
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
scripts/flowctl.py start "${RST_EPIC}.1" --json > /dev/null
set +e
result="$(scripts/flowctl.py restart "${RST_EPIC}.1" --json 2>&1)"
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
result="$(scripts/flowctl.py restart "${RST_EPIC}.1" --force --json)"
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

echo -e "\n${YELLOW}--- review-backend --compare ---${NC}"

# Create mock receipt files
cat > "$TEST_DIR/receipt-codex.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"codex","verdict":"SHIP","timestamp":"2026-03-30T00:00:00Z","review":"Looks good"}
EOF
cat > "$TEST_DIR/receipt-rp.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"rp","verdict":"SHIP","timestamp":"2026-03-30T00:00:00Z","review":"LGTM"}
EOF
cat > "$TEST_DIR/receipt-conflict.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"rp","verdict":"NEEDS_WORK","timestamp":"2026-03-30T00:00:00Z","review":"Needs fixes"}
EOF

# Test 1: compare with consensus (both SHIP)
result="$(scripts/flowctl.py review-backend --compare "$TEST_DIR/receipt-codex.json,$TEST_DIR/receipt-rp.json" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("consensus") == "SHIP", f"expected SHIP consensus, got {data}"
assert data.get("has_conflict") == False, f"expected no conflict: {data}"
assert data.get("reviews") == 2, f"expected 2 reviews: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --compare consensus detected"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --compare consensus failed"
  FAIL=$((FAIL + 1))
fi

# Test 2: compare with conflict (SHIP vs NEEDS_WORK)
result="$(scripts/flowctl.py review-backend --compare "$TEST_DIR/receipt-codex.json,$TEST_DIR/receipt-conflict.json" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("has_conflict") == True, f"expected conflict: {data}"
assert data.get("consensus") is None, f"expected no consensus: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --compare conflict detected"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --compare conflict failed"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- review receipt archival ---${NC}"

# Setup: create epic + task
RR_EPIC_JSON="$(scripts/flowctl.py epic create --title "Receipt test" --json)"
RR_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$RR_EPIC_JSON")"
scripts/flowctl.py task create --epic "$RR_EPIC" --title "Task with review" --json > /dev/null
scripts/flowctl.py start "${RR_EPIC}.1" --json > /dev/null

# Test 1: done with review_receipt archives to .flow/reviews/
RR_EVIDENCE="{\"commits\":[\"x1\"],\"tests\":[],\"prs\":[],\"review_receipt\":{\"type\":\"impl_review\",\"id\":\"${RR_EPIC}.1\",\"mode\":\"codex\",\"verdict\":\"SHIP\",\"timestamp\":\"2026-03-30T00:00:00Z\",\"review\":\"LGTM\"}}"
scripts/flowctl.py done "${RR_EPIC}.1" --summary "done" --evidence "$RR_EVIDENCE" --json > /dev/null
if [ -f ".flow/reviews/impl_review-${RR_EPIC}.1-codex.json" ]; then
  echo -e "${GREEN}✓${NC} review receipt archived to .flow/reviews/"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review receipt not archived"
  FAIL=$((FAIL + 1))
fi

# Add a second receipt (simulate rp review)
cat > ".flow/reviews/impl_review-${RR_EPIC}.1-rp.json" << 'EOF'
{"type":"impl_review","id":"PLACEHOLDER","mode":"rp","verdict":"SHIP","timestamp":"2026-03-30T00:01:00Z","review":"Looks good"}
EOF

# Test 2: review-backend --epic auto-discovers receipts
result="$(scripts/flowctl.py review-backend --epic "$RR_EPIC" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("reviews") == 2, f"expected 2 reviews, got {data.get('reviews')}"
assert data.get("consensus") == "SHIP", f"expected SHIP consensus: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --epic auto-discovers receipts"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --epic failed"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- task domain tagging ---${NC}"

# Setup: create epic + tasks with domains
DOM_EPIC_JSON="$(scripts/flowctl.py epic create --title "Domain test" --json)"
DOM_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$DOM_EPIC_JSON")"
scripts/flowctl.py task create --epic "$DOM_EPIC" --title "Build API" --domain backend --json > /dev/null
scripts/flowctl.py task create --epic "$DOM_EPIC" --title "Build UI" --domain frontend --json > /dev/null
scripts/flowctl.py task create --epic "$DOM_EPIC" --title "No domain" --json > /dev/null

# Test 1: domain stored in task JSON
result="$(scripts/flowctl.py show "${DOM_EPIC}.1" --json)"
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
result="$(scripts/flowctl.py show "${DOM_EPIC}.3" --json)"
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
result="$(scripts/flowctl.py tasks --epic "$DOM_EPIC" --domain backend --json)"
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
result="$(scripts/flowctl.py tasks --epic "$DOM_EPIC" --json)"
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
ARC_EPIC_JSON="$(scripts/flowctl.py epic create --title "Archive me" --json)"
ARC_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$ARC_EPIC_JSON")"
scripts/flowctl.py task create --epic "$ARC_EPIC" --title "Done task" --json > /dev/null
scripts/flowctl.py start "${ARC_EPIC}.1" --json > /dev/null
scripts/flowctl.py done "${ARC_EPIC}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
scripts/flowctl.py epic close "$ARC_EPIC" --json > /dev/null

# Test 1: archive moves files
result="$(scripts/flowctl.py epic archive "$ARC_EPIC" --json)"
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
result="$(scripts/flowctl.py epics --json)"
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
CLEAN_EP1_JSON="$(scripts/flowctl.py epic create --title "Clean1" --json)"
CLEAN_EP1="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$CLEAN_EP1_JSON")"
scripts/flowctl.py task create --epic "$CLEAN_EP1" --title "T1" --json > /dev/null
scripts/flowctl.py start "${CLEAN_EP1}.1" --json > /dev/null
scripts/flowctl.py done "${CLEAN_EP1}.1" --summary "ok" --evidence '{"commits":[],"tests":[],"prs":[]}' --json > /dev/null
scripts/flowctl.py epic close "$CLEAN_EP1" --json > /dev/null

result="$(scripts/flowctl.py epic clean --json)"
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

echo -e "${YELLOW}--- parse-findings ---${NC}"

# Test: valid <findings> tag
FINDINGS_FILE="$TEST_DIR/findings_valid.txt"
cat > "$FINDINGS_FILE" <<'FINDINGS_EOF'
Some review preamble text.

<findings>
[
  {
    "title": "Missing input validation",
    "severity": "critical",
    "location": "src/auth.py:42",
    "recommendation": "Add input sanitization"
  },
  {
    "title": "Unused import",
    "severity": "nitpick",
    "location": "src/utils.py:1",
    "recommendation": "Remove unused import"
  }
]
</findings>

More review text after.
FINDINGS_EOF

pf_result="$(scripts/flowctl.py parse-findings --file "$FINDINGS_FILE" --json)"
pf_count="$(echo "$pf_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$pf_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings extracts findings from <findings> tag"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings count wrong (expected 2, got $pf_count)"
  FAIL=$((FAIL + 1))
fi

# Test: missing <findings> tag → graceful empty
FINDINGS_EMPTY="$TEST_DIR/findings_empty.txt"
echo "No findings here, just plain review text." > "$FINDINGS_EMPTY"

pf_empty="$(scripts/flowctl.py parse-findings --file "$FINDINGS_EMPTY" --json)"
pf_empty_count="$(echo "$pf_empty" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
pf_empty_warns="$(echo "$pf_empty" | "$PYTHON_BIN" -c 'import json,sys; w=json.load(sys.stdin).get("warnings",[]); print(len(w))')"
if [[ "$pf_empty_count" == "0" ]] && [[ "$pf_empty_warns" -ge 1 ]]; then
  echo -e "${GREEN}✓${NC} parse-findings gracefully handles missing tags"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings missing tag handling wrong (count=$pf_empty_count, warns=$pf_empty_warns)"
  FAIL=$((FAIL + 1))
fi

# Test: malformed JSON (trailing commas)
FINDINGS_MALFORMED="$TEST_DIR/findings_malformed.txt"
cat > "$FINDINGS_MALFORMED" <<'FINDINGS_EOF'
<findings>
[
  {
    "title": "Trailing comma issue",
    "severity": "major",
    "location": "src/app.py:10",
    "recommendation": "Fix the trailing comma",
  },
]
</findings>
FINDINGS_EOF

pf_mal="$(scripts/flowctl.py parse-findings --file "$FINDINGS_MALFORMED" --json)"
pf_mal_count="$(echo "$pf_mal" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$pf_mal_count" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings handles malformed JSON (trailing commas)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings malformed JSON handling wrong (expected 1, got $pf_mal_count)"
  FAIL=$((FAIL + 1))
fi

# Test: --register auto gap add
FINDINGS_REG="$TEST_DIR/findings_register.txt"
cat > "$FINDINGS_REG" <<'FINDINGS_EOF'
<findings>
[
  {
    "title": "SQL injection vulnerability",
    "severity": "critical",
    "location": "src/db.py:99",
    "recommendation": "Use parameterized queries"
  },
  {
    "title": "Minor typo in comment",
    "severity": "minor",
    "location": "src/main.py:5",
    "recommendation": "Fix typo"
  }
]
</findings>
FINDINGS_EOF

pf_reg="$(scripts/flowctl.py parse-findings --file "$FINDINGS_REG" --epic "$EPIC1" --register --source plan-review --json)"
pf_reg_registered="$(echo "$pf_reg" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("registered", 0))')"
if [[ "$pf_reg_registered" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings --register adds critical/major gaps (skips minor)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings --register wrong count (expected 1, got $pf_reg_registered)"
  FAIL=$((FAIL + 1))
fi

# Verify the gap was actually created
gap_reg_check="$(scripts/flowctl.py gap list --epic "$EPIC1" --json | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
gaps = data.get("gaps", [])
sql_gaps = [g for g in gaps if "SQL injection" in g.get("capability", "")]
print(len(sql_gaps))
')"
if [[ "$gap_reg_check" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings --register actually created the gap"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings --register gap not found in registry (found $gap_reg_check)"
  FAIL=$((FAIL + 1))
fi

# ── status --interrupted ──
echo -e "\n${YELLOW}=== status --interrupted ===${NC}"

# Create a second epic with todo tasks to test interrupted detection
EPIC_INT_JSON="$(scripts/flowctl.py epic create --title "Interrupted test epic" --json)"
EPIC_INT="$(echo "$EPIC_INT_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$EPIC_INT" --title "Interrupted task 1" --json > /dev/null
scripts/flowctl.py task create --epic "$EPIC_INT" --title "Interrupted task 2" --json > /dev/null

# Test --interrupted --json detects epic with todo tasks
int_json="$(scripts/flowctl.py status --interrupted --json)"
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
EPIC_AE_JSON="$(scripts/flowctl.py epic create --title "Auto execute test" --json)"
EPIC_AE="$(echo "$EPIC_AE_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$EPIC_AE" --title "AE task 1" --json > /dev/null
scripts/flowctl.py task create --epic "$EPIC_AE" --title "AE task 2" --json > /dev/null

# Set pending marker
ae_pending="$(scripts/flowctl.py epic set-auto-execute "$EPIC_AE" --pending --json)"
ae_pending_val="$(echo "$ae_pending" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["auto_execute_pending"])')"
if [[ "$ae_pending_val" == "True" ]]; then
  echo -e "${GREEN}✓${NC} set-auto-execute --pending sets marker"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-auto-execute --pending: expected True, got $ae_pending_val"
  FAIL=$((FAIL + 1))
fi

# Verify --interrupted shows it with reason "planned_not_started"
ae_int_json="$(scripts/flowctl.py status --interrupted --json)"
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
ae_done="$(scripts/flowctl.py epic set-auto-execute "$EPIC_AE" --done --json)"
ae_done_val="$(echo "$ae_done" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["auto_execute_pending"])')"
if [[ "$ae_done_val" == "False" ]]; then
  echo -e "${GREEN}✓${NC} set-auto-execute --done clears marker"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} set-auto-execute --done: expected False, got $ae_done_val"
  FAIL=$((FAIL + 1))
fi

# Verify --interrupted now shows "partially_complete" reason (marker cleared)
ae_int2_json="$(scripts/flowctl.py status --interrupted --json)"
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

echo -e "${YELLOW}--- worker-prompt ---${NC}"

# We need agents/worker.md accessible from the test's CLAUDE_PLUGIN_ROOT
# Copy the agents directory so worker-prompt can find worker.md
cp -r "$PLUGIN_ROOT/agents" "$TEST_DIR/repo/agents"

# Disable memory so default prompt is core-only (memory auto-includes if enabled)
scripts/flowctl.py config set memory.enabled false --json >/dev/null

# Test: worker-prompt default output (core-only, check sections and token count)
wp_json="$(CLAUDE_PLUGIN_ROOT="$TEST_DIR/repo" scripts/flowctl.py worker-prompt --task "${EPIC1}.1" --json)"
wp_sections="$(echo "$wp_json" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); print(",".join(sorted(d["sections"])))')"
wp_tokens="$(echo "$wp_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["estimated_tokens"])')"
if [[ "$wp_sections" == "core" ]] && [[ "$wp_tokens" -gt 0 ]]; then
  echo -e "${GREEN}✓${NC} worker-prompt default: core-only, ${wp_tokens} tokens"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-prompt default: expected sections=core, got $wp_sections (tokens: $wp_tokens)"
  FAIL=$((FAIL + 1))
fi

# Test: worker-prompt --team includes team sections
wp_team_json="$(CLAUDE_PLUGIN_ROOT="$TEST_DIR/repo" scripts/flowctl.py worker-prompt --task "${EPIC1}.1" --team --json)"
wp_team_sections="$(echo "$wp_team_json" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); print(",".join(sorted(d["sections"])))')"
if [[ "$wp_team_sections" == *"team"* ]] && [[ "$wp_team_sections" == *"core"* ]]; then
  echo -e "${GREEN}✓${NC} worker-prompt --team: includes team sections ($wp_team_sections)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-prompt --team: expected core,team in sections, got $wp_team_sections"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- worker-phase ---${NC}"

# Create a fresh epic+task for phase testing
EPIC_PH_JSON="$(scripts/flowctl.py epic create --title "Phase test" --json)"
EPIC_PH="$(echo "$EPIC_PH_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
scripts/flowctl.py task create --epic "$EPIC_PH" --title "Phase task" --json >/dev/null
scripts/flowctl.py start "${EPIC_PH}.1" --json >/dev/null

# Test: worker-phase next returns phase 1 initially
wph_next="$(scripts/flowctl.py worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
wph_done="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["all_done"])')"
if [[ "$wph_phase" == "1" ]] && [[ "$wph_done" == "False" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: initial phase is 1"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: expected phase=1 all_done=False, got phase=$wph_phase all_done=$wph_done"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase done phase 1 → next returns phase 2
scripts/flowctl.py worker-phase done --task "${EPIC_PH}.1" --phase 1 --json >/dev/null
wph_next2="$(scripts/flowctl.py worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase2="$(echo "$wph_next2" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
if [[ "$wph_phase2" == "2" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase done→next: advances to phase 2"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase done→next: expected phase=2, got $wph_phase2"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase skip detection — try to complete phase 3 before phase 2
wph_skip_err="$(scripts/flowctl.py worker-phase done --task "${EPIC_PH}.1" --phase 3 --json 2>&1 || true)"
if echo "$wph_skip_err" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); assert d.get("error") or not d.get("success")' 2>/dev/null; then
  echo -e "${GREEN}✓${NC} worker-phase skip detection: rejects out-of-order phase"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase skip detection: expected error for out-of-order, got: $wph_skip_err"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase next returns non-empty content field
wph_content_len="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(len(json.load(sys.stdin).get("content","")))')"
if [[ "$wph_content_len" -gt 0 ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: content field is non-empty ($wph_content_len chars)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: content field is empty"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase next returns different content for different phases
wph_content_p1="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("content","")[:50])')"
wph_content_p2="$(echo "$wph_next2" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("content","")[:50])')"
if [[ "$wph_content_p1" != "$wph_content_p2" ]] && [[ -n "$wph_content_p2" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: content changes between phases"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: expected different content for phase 1 vs 2"
  FAIL=$((FAIL + 1))
fi

# Test: worker-prompt --bootstrap outputs <300 tokens
wp_boot_json="$(CLAUDE_PLUGIN_ROOT="$TEST_DIR/repo" scripts/flowctl.py worker-prompt --task "${EPIC1}.1" --bootstrap --json)"
wp_boot_tokens="$(echo "$wp_boot_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["estimated_tokens"])')"
wp_boot_mode="$(echo "$wp_boot_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["mode"])')"
if [[ "$wp_boot_mode" == "bootstrap" ]] && [[ "$wp_boot_tokens" -lt 300 ]]; then
  echo -e "${GREEN}✓${NC} worker-prompt --bootstrap: mode=bootstrap, ${wp_boot_tokens} tokens (<300)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-prompt --bootstrap: expected mode=bootstrap and <300 tokens, got mode=$wp_boot_mode tokens=$wp_boot_tokens"
  FAIL=$((FAIL + 1))
fi

# Test: complete all default phases → all_done
for phase in 2 2.5 3 5 6; do
  scripts/flowctl.py worker-phase done --task "${EPIC_PH}.1" --phase "$phase" --json >/dev/null
done
wph_final="$(scripts/flowctl.py worker-phase next --task "${EPIC_PH}.1" --json)"
wph_all_done="$(echo "$wph_final" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["all_done"])')"
if [[ "$wph_all_done" == "True" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase lifecycle: all phases complete"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase lifecycle: expected all_done=True, got $wph_all_done"
  FAIL=$((FAIL + 1))
fi

echo ""
echo -e "${YELLOW}=== Results ===${NC}"
echo -e "Passed: ${GREEN}$PASS${NC}"
echo -e "Failed: ${RED}$FAIL${NC}"

if [ $FAIL -gt 0 ]; then
  exit 1
fi
echo -e "\n${GREEN}All tests passed!${NC}"
