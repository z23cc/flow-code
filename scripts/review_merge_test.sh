#!/usr/bin/env bash
# Integration tests for: flowctl review merge --files "..." --json
# Validates the end-to-end merge pipeline via CLI.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Temp dir ────────────────────────────────────────────────────────
TEST_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TEST_DIR"; }
trap cleanup EXIT

# ── Colors and counters ─────────────────────────────────────────────
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'
PASS=0
FAIL=0

pass() { echo -e "${GREEN}✓${NC} $1"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}✗${NC} $1"; FAIL=$((FAIL + 1)); }

# ── Locate flowctl binary ──────────────────────────────────────────
if [[ -x "$PLUGIN_ROOT/bin/flowctl" ]]; then
  FLOWCTL="$PLUGIN_ROOT/bin/flowctl"
elif [[ -x "$HOME/.flow/bin/flowctl" ]]; then
  FLOWCTL="$HOME/.flow/bin/flowctl"
elif command -v flowctl >/dev/null 2>&1; then
  FLOWCTL="$(command -v flowctl)"
else
  echo "ERROR: flowctl binary not found. Build with: cd flowctl && cargo build --release && cp target/release/flowctl ../bin/" >&2
  exit 1
fi

echo -e "${YELLOW}=== Review Merge Integration Tests ===${NC}"
echo "flowctl: $FLOWCTL"
echo "Test dir: $TEST_DIR"

# ── Helper: make a finding JSON object ──────────────────────────────
# Usage: make_finding severity file line confidence autofix_class pre_existing reviewer description
make_finding() {
  local sev="$1" file="$2" line="$3" conf="$4" autofix="$5" pre="$6" reviewer="$7" desc="$8"
  cat <<ENDJSON
{
  "severity": "$sev",
  "category": "logic",
  "description": "$desc",
  "file": "$file",
  "line": $line,
  "confidence": $conf,
  "autofix_class": "$autofix",
  "owner": "review-fixer",
  "evidence": ["test evidence"],
  "pre_existing": $pre,
  "requires_verification": false,
  "reviewer": "$reviewer"
}
ENDJSON
}

# ═════════════════════════════════════════════════════════════════════
# Test 1: Basic merge of two reviewer outputs
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 1: Basic merge (non-overlapping) ---${NC}"

cat > "$TEST_DIR/r1.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "security",
    "description": "Missing input validation",
    "file": "src/api.rs",
    "line": 10,
    "confidence": 0.85,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["No sanitization on user input"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  }
]
EOF

cat > "$TEST_DIR/r2.json" <<'EOF'
[
  {
    "severity": "P2",
    "category": "performance",
    "description": "Unbounded query without limit",
    "file": "src/db.rs",
    "line": 50,
    "confidence": 0.80,
    "autofix_class": "safe_auto",
    "owner": "review-fixer",
    "evidence": ["SELECT * with no LIMIT"],
    "pre_existing": false,
    "reviewer": "reviewer-b"
  }
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/r1.json,$TEST_DIR/r2.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
TOTAL_INPUT="$(echo "$OUT" | jq '.stats.total_input')"
DEDUPED="$(echo "$OUT" | jq '.stats.deduplicated')"

[[ "$FINDING_COUNT" -eq 2 ]] && pass "Basic merge: 2 findings" || fail "Basic merge: expected 2 findings, got $FINDING_COUNT"
[[ "$TOTAL_INPUT" -eq 2 ]] && pass "Basic merge: total_input=2" || fail "Basic merge: expected total_input=2, got $TOTAL_INPUT"
[[ "$DEDUPED" -eq 0 ]] && pass "Basic merge: no dedup" || fail "Basic merge: expected deduplicated=0, got $DEDUPED"

# ═════════════════════════════════════════════════════════════════════
# Test 2: Dedup overlapping findings from different reviewers
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 2: Dedup overlapping findings ---${NC}"

# Same file, same line bucket (line 10, bucket = 10/6*6 = 6), same title
cat > "$TEST_DIR/d1.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "logic",
    "description": "Null pointer dereference",
    "file": "src/main.rs",
    "line": 10,
    "confidence": 0.80,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Unchecked .unwrap()"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  }
]
EOF

