#!/usr/bin/env bash
set -euo pipefail

# Shared setup for all smoke test files.
# Source this file at the top of each test_*.sh.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

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

mkdir -p "$TEST_DIR/repo"
cd "$TEST_DIR/repo"
git init -q

# Locate flowctl binary (Rust)
if [[ -x "$PLUGIN_ROOT/bin/flowctl" ]]; then
  FLOWCTL="$PLUGIN_ROOT/bin/flowctl"
elif command -v flowctl >/dev/null 2>&1; then
  FLOWCTL="$(command -v flowctl)"
else
  echo "ERROR: flowctl binary not found. Build with: cd flowctl && cargo build --release && cp target/release/flowctl ../bin/" >&2
  exit 1
fi

$FLOWCTL init --json >/dev/null
printf '{"commits":[],"tests":[],"prs":[]}' > "$TEST_DIR/evidence.json"
printf "ok\n" > "$TEST_DIR/summary.md"

# Print results summary (call at end of each test file)
print_results() {
  echo ""
  echo -e "${YELLOW}=== Results ===${NC}"
  echo -e "Passed: ${GREEN}$PASS${NC}"
  echo -e "Failed: ${RED}$FAIL${NC}"
  if [ $FAIL -gt 0 ]; then
    exit 1
  fi
  echo -e "\n${GREEN}All tests passed!${NC}"
}
