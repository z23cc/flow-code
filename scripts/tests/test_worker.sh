#!/usr/bin/env bash
# Tests: worker-prompt and worker-phase
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== worker tests ===${NC}"

# Create epic + tasks for worker tests
EPIC1_JSON="$($FLOWCTL epic create --title "Worker Epic" --json)"
EPIC1="$(echo "$EPIC1_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$EPIC1" --title "Task 1" --json >/dev/null

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
mkdir -p "$TEST_DIR/repo/src" "$TEST_DIR/repo/.flow/outputs"
cat > "$TEST_DIR/repo/src/phase_task.py" <<'EOF'
def run():
    return "ok"
EOF
cat > "$TEST_DIR/repo/.flow/tasks/${EPIC_PH}.1.md" <<EOF
# ${EPIC_PH}.1 Phase task

## Description
Exercise worker phase gating.

## Investigation targets
- src/phase_task.py

## Acceptance
- [ ] Worker phase gates require receipts

**Files:**
- src/phase_task.py
EOF

phase_receipt_file() {
  local phase="$1"
  echo "$TEST_DIR/worker-phase-${phase}.json"
}

worker_phase_done() {
  local phase="$1"
  local payload="${2:-}"
  local receipt_file
  receipt_file="$(phase_receipt_file "$phase")"
  if [[ -n "$payload" ]]; then
    printf '%s\n' "$payload" > "$receipt_file"
    $FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase "$phase" --receipt-file "$receipt_file" --json >/dev/null
  else
    $FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase "$phase" --json >/dev/null
  fi
}

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
phase1_gate_err="$($FLOWCTL worker-phase done --task "${EPIC_PH}.1" --phase 1 --json 2>&1 || true)"
if echo "$phase1_gate_err" | grep -q "requires --receipt"; then
  echo -e "${GREEN}✓${NC} worker-phase gate: phase 1 rejects missing receipt"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase gate: expected missing-receipt error for phase 1, got: $phase1_gate_err"
  FAIL=$((FAIL + 1))
fi
worker_phase_done 1 '{"owned_files":["src/phase_task.py"],"config_valid":true}'
wph_next1b="$($FLOWCTL worker-phase next --task "${EPIC_PH}.1" --json)"
wph_phase1b="$(echo "$wph_next1b" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["phase"])')"
if [[ "$wph_phase1b" == "2" ]]; then
  echo -e "${GREEN}✓${NC} worker-phase done->next: advances to phase 2"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} worker-phase done->next: expected phase=2, got $wph_phase1b"
  FAIL=$((FAIL + 1))
fi

# Advance through phases 2, 3, and 5 to test 6
worker_phase_done 2 '{"acceptance_points":["- [ ] Worker phase gates require receipts"]}'
worker_phase_done 3 '{"files_read":[".flow/tasks/'"${EPIC_PH}"'.1.md","src/phase_task.py"]}'
worker_phase_done 5 '{"files_changed":["src/phase_task.py"],"implemented":true}'
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
# Phases 1, 2, 3, 5 already done above; complete remaining: 6, 7, 9, 10, 11, 12
worker_phase_done 6 '{"guard_passed":true,"diff_reviewed":true}'
worker_phase_done 7 '{"commit":"abcdef1234567"}'
printf 'Implemented worker phase gate test.\n' > "$TEST_DIR/repo/.flow/outputs/${EPIC_PH}.1.md"
worker_phase_done 9 '{"output_path":".flow/outputs/'"${EPIC_PH}"'.1.md"}'
$FLOWCTL done "${EPIC_PH}.1" --summary-file "$TEST_DIR/summary.md" --evidence-json "$TEST_DIR/evidence.json" --json >/dev/null
worker_phase_done 10
worker_phase_done 11 '{"checked":true,"saved":false}'
worker_phase_done 12 '{"summary":"Completed worker phase lifecycle in the smoke test."}'
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
