#!/usr/bin/env bash
# Tests: plan_review_status, branch_name, epic set-title, block/validate/epic close,
#        duration tracking, workspace_changes evidence
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== lifecycle tests ===${NC}"

# Create initial epic for plan_review/branch tests
EPIC1_JSON="$($FLOWCTL epic create --title "Epic One" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC1" --title "Task 1" --json >/dev/null
$FLOWCTL task create --epic "$EPIC1" --title "Task 2" --json >/dev/null

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
show_json="$($FLOWCTL show "$EPIC1" --json)"
"$PYTHON_BIN" - <<'PY' "$show_json"
import json, sys
data = json.loads(sys.argv[1])
assert data.get("plan_review_status") is None or data.get("plan_review_status") == "unknown"
assert data.get("plan_reviewed_at") is None
assert data.get("branch_name") is None
PY
echo -e "${GREEN}✓${NC} plan_review_status defaulted"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- branch_name set ---${NC}"
$FLOWCTL epic branch "$EPIC1" "${EPIC1}-epic" --json >/dev/null
show_json="$($FLOWCTL show "$EPIC1" --json)"
if "$PYTHON_BIN" - "$show_json" "$EPIC1" <<'PY' 2>/dev/null
import json, sys
data = json.loads(sys.argv[1])
expected_branch = f"{sys.argv[2]}-epic"
assert data.get("branch_name") == expected_branch, f"Expected {expected_branch}, got {data.get('branch_name')}"
PY
then
  echo -e "${GREEN}✓${NC} branch_name set"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} branch_name set: show does not return branch_name (DB-only field)"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- epic set-title ---${NC}"
# Create epic with tasks for rename test
RENAME_EPIC_JSON="$($FLOWCTL epic create --title "Old Title" --json)"
RENAME_EPIC="$(echo "$RENAME_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$RENAME_EPIC" --title "First task" --json >/dev/null
$FLOWCTL task create --epic "$RENAME_EPIC" --title "Second task" --json >/dev/null
# Add task dependency within epic
$FLOWCTL dep add "${RENAME_EPIC}.2" "${RENAME_EPIC}.1" --json >/dev/null

# Rename epic
rename_result="$($FLOWCTL epic title "$RENAME_EPIC" --title "New Shiny Title" --json)"
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
show_json="$($FLOWCTL show "$NEW_EPIC" --json)"
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
DEP_EPIC_JSON="$($FLOWCTL epic create --title "Depends on renamed" --json)"
DEP_EPIC="$(echo "$DEP_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL epic add-dep "$DEP_EPIC" "$NEW_EPIC" --json >/dev/null
# Rename the dependency
rename2_result="$($FLOWCTL epic title "$NEW_EPIC" --title "Final Title" --json)"
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
EPIC2_JSON="$($FLOWCTL epic create --title "Epic Two" --json)"
EPIC2="$(echo "$EPIC2_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC2" --title "Block me" --json >/dev/null
$FLOWCTL task create --epic "$EPIC2" --title "Other" --json >/dev/null
printf "Blocked by test\n" > "$TEST_DIR/reason.md"
$FLOWCTL block "${EPIC2}.1" --reason-file "$TEST_DIR/reason.md" --json >/dev/null
$FLOWCTL validate --epic "$EPIC2" --json >/dev/null
echo -e "${GREEN}✓${NC} validate allows blocked"
PASS=$((PASS + 1))

set +e
$FLOWCTL epic close "$EPIC2" --json >/dev/null
rc=$?
set -e
if [[ "$rc" -ne 0 ]]; then
  echo -e "${GREEN}✓${NC} epic close fails when blocked"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} epic close fails when blocked"
  FAIL=$((FAIL + 1))
fi

$FLOWCTL start "${EPIC2}.1" --force --json >/dev/null
$FLOWCTL done "${EPIC2}.1" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
$FLOWCTL start "${EPIC2}.2" --json >/dev/null
$FLOWCTL done "${EPIC2}.2" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
$FLOWCTL epic close "$EPIC2" --json >/dev/null
echo -e "${GREEN}✓${NC} epic close succeeds when done"
PASS=$((PASS + 1))

echo -e "\n${YELLOW}--- task duration tracking ---${NC}"

# Setup: create epic + task, start and complete with a small delay
DUR_EPIC_JSON="$($FLOWCTL epic create --title "Duration test" --json)"
DUR_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$DUR_EPIC_JSON")"
$FLOWCTL task create --epic "$DUR_EPIC" --title "Timed task" --json > /dev/null
$FLOWCTL start "${DUR_EPIC}.1" --json > /dev/null
sleep 1
result="$($FLOWCTL done "${DUR_EPIC}.1" --summary "done" --evidence '{"commits":[],"tests":[],"prs":[]}' --json)"

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
SPEC="$($FLOWCTL cat "${DUR_EPIC}.1")"
if echo "$SPEC" | grep -q "Duration:"; then
  echo -e "${GREEN}✓${NC} duration rendered in spec evidence"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} duration not in spec"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- workspace_changes evidence ---${NC}"

# Setup: create epic + task, start it
WS_EPIC_JSON="$($FLOWCTL epic create --title "Workspace test" --json)"
WS_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$WS_EPIC_JSON")"
$FLOWCTL task create --epic "$WS_EPIC" --title "WS task" --json > /dev/null
$FLOWCTL start "${WS_EPIC}.1" --json > /dev/null

# Test 1: valid workspace_changes renders in spec
WS_EVIDENCE='{"commits":["abc"],"tests":["pytest"],"prs":[],"workspace_changes":{"baseline_rev":"aaa111bbb","final_rev":"ccc222ddd","files_changed":5,"insertions":120,"deletions":30}}'
result="$($FLOWCTL done "${WS_EPIC}.1" --summary "done" --evidence "$WS_EVIDENCE" --json)"
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
WS_SPEC="$($FLOWCTL cat "${WS_EPIC}.1")"
if echo "$WS_SPEC" | grep -q "5 files changed"; then
  echo -e "${GREEN}✓${NC} workspace_changes rendered in spec markdown"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} workspace_changes not in spec"
  FAIL=$((FAIL + 1))
fi

# Test 2: malformed workspace_changes triggers warning
$FLOWCTL task reset "${WS_EPIC}.1" --json > /dev/null
$FLOWCTL start "${WS_EPIC}.1" --force --json > /dev/null
BAD_EVIDENCE='{"commits":[],"tests":[],"prs":[],"workspace_changes":{"baseline_rev":"aaa"}}'
result="$($FLOWCTL done "${WS_EPIC}.1" --summary "done" --evidence "$BAD_EVIDENCE" --json)"
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

print_results
