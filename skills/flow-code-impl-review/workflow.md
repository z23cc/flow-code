# Implementation Review Workflow

## Philosophy

The reviewer model only sees selected files. RepoPrompt's Builder discovers context you'd miss (rp backend). Codex uses context hints from flowctl (codex backend).

---

## Phase 0: Backend Detection

**Run this first. Do not skip.**

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
set -e
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

# Priority: --review flag > env > config (flag parsed in SKILL.md)
BACKEND=$($FLOWCTL review-backend)

if [[ "$BACKEND" == "ASK" ]]; then
  echo "Error: No review backend configured."
  echo "Run /flow-code:setup to configure, or pass --review=rp|codex|none"
  exit 1
fi

echo "Review backend: $BACKEND (override: --review=rp|codex|none)"
```

**If backend is "none"**: Skip review, inform user, and exit cleanly (no error).

**Then branch to backend-specific workflow below.**

---

## Codex Backend Workflow

Use when `BACKEND="codex"`.

### Step 1: Identify Task and Diff Base

```bash
BRANCH="$(git branch --show-current)"

# Use BASE_COMMIT from arguments if provided (task-scoped review)
# Otherwise fall back to main/master (full branch review)
if [[ -z "$BASE_COMMIT" ]]; then
  DIFF_BASE="main"
  git rev-parse main >/dev/null 2>&1 || DIFF_BASE="master"
else
  DIFF_BASE="$BASE_COMMIT"
fi

git log ${DIFF_BASE}..HEAD --oneline
```

### Step 2: Execute Review

```bash
RECEIPT_PATH="${REVIEW_RECEIPT_PATH:-/tmp/impl-review-receipt.json}"

$FLOWCTL codex impl-review "$TASK_ID" --base "$DIFF_BASE" --receipt "$RECEIPT_PATH"
```

**Output includes `VERDICT=SHIP|NEEDS_WORK|MAJOR_RETHINK`.**

Codex is instructed to return findings as a JSON array. Each finding must have: `title`, `severity` (P0/P1/P2/P3), `file`, `line`, `why_it_matters`, `confidence` (0.0-1.0), `autofix_class` (safe_auto/gated_auto/manual/advisory), `evidence` (array of strings). Findings with confidence < 0.6 are suppressed unless P0. If structured JSON is not possible, free-text findings are still accepted by parse-findings.

### Step 3: Handle Verdict

If `VERDICT=NEEDS_WORK`:
1. Parse issues from output and register as gaps:
   ```bash
   # Save review output to temp file, then register findings as gaps
   echo "$REVIEW_OUTPUT" > /tmp/review-response.txt
   FINDINGS_RESULT="$($FLOWCTL parse-findings --file /tmp/review-response.txt --epic "$EPIC_ID" --register --source impl-review --json)"
   REGISTERED="$(echo "$FINDINGS_RESULT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("registered",0))' 2>/dev/null || echo 0)"
   echo "Registered $REGISTERED findings as gaps"
   ```
2. Fix code and run tests
3. Commit fixes
4. Re-run step 2 (receipt enables session continuity)
5. Repeat until SHIP

### Step 4: Receipt

Receipt is written automatically by `flowctl codex impl-review` when `--receipt` provided.
Format: `{"mode":"codex","task":"<id>","verdict":"<verdict>","session_id":"<thread_id>","timestamp":"..."}`

---

## RepoPrompt Backend Workflow

Use when `BACKEND="rp"`.

**This workflow follows the shared RP review protocol** defined in `skills/_shared/rp-review-protocol.md`. The steps below set impl-review-specific variables, then delegate to the shared protocol.

### Phase 1: Identify Changes (RP)

```bash
BRANCH="$(git branch --show-current)"

# Use BASE_COMMIT from arguments if provided (task-scoped review)
# Otherwise fall back to main/master (full branch review)
if [[ -z "$BASE_COMMIT" ]]; then
  DIFF_BASE="main"
  git rev-parse main >/dev/null 2>&1 || DIFF_BASE="master"
else
  DIFF_BASE="$BASE_COMMIT"
fi

git log ${DIFF_BASE}..HEAD --oneline
CHANGED_FILES="$(git diff ${DIFF_BASE}..HEAD --name-only)"
git diff ${DIFF_BASE}..HEAD --stat
```

Save branch name, changed files list, commit summary, and DIFF_BASE.

### Set Shared Protocol Variables

```bash
REVIEW_TYPE="impl"
REVIEW_ENTITY_ID="<TASK_ID>"
REVIEW_SUMMARY="Impl review for $BRANCH: <1-2 sentence description of changes>"
RECEIPT_TYPE="impl_review"
PARSE_SOURCE="impl-review"
STATUS_CMD_SHIP=""   # impl-review does not update epic status
STATUS_CMD_FAIL=""   # impl-review does not update epic status
FIX_ACTION="impl"   # Fix code + run tests + commit
```

**REVIEW_CONTEXT** — the git diff, changed files list, and commit summary from Phase 1.

**PROMPT_CRITERIA** — impl-review-specific review template:

```
## Changes Under Review
Branch: [BRANCH_NAME]
Files: [LIST CHANGED FILES]
Commits: [COMMIT SUMMARY]

