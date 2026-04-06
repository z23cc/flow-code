#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Locate flowctl binary
if [[ -n "${1:-}" ]] && [[ -x "$PLUGIN_ROOT/$1" ]]; then
  FLOWCTL="$PLUGIN_ROOT/$1"
elif [[ -x "$PLUGIN_ROOT/bin/flowctl" ]]; then
  FLOWCTL="$PLUGIN_ROOT/bin/flowctl"
elif [[ -x "$PLUGIN_ROOT/flowctl/target/release/flowctl" ]]; then
  FLOWCTL="$PLUGIN_ROOT/flowctl/target/release/flowctl"
elif command -v flowctl >/dev/null 2>&1; then
  FLOWCTL="$(command -v flowctl)"
else
  echo "ERROR: flowctl binary not found. Build with: cd flowctl && cargo build --release && cp target/release/flowctl ../bin/" >&2
  exit 1
fi

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0

check() {
  local label="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    echo -e "  ${GREEN}PASS${NC} $label"
    PASS=$((PASS + 1))
  else
    echo -e "  ${RED}FAIL${NC} $label"
    FAIL=$((FAIL + 1))
  fi
}

TEST_DIR=$(mktemp -d)
trap "rm -rf $TEST_DIR" EXIT

echo -e "${YELLOW}=== codex sync smoke tests ===${NC}"

# --- Setup test agents ---
mkdir -p "$TEST_DIR/agents"
cat > "$TEST_DIR/agents/test-scout.md" <<'EOF'
---
name: test-scout
description: A test scout
model: opus
disallowedTools: Edit, Write
---
# Test Scout Instructions
Do the thing.
EOF

cat > "$TEST_DIR/agents/test-worker.md" <<'EOF'
---
name: worker
description: A test worker
model: inherit
---
# Worker Instructions
Implement the task.
EOF

# --- Setup test hooks ---
mkdir -p "$TEST_DIR/hooks"
echo '{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo test"}]}]}}' > "$TEST_DIR/hooks/hooks.json"

# --- Test 1: sync writes TOML files ---
echo -e "${YELLOW}--- codex sync generates TOML files ---${NC}"
OUTPUT_DIR="$TEST_DIR/codex"
$FLOWCTL codex sync --agents-dir "$TEST_DIR/agents" --output-dir "$OUTPUT_DIR" --hooks "$TEST_DIR/hooks/hooks.json"

TOML_COUNT=$(ls "$OUTPUT_DIR/agents/"*.toml 2>/dev/null | wc -l | tr -d ' ')
check "expected 2 TOML files" [ "$TOML_COUNT" -eq 2 ]

# --- Test 2: scout gets read-only sandbox ---
echo -e "${YELLOW}--- scout sandbox is read-only ---${NC}"
check "scout sandbox read-only" grep -q 'sandbox_mode = "read-only"' "$OUTPUT_DIR/agents/test-scout.toml"

# --- Test 3: worker gets workspace-write sandbox ---
echo -e "${YELLOW}--- worker sandbox is workspace-write ---${NC}"
check "worker sandbox workspace-write" grep -q 'sandbox_mode = "workspace-write"' "$OUTPUT_DIR/agents/worker.toml"

# --- Test 4: hooks patched Bash → Bash|Execute ---
echo -e "${YELLOW}--- hooks patched ---${NC}"
check "hooks contain Bash|Execute" grep -q 'Bash|Execute' "$OUTPUT_DIR/hooks.json"

# --- Test 5: dry run does not create output ---
echo -e "${YELLOW}--- dry run ---${NC}"
DRY_DIR="$TEST_DIR/codex-dry"
$FLOWCTL codex sync --agents-dir "$TEST_DIR/agents" --output-dir "$DRY_DIR" --dry-run --json
check "dry-run does not create output dir" [ ! -d "$DRY_DIR" ]

# --- Summary ---
echo ""
echo -e "${YELLOW}=== Results: ${GREEN}$PASS passed${NC}, ${RED}$FAIL failed${NC} ==="
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
echo "All codex smoke tests passed!"
