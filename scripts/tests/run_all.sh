#!/usr/bin/env bash
set -euo pipefail

# Run all smoke test files sequentially.
# Each test file is independently runnable and creates its own isolated temp dir.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OVERALL_PASS=0
OVERALL_FAIL=0

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}=== flowctl smoke tests (split) ===${NC}\n"

for test_file in "$SCRIPT_DIR"/test_*.sh; do
  test_name="$(basename "$test_file" .sh)"
  echo -e "${YELLOW}>>> Running $test_name ...${NC}"
  if bash "$test_file"; then
    echo -e "${GREEN}<<< $test_name passed${NC}\n"
    OVERALL_PASS=$((OVERALL_PASS + 1))
  else
    echo -e "${RED}<<< $test_name FAILED${NC}\n"
    OVERALL_FAIL=$((OVERALL_FAIL + 1))
  fi
done

echo -e "\n${YELLOW}=== Overall Results ===${NC}"
echo -e "Test files passed: ${GREEN}$OVERALL_PASS${NC}"
echo -e "Test files failed: ${RED}$OVERALL_FAIL${NC}"

if [ $OVERALL_FAIL -gt 0 ]; then
  echo -e "\n${RED}Some test files failed!${NC}"
  exit 1
fi
echo -e "\n${GREEN}All test files passed!${NC}"