cat > "$TEST_DIR/d2.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "logic",
    "description": "Null pointer dereference",
    "file": "src/main.rs",
    "line": 11,
    "confidence": 0.75,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Unchecked unwrap call"],
    "pre_existing": false,
    "reviewer": "reviewer-b"
  }
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/d1.json,$TEST_DIR/d2.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
DEDUPED="$(echo "$OUT" | jq '.stats.deduplicated')"

[[ "$FINDING_COUNT" -eq 1 ]] && pass "Dedup: merged to 1 finding" || fail "Dedup: expected 1 finding, got $FINDING_COUNT"
[[ "$DEDUPED" -gt 0 ]] && pass "Dedup: stats.deduplicated > 0" || fail "Dedup: expected deduplicated > 0, got $DEDUPED"

# ═════════════════════════════════════════════════════════════════════
# Test 3: Cross-reviewer confidence boost
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 3: Cross-reviewer confidence boost ---${NC}"

# Two reviewers report the same fingerprint, both at 0.75.
# Winner keeps 0.75 (highest), then gets +0.10 boost = 0.85
cat > "$TEST_DIR/b1.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "logic",
    "description": "Race condition in cache update",
    "file": "src/cache.rs",
    "line": 20,
    "confidence": 0.75,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Concurrent writes without lock"],
    "pre_existing": false,
    "reviewer": "reviewer-alpha"
  }
]
EOF

cat > "$TEST_DIR/b2.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "logic",
    "description": "Race condition in cache update",
    "file": "src/cache.rs",
    "line": 20,
    "confidence": 0.75,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Missing lock on shared state"],
    "pre_existing": false,
    "reviewer": "reviewer-beta"
  }
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/b1.json,$TEST_DIR/b2.json" --json)"

BOOSTED="$(echo "$OUT" | jq '.stats.boosted')"
CONF="$(echo "$OUT" | jq '.findings[0].confidence')"

[[ "$BOOSTED" -ge 1 ]] && pass "Boost: stats.boosted >= 1" || fail "Boost: expected boosted >= 1, got $BOOSTED"
# Confidence should be 0.85 (0.75 + 0.10)
CONF_OK="$(echo "$CONF" | awk '{ if ($1 >= 0.84 && $1 <= 0.86) print "yes"; else print "no" }')"
[[ "$CONF_OK" == "yes" ]] && pass "Boost: confidence = 0.85 (was 0.75 + 0.10)" || fail "Boost: expected confidence ~0.85, got $CONF"

# ═════════════════════════════════════════════════════════════════════
# Test 4: Confidence suppression
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 4: Confidence suppression ---${NC}"

