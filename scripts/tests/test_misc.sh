#!/usr/bin/env bash
# Tests: schema validate, codex commands, depends_on_epics, stdin support,
#        task set-spec, checkpoint, sync command files
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== misc tests ===${NC}"

echo -e "${YELLOW}--- schema v1 validate ---${NC}"
"$PYTHON_BIN" - <<'PY'
import json
from pathlib import Path
path = Path(".flow/meta.json")
data = json.loads(path.read_text())
data["schema_version"] = 1
path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
PY
$FLOWCTL validate --all --json >/dev/null
echo -e "${GREEN}✓${NC} schema v1 validate"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- codex commands ---${NC}"
# Test codex check (may or may not have codex installed)
codex_check_json="$($FLOWCTL codex check --json 2>/dev/null || echo '{"success":true}')"
"$PYTHON_BIN" - <<'PY' "$codex_check_json"
import json, sys
data = json.loads(sys.argv[1])
assert data["success"] == True, f"codex check failed: {data}"
PY
echo -e "${GREEN}✓${NC} codex check"
PASS=$((PASS + 1))

# Test codex impl-review help (no codex required for argparse check)
set +e
$FLOWCTL codex impl-review --help >/dev/null 2>&1
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
$FLOWCTL codex plan-review --help >/dev/null 2>&1
rc=$?
set -e
if [[ "$rc" -eq 0 ]]; then
  echo -e "${GREEN}✓${NC} codex plan-review --help"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} codex plan-review --help"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- depends_on_epics gate ---${NC}"
# Create epics and capture their IDs
DEP_BASE_JSON="$($FLOWCTL epic create --title "Dep base" --json)"
DEP_BASE_ID="$(echo "$DEP_BASE_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$DEP_BASE_ID" --title "Base task" --json >/dev/null
DEP_CHILD_JSON="$($FLOWCTL epic create --title "Dep child" --json)"
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
blocked_json="$($FLOWCTL next --epics-file "$TEST_DIR/epics.json" --json)"
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
STDIN_EPIC_JSON="$($FLOWCTL epic create --title "Stdin test" --json)"
STDIN_EPIC="$(echo "$STDIN_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
# Test epic set-plan with stdin
$FLOWCTL epic plan "$STDIN_EPIC" --file - --json <<'EOF'
# Stdin Test Plan

## Overview
Testing stdin support for set-plan.

## Acceptance
- Works via stdin
EOF
# Verify content was written
spec_content="$($FLOWCTL cat "$STDIN_EPIC")"
echo "$spec_content" | grep -q "Testing stdin support" || { echo "stdin set-plan failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} stdin epic set-plan"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec combined ---${NC}"
$FLOWCTL task create --epic "$STDIN_EPIC" --title "Set-spec test" --json >/dev/null
SETSPEC_TASK="${STDIN_EPIC}.1"
# Write temp files for combined set-spec
echo 'This is the description.' > "$TEST_DIR/desc.md"
echo '- [ ] Check 1
- [ ] Check 2' > "$TEST_DIR/acc.md"
$FLOWCTL task set-spec "$SETSPEC_TASK" --description "$TEST_DIR/desc.md" --acceptance "$TEST_DIR/acc.md" --json >/dev/null
# Verify both sections were written
task_spec="$($FLOWCTL cat "$SETSPEC_TASK")"
echo "$task_spec" | grep -q "This is the description" || { echo "set-spec description failed"; FAIL=$((FAIL + 1)); }
echo "$task_spec" | grep -q "Check 1" || { echo "set-spec acceptance failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec combined"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec --file (full replacement) ---${NC}"
$FLOWCTL task create --epic "$STDIN_EPIC" --title "Full replacement test" --json >/dev/null
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
$FLOWCTL task set-spec "$FULLREPLACE_TASK" --file "$TEST_DIR/full_spec.md" --json >/dev/null
# Verify full replacement
full_spec="$($FLOWCTL cat "$FULLREPLACE_TASK")"
echo "$full_spec" | grep -q "completely new spec that replaces everything" || { echo "set-spec --file content failed"; FAIL=$((FAIL + 1)); }
echo "$full_spec" | grep -q "Verify full replacement works" || { echo "set-spec --file acceptance failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec --file"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- task set-spec --file stdin ---${NC}"
$FLOWCTL task create --epic "$STDIN_EPIC" --title "Stdin replacement test" --json >/dev/null
STDIN_REPLACE_TASK="${STDIN_EPIC}.3"
# Full replacement via stdin
$FLOWCTL task set-spec "$STDIN_REPLACE_TASK" --file - --json <<'EOF'
# Task: Stdin replacement test

## Description

This spec was written via stdin.

## Acceptance

- [ ] Stdin replacement works
EOF
# Verify stdin replacement
stdin_spec="$($FLOWCTL cat "$STDIN_REPLACE_TASK")"
echo "$stdin_spec" | grep -q "spec was written via stdin" || { echo "set-spec --file stdin failed"; FAIL=$((FAIL + 1)); }
echo -e "${GREEN}✓${NC} task set-spec --file stdin"
PASS=$((PASS + 1))

echo -e "${YELLOW}--- checkpoint save/restore ---${NC}"
# Save checkpoint
$FLOWCTL checkpoint save --epic "$STDIN_EPIC" --json >/dev/null
# Verify checkpoint file exists
[[ -f ".flow/.checkpoint-${STDIN_EPIC}.json" ]] || { echo "checkpoint file not created"; FAIL=$((FAIL + 1)); }
# Modify epic spec
$FLOWCTL epic plan "$STDIN_EPIC" --file - --json <<'EOF'
# Modified content
EOF
# Restore from checkpoint
$FLOWCTL checkpoint restore --epic "$STDIN_EPIC" --json >/dev/null
# Verify original content restored
restored_spec="$($FLOWCTL cat "$STDIN_EPIC")"
echo "$restored_spec" | grep -q "Testing stdin support" || { echo "checkpoint restore failed"; FAIL=$((FAIL + 1)); }
# Delete checkpoint
$FLOWCTL checkpoint delete --epic "$STDIN_EPIC" --json >/dev/null
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

print_results
