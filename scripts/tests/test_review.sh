#!/usr/bin/env bash
# Tests: parse_receipt_path, review-backend --compare, review receipt archival, parse-findings
source "$(cd "$(dirname "$0")" && pwd)/common.sh"

echo -e "${YELLOW}=== review tests ===${NC}"

echo -e "${YELLOW}--- parse_receipt_path ---${NC}"
# Test receipt path parsing for Ralph gating (both legacy and new fn-N-xxx formats)
"$PYTHON_BIN" - "$PLUGIN_ROOT/hooks" <<'PY'
import sys
hooks_dir = sys.argv[1]
sys.path.insert(0, hooks_dir)
from importlib.util import spec_from_file_location, module_from_spec
spec = spec_from_file_location("ralph_guard", f"{hooks_dir}/ralph-guard.py")
guard = module_from_spec(spec)
spec.loader.exec_module(guard)

# Test plan receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/plan-fn-1.json")
assert rtype == "plan_review", f"Expected plan_review, got {rtype}"
assert rid == "fn-1", f"Expected fn-1, got {rid}"

# Test impl receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/impl-fn-1.3.json")
assert rtype == "impl_review", f"Expected impl_review, got {rtype}"
assert rid == "fn-1.3", f"Expected fn-1.3, got {rid}"

# Test plan receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/plan-fn-5-x7k.json")
assert rtype == "plan_review", f"Expected plan_review, got {rtype}"
assert rid == "fn-5-x7k", f"Expected fn-5-x7k, got {rid}"

# Test impl receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/impl-fn-5-x7k.3.json")
assert rtype == "impl_review", f"Expected impl_review, got {rtype}"
assert rid == "fn-5-x7k.3", f"Expected fn-5-x7k.3, got {rid}"

# Test completion receipt parsing (legacy format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/completion-fn-2.json")
assert rtype == "completion_review", f"Expected completion_review, got {rtype}"
assert rid == "fn-2", f"Expected fn-2, got {rid}"

# Test completion receipt parsing (new fn-N-xxx format)
rtype, rid = guard.parse_receipt_path("/tmp/receipts/completion-fn-7-abc.json")
assert rtype == "completion_review", f"Expected completion_review, got {rtype}"
assert rid == "fn-7-abc", f"Expected fn-7-abc, got {rid}"

# Test fallback
rtype, rid = guard.parse_receipt_path("/tmp/unknown.json")
assert rtype == "impl_review"
assert rid == "UNKNOWN"
PY
echo -e "${GREEN}✓${NC} parse_receipt_path works"
PASS=$((PASS + 1))

echo -e "\n${YELLOW}--- review-backend --compare ---${NC}"

# Create mock receipt files
cat > "$TEST_DIR/receipt-codex.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"codex","verdict":"SHIP","timestamp":"2026-03-30T00:00:00Z","review":"Looks good"}
EOF
cat > "$TEST_DIR/receipt-rp.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"rp","verdict":"SHIP","timestamp":"2026-03-30T00:00:00Z","review":"LGTM"}
EOF
cat > "$TEST_DIR/receipt-conflict.json" << 'EOF'
{"type":"impl_review","id":"fn-1.1","mode":"rp","verdict":"NEEDS_WORK","timestamp":"2026-03-30T00:00:00Z","review":"Needs fixes"}
EOF

# Test 1: compare with consensus (both SHIP)
result="$($FLOWCTL review-backend --compare "$TEST_DIR/receipt-codex.json,$TEST_DIR/receipt-rp.json" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("consensus") == "SHIP", f"expected SHIP consensus, got {data}"
assert data.get("has_conflict") == False, f"expected no conflict: {data}"
assert data.get("reviews") == 2, f"expected 2 reviews: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --compare consensus detected"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --compare consensus failed"
  FAIL=$((FAIL + 1))
fi

# Test 2: compare with conflict (SHIP vs NEEDS_WORK)
result="$($FLOWCTL review-backend --compare "$TEST_DIR/receipt-codex.json,$TEST_DIR/receipt-conflict.json" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("has_conflict") == True, f"expected conflict: {data}"
assert data.get("consensus") is None, f"expected no consensus: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --compare conflict detected"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --compare conflict failed"
  FAIL=$((FAIL + 1))
fi

echo -e "\n${YELLOW}--- review receipt archival ---${NC}"

# Setup: create epic + task
RR_EPIC_JSON="$($FLOWCTL epic create --title "Receipt test" --json)"
RR_EPIC="$("$PYTHON_BIN" -c "import json,sys; print(json.loads(sys.argv[1])['id'])" "$RR_EPIC_JSON")"
$FLOWCTL task create --epic "$RR_EPIC" --title "Task with review" --json > /dev/null
$FLOWCTL start "${RR_EPIC}.1" --json > /dev/null

# Test 1: done with review_receipt archives to .flow/reviews/
RR_EVIDENCE="{\"commits\":[\"x1\"],\"tests\":[],\"prs\":[],\"review_receipt\":{\"type\":\"impl_review\",\"id\":\"${RR_EPIC}.1\",\"mode\":\"codex\",\"verdict\":\"SHIP\",\"timestamp\":\"2026-03-30T00:00:00Z\",\"review\":\"LGTM\"}}"
$FLOWCTL done "${RR_EPIC}.1" --summary "done" --evidence "$RR_EVIDENCE" --json > /dev/null
if [ -f ".flow/reviews/impl_review-${RR_EPIC}.1-codex.json" ]; then
  echo -e "${GREEN}✓${NC} review receipt archived to .flow/reviews/"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review receipt not archived"
  FAIL=$((FAIL + 1))
fi

