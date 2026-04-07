#!/usr/bin/env bash
# Tests: file ownership map, lock/unlock
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== files tests ===${NC}"

echo -e "${YELLOW}--- files ownership map ---${NC}"

# Setup: epic + tasks with --files
FO_EPIC_JSON="$($FLOWCTL epic create --title "Files test" --json)"
FO_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$FO_EPIC_JSON")"
$FLOWCTL task create --epic "$FO_EPIC" --title "T1" --files "src/auth.ts,src/middleware.ts" --json > /dev/null
$FLOWCTL task create --epic "$FO_EPIC" --title "T2" --files "src/routes.ts" --json > /dev/null
$FLOWCTL task create --epic "$FO_EPIC" --title "T3" --files "src/auth.ts" --json > /dev/null

# Test 1: files stored in task JSON
result="$($FLOWCTL show "${FO_EPIC}.1" --json)"
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
result="$($FLOWCTL files --epic "$FO_EPIC" --json)"
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

print_results