## Original Spec
[PASTE flowctl show OUTPUT if known]

## Review Focus
[USER'S FOCUS AREAS]

## Review Structure: Two-Layer Review

You MUST conduct the review in two distinct layers, in order. Do NOT merge them.

### Layer 1: Spec Compliance

Answer: "Did the implementation build what was asked -- nothing more, nothing less?"

For each spec requirement, check:
- [ ] Implemented as specified (not a different interpretation)
- [ ] No gold-plating (features not in spec)
- [ ] No missing requirements (spec items not implemented)
- [ ] No scope creep (changes outside the spec boundary)

If spec is unknown, skip Layer 1 and proceed to Layer 2.

### Layer 2: Code Quality

Answer: "Is the implementation well-built?"

1. **Correctness** - Logic errors? Off-by-one? Null handling?
2. **Simplicity** - Simplest solution? Over-engineering?
3. **DRY** - Duplicated logic? Existing patterns ignored?
4. **Architecture** - Data flow? Clear boundaries?
5. **Edge Cases** - Failure modes? Race conditions?
6. **Tests** - Adequate coverage? Testing behavior not implementation?
7. **Security** - Injection? Auth gaps? Input validation?

## Scenario Exploration (for changed code only)

Walk through these scenarios mentally for any new/modified code paths:

- [ ] Happy path - Normal operation with valid inputs
- [ ] Invalid inputs - Null, empty, malformed data
- [ ] Boundary conditions - Min/max values, empty collections
- [ ] Concurrent access - Race conditions, deadlocks
- [ ] Network issues - Timeouts, partial failures
- [ ] Resource exhaustion - Memory, disk, connections
- [ ] Security attacks - Injection, overflow, DoS vectors
- [ ] Data corruption - Partial writes, inconsistency
- [ ] Cascading failures - Downstream service issues

Only flag issues that apply to the **changed code** - not pre-existing patterns.

## Output Format

Structure output as two sections:

### Spec Compliance
- PASS or list of spec gaps (with file:line references)

### Code Quality
For each issue:
- **Severity**: Critical / Major / Minor / Nitpick
- **File:Line**: Exact location
- **Problem**: What's wrong
- **Suggestion**: How to fix

**Structured findings (preferred):** Return findings as a JSON array inside a <findings> block. Each finding must include the fields below. Suppress findings with confidence < 0.6 unless severity is P0.

<findings>
[
  {
    "title": "Short description of the issue",
    "severity": "P0 | P1 | P2 | P3",
    "file": "path/to/file.rs",
    "line": 42,
    "why_it_matters": "Explain the real-world impact",
    "confidence": 0.95,
    "autofix_class": "safe_auto | gated_auto | manual | advisory",
    "evidence": ["grep output", "test failure", "spec reference"]
  }
]
</findings>

**Backward compatibility:** If structured JSON is not possible, free-text findings are still accepted by parse-findings.

**REQUIRED**: You MUST end your response with exactly one verdict tag. This is mandatory:
<verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict> or <verdict>MAJOR_RETHINK</verdict>

Verdict rules:
- Any spec compliance gap -> NEEDS_WORK (regardless of code quality)
- Any Critical code quality issue -> NEEDS_WORK
- Only Minor/Nitpick issues remaining -> SHIP

Do NOT skip this tag. The automation depends on it.
```

### Execute Review

**Follow `skills/_shared/rp-review-protocol.md`** — RP Backend: context_builder Review (Steps 1-3) and Fix Loop.

Use `context_builder(instructions=REVIEW_CONTEXT + PROMPT_CRITERIA, response_type="review")` for initial review. On NEEDS_WORK, use `oracle_send(chat_id, message)` for re-reviews.

### Impl-Specific Fix Actions

When fixing NEEDS_WORK issues:
1. **Verify before fixing** - For each issue, check:
   - Will this change break existing tests or functionality? If yes, skip with a note in re-review.
   - Is the suggested addition actually used? (YAGNI check)
   - Do you understand the suggestion? If unclear, skip it and note "unclear, skipped" in re-review.
2. **Fix verified issues** - Address each in order (blocking -> simple -> complex)
3. **Run tests/lints** - Verify fixes don't break anything
4. **Commit fixes**: `git add -A && git commit -m "fix: address review feedback"`

---

**For anti-patterns and general protocol rules, see `skills/_shared/rp-review-protocol.md`.**