# P1 at 0.55 should be suppressed (threshold 0.60)
# P0 at 0.50 should be kept (P0 threshold 0.50)
# P1 at 0.70 should be kept (above 0.60)
cat > "$TEST_DIR/s1.json" <<'EOF'
[
  {
    "severity": "P1",
    "category": "logic",
    "description": "Maybe unused variable",
    "file": "src/a.rs",
    "line": 5,
    "confidence": 0.55,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Possibly unused"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  },
  {
    "severity": "P0",
    "category": "security",
    "description": "SQL injection vector",
    "file": "src/b.rs",
    "line": 100,
    "confidence": 0.50,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["String interpolation in query"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  },
  {
    "severity": "P1",
    "category": "logic",
    "description": "Off by one error in loop",
    "file": "src/c.rs",
    "line": 30,
    "confidence": 0.70,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Loop bound is len not len-1"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  }
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/s1.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
SUPPRESSED="$(echo "$OUT" | jq '.stats.suppressed')"

[[ "$FINDING_COUNT" -eq 2 ]] && pass "Suppression: 2 findings kept" || fail "Suppression: expected 2 findings, got $FINDING_COUNT"
[[ "$SUPPRESSED" -eq 1 ]] && pass "Suppression: 1 suppressed (P1 at 0.55)" || fail "Suppression: expected 1 suppressed, got $SUPPRESSED"

# Verify P0 is in the output
P0_PRESENT="$(echo "$OUT" | jq '[.findings[] | select(.severity == "P0")] | length')"
[[ "$P0_PRESENT" -eq 1 ]] && pass "Suppression: P0 at 0.50 kept (exception)" || fail "Suppression: P0 should survive at 0.50"

# ═════════════════════════════════════════════════════════════════════
# Test 5: Conservative autofix routing
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 5: Conservative autofix routing ---${NC}"

# Same finding, reviewer-a says safe_auto, reviewer-b says manual.
# Merged result should use manual (most restrictive).
cat > "$TEST_DIR/c1.json" <<'EOF'
[
  {
    "severity": "P2",
    "category": "style",
    "description": "Inconsistent naming convention",
    "file": "src/utils.rs",
    "line": 15,
    "confidence": 0.80,
    "autofix_class": "safe_auto",
    "owner": "review-fixer",
    "evidence": ["snake_case vs camelCase"],
    "pre_existing": false,
    "reviewer": "reviewer-a"
  }
]
EOF

cat > "$TEST_DIR/c2.json" <<'EOF'
[
  {
    "severity": "P2",
    "category": "style",
    "description": "Inconsistent naming convention",
    "file": "src/utils.rs",
    "line": 15,
    "confidence": 0.80,
    "autofix_class": "manual",
    "owner": "review-fixer",
    "evidence": ["Naming inconsistency across module"],
    "pre_existing": false,
    "reviewer": "reviewer-b"
  }
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/c1.json,$TEST_DIR/c2.json" --json)"

AUTOFIX="$(echo "$OUT" | jq -r '.findings[0].autofix_class')"
[[ "$AUTOFIX" == "manual" ]] && pass "Conservative routing: manual wins over safe_auto" || fail "Conservative routing: expected manual, got $AUTOFIX"

# ═════════════════════════════════════════════════════════════════════
# Test 6: Partition verification
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 6: Partition verification ---${NC}"

cat > "$TEST_DIR/p1.json" <<EOF
[
  $(make_finding P1 "src/a.rs" 10 0.90 "safe_auto" false "rev-a" "Auto-fixable lint issue"),
  $(make_finding P2 "src/b.rs" 20 0.85 "gated_auto" false "rev-a" "Gated auto fix needed"),
  $(make_finding P2 "src/c.rs" 30 0.80 "manual" false "rev-a" "Manual review required"),
  $(make_finding P3 "src/d.rs" 40 0.75 "advisory" false "rev-a" "Style suggestion only")
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/p1.json" --json)"

FIXER="$(echo "$OUT" | jq '.partition.fixer_queue')"
RESIDUAL="$(echo "$OUT" | jq '.partition.residual_queue')"
REPORT="$(echo "$OUT" | jq '.partition.report_only')"

[[ "$FIXER" -eq 1 ]] && pass "Partition: fixer_queue=1 (safe_auto)" || fail "Partition: expected fixer_queue=1, got $FIXER"
[[ "$RESIDUAL" -eq 2 ]] && pass "Partition: residual_queue=2 (gated_auto + manual)" || fail "Partition: expected residual_queue=2, got $RESIDUAL"
[[ "$REPORT" -eq 1 ]] && pass "Partition: report_only=1 (advisory)" || fail "Partition: expected report_only=1, got $REPORT"

# ═════════════════════════════════════════════════════════════════════
# Test 7: Pre-existing separation
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 7: Pre-existing separation ---${NC}"

cat > "$TEST_DIR/pe1.json" <<EOF
[
  $(make_finding P1 "src/old.rs" 10 0.90 "manual" true "rev-a" "Legacy bug in old code"),
  $(make_finding P1 "src/new.rs" 20 0.85 "manual" false "rev-a" "New bug introduced by diff")
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/pe1.json" --json)"

MAIN_COUNT="$(echo "$OUT" | jq '.findings | length')"
PRE_COUNT="$(echo "$OUT" | jq '.pre_existing | length')"
PRE_STAT="$(echo "$OUT" | jq '.stats.pre_existing_count')"

[[ "$MAIN_COUNT" -eq 1 ]] && pass "Pre-existing: 1 actionable finding" || fail "Pre-existing: expected 1 actionable, got $MAIN_COUNT"
[[ "$PRE_COUNT" -eq 1 ]] && pass "Pre-existing: 1 in pre_existing array" || fail "Pre-existing: expected 1 pre_existing, got $PRE_COUNT"
[[ "$PRE_STAT" -eq 1 ]] && pass "Pre-existing: stats.pre_existing_count=1" || fail "Pre-existing: expected stat=1, got $PRE_STAT"

# Verify the pre-existing finding is the right one
PRE_FILE="$(echo "$OUT" | jq -r '.pre_existing[0].file')"
[[ "$PRE_FILE" == "src/old.rs" ]] && pass "Pre-existing: correct file (src/old.rs)" || fail "Pre-existing: expected src/old.rs, got $PRE_FILE"

# ═════════════════════════════════════════════════════════════════════
# Test 8: Sort order (P0 first, then confidence desc)
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 8: Sort order ---${NC}"

cat > "$TEST_DIR/sort1.json" <<EOF
[
  $(make_finding P3 "src/z.rs" 10 0.90 "manual" false "rev-a" "Low severity high confidence"),
  $(make_finding P0 "src/a.rs" 20 0.70 "manual" false "rev-a" "Critical issue low confidence"),
  $(make_finding P1 "src/b.rs" 30 0.95 "manual" false "rev-a" "High severity very confident"),
  $(make_finding P2 "src/c.rs" 40 0.80 "manual" false "rev-a" "Medium severity issue")
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/sort1.json" --json)"

SEV0="$(echo "$OUT" | jq -r '.findings[0].severity')"
SEV1="$(echo "$OUT" | jq -r '.findings[1].severity')"
SEV2="$(echo "$OUT" | jq -r '.findings[2].severity')"
SEV3="$(echo "$OUT" | jq -r '.findings[3].severity')"

[[ "$SEV0" == "P0" ]] && pass "Sort: P0 is first" || fail "Sort: expected P0 first, got $SEV0"
[[ "$SEV1" == "P1" ]] && pass "Sort: P1 is second" || fail "Sort: expected P1 second, got $SEV1"
[[ "$SEV2" == "P2" ]] && pass "Sort: P2 is third" || fail "Sort: expected P2 third, got $SEV2"
[[ "$SEV3" == "P3" ]] && pass "Sort: P3 is fourth" || fail "Sort: expected P3 fourth, got $SEV3"

# ═════════════════════════════════════════════════════════════════════
# Test 9: Empty input
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 9: Empty input ---${NC}"

echo '[]' > "$TEST_DIR/empty.json"

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/empty.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
TOTAL_INPUT="$(echo "$OUT" | jq '.stats.total_input')"
PRE_COUNT="$(echo "$OUT" | jq '.pre_existing | length')"

[[ "$FINDING_COUNT" -eq 0 ]] && pass "Empty: 0 findings" || fail "Empty: expected 0 findings, got $FINDING_COUNT"
[[ "$TOTAL_INPUT" -eq 0 ]] && pass "Empty: total_input=0" || fail "Empty: expected total_input=0, got $TOTAL_INPUT"
[[ "$PRE_COUNT" -eq 0 ]] && pass "Empty: 0 pre_existing" || fail "Empty: expected 0 pre_existing, got $PRE_COUNT"

# ═════════════════════════════════════════════════════════════════════
# Test 10: Single file pass-through
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 10: Single file pass-through ---${NC}"

cat > "$TEST_DIR/single.json" <<EOF
[
  $(make_finding P1 "src/single.rs" 10 0.90 "safe_auto" false "rev-a" "Single finding pass-through")
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/single.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
TOTAL_INPUT="$(echo "$OUT" | jq '.stats.total_input')"
DESC="$(echo "$OUT" | jq -r '.findings[0].description')"

[[ "$FINDING_COUNT" -eq 1 ]] && pass "Single: 1 finding" || fail "Single: expected 1 finding, got $FINDING_COUNT"
[[ "$TOTAL_INPUT" -eq 1 ]] && pass "Single: total_input=1" || fail "Single: expected total_input=1, got $TOTAL_INPUT"
[[ "$DESC" == "Single finding pass-through" ]] && pass "Single: description preserved" || fail "Single: description mismatch"

# ═════════════════════════════════════════════════════════════════════
# Test 11: Object-with-findings-key format
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 11: Object-with-findings-key format ---${NC}"

cat > "$TEST_DIR/obj.json" <<EOF
{
  "findings": [
    $(make_finding P2 "src/obj.rs" 10 0.80 "manual" false "rev-a" "Object format finding")
  ]
}
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/obj.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
[[ "$FINDING_COUNT" -eq 1 ]] && pass "Object format: accepted and merged" || fail "Object format: expected 1 finding, got $FINDING_COUNT"

# ═════════════════════════════════════════════════════════════════════
# Test 12: Error on missing file
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 12: Error on missing file ---${NC}"

set +e
ERR_OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/nonexistent.json" --json 2>&1)"
ERR_RC=$?
set -e

[[ $ERR_RC -ne 0 ]] && pass "Missing file: exits with error" || fail "Missing file: should fail"

# ═════════════════════════════════════════════════════════════════════
# Test 13: Multi-file with mixed formats
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 13: Multi-file with mixed formats ---${NC}"

# r1.json is bare array (from Test 1), obj.json is object format (from Test 11)
OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/r1.json,$TEST_DIR/obj.json" --json)"

FINDING_COUNT="$(echo "$OUT" | jq '.findings | length')"
[[ "$FINDING_COUNT" -eq 2 ]] && pass "Mixed formats: 2 findings merged" || fail "Mixed formats: expected 2, got $FINDING_COUNT"

# ═════════════════════════════════════════════════════════════════════
# Test 14: Confidence boost caps at 1.0
# ═════════════════════════════════════════════════════════════════════
echo -e "\n${YELLOW}--- Test 14: Confidence boost capped at 1.0 ---${NC}"

cat > "$TEST_DIR/cap1.json" <<EOF
[
  $(make_finding P1 "src/cap.rs" 10 0.95 "manual" false "rev-a" "High confidence capped finding")
]
EOF

cat > "$TEST_DIR/cap2.json" <<EOF
[
  $(make_finding P1 "src/cap.rs" 10 0.92 "manual" false "rev-b" "High confidence capped finding")
]
EOF

OUT="$("$FLOWCTL" review merge --files "$TEST_DIR/cap1.json,$TEST_DIR/cap2.json" --json)"

CONF="$(echo "$OUT" | jq '.findings[0].confidence')"
CONF_OK="$(echo "$CONF" | awk '{ if ($1 <= 1.001) print "yes"; else print "no" }')"
[[ "$CONF_OK" == "yes" ]] && pass "Boost cap: confidence <= 1.0 ($CONF)" || fail "Boost cap: confidence > 1.0 ($CONF)"

# ═════════════════════════════════════════════════════════════════════
# Summary
# ═════════════════════════════════════════════════════════════════════
echo ""
echo -e "${YELLOW}═══════════════════════════════════════${NC}"
TOTAL=$((PASS + FAIL))
echo -e "  ${GREEN}Passed${NC}: $PASS / $TOTAL"
if [[ $FAIL -gt 0 ]]; then
  echo -e "  ${RED}Failed${NC}: $FAIL / $TOTAL"
  echo -e "${YELLOW}═══════════════════════════════════════${NC}"
  exit 1
else
  echo -e "  ${GREEN}All tests passed!${NC}"
  echo -e "${YELLOW}═══════════════════════════════════════${NC}"
  exit 0
fi