# Add a second receipt (simulate rp review)
cat > ".flow/reviews/impl_review-${RR_EPIC}.1-rp.json" << 'EOF'
{"type":"impl_review","id":"PLACEHOLDER","mode":"rp","verdict":"SHIP","timestamp":"2026-03-30T00:01:00Z","review":"Looks good"}
EOF

# Test 2: review-backend --epic auto-discovers receipts
result="$($FLOWCTL review-backend --epic "$RR_EPIC" --json)"
"$PYTHON_BIN" - "$result" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data.get("reviews") == 2, f"expected 2 reviews, got {data.get('reviews')}"
assert data.get("consensus") == "SHIP", f"expected SHIP consensus: {data}"
PY
if [ $? -eq 0 ]; then
  echo -e "${GREEN}✓${NC} review-backend --epic auto-discovers receipts"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} review-backend --epic failed"
  FAIL=$((FAIL + 1))
fi

echo -e "${YELLOW}--- parse-findings ---${NC}"

# Test: valid <findings> tag
FINDINGS_FILE="$TEST_DIR/findings_valid.txt"
cat > "$FINDINGS_FILE" <<'FINDINGS_EOF'
Some review preamble text.

<findings>
[
  {
    "title": "Missing input validation",
    "severity": "critical",
    "location": "src/auth.py:42",
    "recommendation": "Add input sanitization"
  },
  {
    "title": "Unused import",
    "severity": "nitpick",
    "location": "src/utils.py:1",
    "recommendation": "Remove unused import"
  }
]
</findings>

More review text after.
FINDINGS_EOF

pf_result="$($FLOWCTL parse-findings --file "$FINDINGS_FILE" --json)"
pf_count="$(echo "$pf_result" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$pf_count" == "2" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings extracts findings from <findings> tag"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings count wrong (expected 2, got $pf_count)"
  FAIL=$((FAIL + 1))
fi

# Test: missing <findings> tag -> graceful empty
FINDINGS_EMPTY="$TEST_DIR/findings_empty.txt"
echo "No findings here, just plain review text." > "$FINDINGS_EMPTY"

pf_empty="$($FLOWCTL parse-findings --file "$FINDINGS_EMPTY" --json)"
pf_empty_count="$(echo "$pf_empty" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
pf_empty_warns="$(echo "$pf_empty" | "$PYTHON_BIN" -c 'import json,sys; w=json.load(sys.stdin).get("warnings",[]); print(len(w))')"
if [[ "$pf_empty_count" == "0" ]] && [[ "$pf_empty_warns" -ge 1 ]]; then
  echo -e "${GREEN}✓${NC} parse-findings gracefully handles missing tags"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings missing tag handling wrong (count=$pf_empty_count, warns=$pf_empty_warns)"
  FAIL=$((FAIL + 1))
fi

# Test: malformed JSON (trailing commas)
FINDINGS_MALFORMED="$TEST_DIR/findings_malformed.txt"
cat > "$FINDINGS_MALFORMED" <<'FINDINGS_EOF'
<findings>
[
  {
    "title": "Trailing comma issue",
    "severity": "major",
    "location": "src/app.py:10",
    "recommendation": "Fix the trailing comma",
  },
]
</findings>
FINDINGS_EOF

pf_mal="$($FLOWCTL parse-findings --file "$FINDINGS_MALFORMED" --json)"
pf_mal_count="$(echo "$pf_mal" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("count", 0))')"
if [[ "$pf_mal_count" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings handles malformed JSON (trailing commas)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings malformed JSON handling wrong (expected 1, got $pf_mal_count)"
  FAIL=$((FAIL + 1))
fi

# Test: --register auto gap add
# Need an epic for gap registration
REG_EPIC_JSON="$($FLOWCTL epic create --title "Findings register" --json)"
REG_EPIC="$(echo "$REG_EPIC_JSON" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin)["id"])')"
$FLOWCTL task create --epic "$REG_EPIC" --title "Task 1" --json >/dev/null

FINDINGS_REG="$TEST_DIR/findings_register.txt"
cat > "$FINDINGS_REG" <<'FINDINGS_EOF'
<findings>
[
  {
    "title": "SQL injection vulnerability",
    "severity": "critical",
    "location": "src/db.py:99",
    "recommendation": "Use parameterized queries"
  },
  {
    "title": "Minor typo in comment",
    "severity": "minor",
    "location": "src/main.py:5",
    "recommendation": "Fix typo"
  }
]
</findings>
FINDINGS_EOF

pf_reg="$($FLOWCTL parse-findings --file "$FINDINGS_REG" --epic "$REG_EPIC" --register --source plan-review --json)"
pf_reg_registered="$(echo "$pf_reg" | "$PYTHON_BIN" -c 'import json,sys; print(json.load(sys.stdin).get("registered", 0))')"
if [[ "$pf_reg_registered" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings --register adds critical/major gaps (skips minor)"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings --register wrong count (expected 1, got $pf_reg_registered)"
  FAIL=$((FAIL + 1))
fi

# Verify the gap was actually created
gap_reg_check="$($FLOWCTL gap list --epic "$REG_EPIC" --json | "$PYTHON_BIN" -c '
import json, sys
data = json.load(sys.stdin)
gaps = data.get("gaps", [])
sql_gaps = [g for g in gaps if "SQL injection" in g.get("capability", "")]
print(len(sql_gaps))
')"
if [[ "$gap_reg_check" == "1" ]]; then
  echo -e "${GREEN}✓${NC} parse-findings --register actually created the gap"
  PASS=$((PASS + 1))
else
  echo -e "${RED}✗${NC} parse-findings --register gap not found in registry (found $gap_reg_check)"
  FAIL=$((FAIL + 1))
fi

print_results
