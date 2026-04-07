#!/usr/bin/env bash
# Tests: context hints, build_review_prompt, worker-prompt, worker-phase
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== worker tests ===${NC}"

# Create epic + tasks for worker tests
EPIC1_JSON="$($FLOWCTL epic create --title "Worker Epic" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC1" --title "Task 1" --json >/dev/null

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
hints_output="$(PYTHONPATH="$PLUGIN_ROOT/scripts" "$PYTHON_BIN" -c "
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
cd "$TEST_DIR/repo"
# Test that build_review_prompt generates proper structure
"$PYTHON_BIN" - "$PLUGIN_ROOT/scripts" <<'PY'
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

echo -e "${YELLOW}--- worker-prompt ---${NC}"

# Copy agents directory so worker-phase can find worker.md
cp -r "$PLUGIN_ROOT/agents" "$TEST_DIR/repo/agents"

# Test: worker-prompt default output (bootstrap mode)
wp_json="$(CLAUDE_PLUGIN_ROOT="$TEST_DIR/repo" $FLOWCTL worker-prompt --task "${EPIC1}.1" --json)"
wp_mode="$(echo "$wp_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["mode"])')"
wp_tokens="$(echo "$wp_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["estimated_tokens"])')"
if [[ "$wp_mode" == "bootstrap" ]] && [[ "$wp_tokens" -gt 0 ]] && [[ "$wp_tokens" -lt 300 ]]; then
  echo -e "${GREEN}✓${NC} worker-prompt default: bootstrap mode, ${wp_tokens} tokens (<300)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-prompt default: expected mode=bootstrap and <300 tokens, got mode=$wp_mode tokens=$wp_tokens"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- worker-phase ---${NC}"

# Create a fresh epic+task for phase testing
EPIC_PH_JSON="$($FLOWCTL epic create --title "Phase test" --json)"
EPIC_PH="$(echo "$EPIC_PH_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC_PH" --title "Phase task" --json >/dev/null
$FLOWCTL start "${EPIC_PH}.1" --json >/dev/null

# Test: worker-phase next returns phase 1 initially (worktree+teams default)
wph_next="$($FLOWCTL worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
wph_done="$(echo "$wph_next" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["all_done"])')"
if [[ "$wph_phase" == "1" ]] && [[ "$wph_done" == "False" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: initial phase is 1 (worktree+teams default)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: expected phase=1 all_done=False, got phase=$wph_phase all_done=$wph_done"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase done phase 1 -> next returns phase 2
wph_next1="$wph_next"
$FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase 1 --json >/dev/null
wph_next1b="$($FLOWCTL worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase1b="$(echo "$wph_next1b" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
if [[ "$wph_phase1b" == "2" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase done->next: advances to phase 2"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase done->next: expected phase=2, got $wph_phase1b"
  FAIL=$((FAIL + 1))
fi

# Advance through phase 2 and 5 to test 6
$FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase 2 --json >/dev/null
$FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase 5 --json >/dev/null
wph_next6="$($FLOWCTL worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase6="$(echo "$wph_next6" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
if [[ "$wph_phase6" == "6" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase done->next: advances to phase 6"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase done->next: expected phase=6, got $wph_phase6"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase skip detection — try to complete phase 10 before phase 6
wph_skip_err="$($FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase 10 --json 2>&1 || true)"
if echo "$wph_skip_err" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); assert d.get("error") or not d.get("success")' 2>/dev/null; then
  echo -e "${GREEN}✓${NC} worker-phase skip detection: rejects out-of-order phase"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase skip detection: expected error for out-of-order, got: $wph_skip_err"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase next returns content field (may be empty in streamlined mode)
wph_has_content="$(echo "$wph_next1" | "$PYTHON_BIN" -c 'import json,sys; d=json.load(sys.stdin); print("content" in d)')"
if [[ "$wph_has_content" == "True" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: content field present"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: content field missing"
  FAIL=$((FAIL + 1))
fi

# Test: worker-phase next returns different titles for different phases (phase 1 vs phase 2)
wph_title_p1="$(echo "$wph_next1" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("title",""))')"
wph_title_p2="$(echo "$wph_next1b" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("title",""))')"
if [[ "$wph_title_p1" != "$wph_title_p2" ]] && [[ -n "$wph_title_p2" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase next: title changes between phases (1 vs 2)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase next: expected different title for phase 1 vs 2"
  FAIL=$((FAIL + 1))
fi

# Test: worker-prompt --bootstrap outputs <300 tokens
wp_boot_json="$(CLAUDE_PLUGIN_ROOT="$TEST_DIR/repo" $FLOWCTL worker-prompt --task "${EPIC1}.1" --bootstrap --json)"
wp_boot_tokens="$(echo "$wp_boot_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["estimated_tokens"])')"
wp_boot_mode="$(echo "$wp_boot_json" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["mode"])')"
if [[ "$wp_boot_mode" == "bootstrap" ]] && [[ "$wp_boot_tokens" -lt 300 ]]; then
  echo -e "${GREEN}✓${NC} worker-prompt --bootstrap: mode=bootstrap, ${wp_boot_tokens} tokens (<300)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-prompt --bootstrap: expected mode=bootstrap and <300 tokens, got mode=$wp_boot_mode tokens=$wp_boot_tokens"
  FAIL=$((FAIL + 1))
fi

# Test: complete all remaining default phases -> all_done
# Phases 1, 2, 5 already done above; complete remaining: 6, 7, 9, 10, 11, 12
for phase in 6 7 9 10 11 12; do
  $FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase "$phase" --json >/dev/null
done
wph_final="$($FLOWCTL worker-phase next --task "${EPIC_PH}.1" --json)"
wph_all_done="$(echo "$wph_final" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["all_done"])')"
if [[ "$wph_all_done" == "True" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase lifecycle: all phases complete"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase lifecycle: expected all_done=True, got $wph_all_done"
  FAIL=$((FAIL + 1))
fi

print_results
