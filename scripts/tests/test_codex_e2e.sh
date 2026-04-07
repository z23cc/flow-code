#!/usr/bin/env bash
# Tests: codex plan-review and impl-review end-to-end (requires codex CLI)
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== codex e2e tests ===${NC}"

echo -e "${YELLOW}--- codex e2e (requires codex CLI) ---${NC}"
# Check if codex is available (handles its own auth)
codex_available="$($FLOWCTL codex check --json 2>/dev/null | "$PYTHON_BIN" -c "import sys,json; print(json.load(sys.stdin).get('available', False))" 2>/dev/null || echo "False")"
if [[ "$codex_available" == "True" ]]; then
  # Create a simple epic + task for testing
  EPIC3_JSON="$($FLOWCTL epic create --title "Codex test epic" --json)"
  EPIC3="$(echo "$EPIC3_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
  $FLOWCTL task create --epic "$EPIC3" --title "Test task" --json >/dev/null

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
  mkdir -p src
  echo 'def hello(): return "hello world"' > src/hello.py
  set +e
  plan_result="$($FLOWCTL codex plan-review "$EPIC3" --files "src/hello.py" --base main --receipt "$TEST_DIR/plan-receipt.json" --json 2>&1)"
  plan_rc=$?
  set -e

  if [[ "$plan_rc" -eq 0 ]]; then
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

  # Test impl-review e2e
  cat > "$TEST_DIR/repo/src/hello.py" << 'EOF'
def hello():
    return "hello world"
EOF
  git -C "$TEST_DIR/repo" add src/hello.py
  git -C "$TEST_DIR/repo" commit -m "Add hello function" >/dev/null

  set +e
  impl_result="$($FLOWCTL codex impl-review "${EPIC3}.1" --base HEAD~1 --receipt "$TEST_DIR/impl-receipt.json" --json 2>&1)"
  impl_rc=$?
  set -e

  if [[ "$impl_rc" -eq 0 ]]; then
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

print_results
