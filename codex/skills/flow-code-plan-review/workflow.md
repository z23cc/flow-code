# Plan Review Workflow

## Philosophy

The reviewer model only sees selected files. RepoPrompt's Builder discovers context you'd miss (rp backend). Codex uses context hints from flowctl (codex backend).

---

## Phase 0: Backend Detection

**Run this first. Do not skip.**

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
set -e
FLOWCTL="$HOME/.flow/bin/flowctl"
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

### Step 0: Save Checkpoint

**Before review** (protects against context compaction):
```bash
EPIC_ID="${1:-}"
$FLOWCTL checkpoint save --epic "$EPIC_ID" --json
```

### Step 1: Execute Review

```bash
RECEIPT_PATH="${REVIEW_RECEIPT_PATH:-/tmp/plan-review-receipt.json}"

# --files: comma-separated CODE files for reviewer context
# Epic/task specs are auto-included; pass files the plan will CREATE or MODIFY
# Read epic spec to identify affected paths, then list key files
CODE_FILES="src/main.py,src/config.py"  # Customize per epic

$FLOWCTL codex plan-review "$EPIC_ID" --files "$CODE_FILES" --receipt "$RECEIPT_PATH"
```

**Output includes `VERDICT=SHIP|NEEDS_WORK|MAJOR_RETHINK`.**

Codex is instructed to return findings as a JSON array. Each finding must have: `title`, `severity` (P0/P1/P2/P3), `file`, `line`, `why_it_matters`, `confidence` (0.0-1.0), `autofix_class` (safe_auto/gated_auto/manual/advisory), `evidence` (array of strings). Findings with confidence < 0.6 are suppressed unless P0. If structured JSON is not possible, free-text findings are still accepted by parse-findings.

### Step 2: Update Status

```bash
# Based on verdict
$FLOWCTL epic review "$EPIC_ID" ship --json
# OR
$FLOWCTL epic review "$EPIC_ID" needs_work --json
```

### Step 3: Handle Verdict

If `VERDICT=NEEDS_WORK`:
1. Parse issues from output and register as gaps:
   ```bash
   # Save review output to temp file, then register findings as gaps
   echo "$REVIEW_OUTPUT" > /tmp/review-response.txt
   FINDINGS_RESULT="$($FLOWCTL parse-findings --file /tmp/review-response.txt --epic "$EPIC_ID" --register --source plan-review --json)"
   REGISTERED="$(echo "$FINDINGS_RESULT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("registered",0))' 2>/dev/null || echo 0)"
   echo "Registered $REGISTERED findings as gaps"
   ```
2. Fix plan via `$FLOWCTL epic set-plan`
3. Re-run step 1 (receipt enables session continuity)
4. Repeat until SHIP

### Step 4: Receipt

Receipt is written automatically by `flowctl codex plan-review` when `--receipt` provided.
Format: `{"mode":"codex","epic":"<id>","verdict":"<verdict>","session_id":"<thread_id>","timestamp":"..."}`

---

## RepoPrompt Backend Workflow

Use when `BACKEND="rp"`.

**This workflow follows the shared RP review protocol** defined in `skills/_shared/rp-review-protocol.md`. The steps below set plan-review-specific variables, then delegate to the shared protocol.

### Phase 1: Read the Plan (RP)

```bash
$FLOWCTL show <id> --json
$FLOWCTL cat <id>
```

Save output for inclusion in review prompt.

**Save checkpoint** (protects against context compaction during review):
```bash
$FLOWCTL checkpoint save --epic <id> --json
```

### Set Shared Protocol Variables

```bash
REVIEW_TYPE="plan"
REVIEW_ENTITY_ID="<EPIC_ID>"
REVIEW_SUMMARY="Plan review for <EPIC_ID>: <1-2 sentence description>"
RECEIPT_TYPE="plan_review"
PARSE_SOURCE="plan-review"
STATUS_CMD_SHIP='$FLOWCTL epic review <EPIC_ID> ship --json'
STATUS_CMD_FAIL='$FLOWCTL epic review <EPIC_ID> needs_work --json'
FIX_ACTION="plan"  # Update epic spec via flowctl epic plan, sync task specs
```

**REVIEW_CONTEXT** — the epic spec output and task list from Phase 1.

**PROMPT_CRITERIA** — plan-review-specific review template:

```
## Plan Under Review
[PASTE flowctl show OUTPUT]

## Review Focus
[USER'S FOCUS AREAS]

## Review Scope

You are reviewing:
1. **Epic spec** - The high-level plan
2. **Task specs** - Individual task breakdowns

**CRITICAL**: Check for consistency between epic and tasks. Flag if:
- Task specs contradict or miss epic requirements
- Task acceptance criteria don't align with epic acceptance criteria
- Task approaches would need to change based on epic design decisions
- Epic mentions states/enums/types that tasks don't account for

## Review Criteria

Conduct a John Carmack-level review:

1. **Completeness** - All requirements covered? Missing edge cases?
2. **Feasibility** - Technically sound? Dependencies clear?
3. **Parallelizability** - Do independent tasks touch disjoint files? Flag overlapping file scopes that will cause merge conflicts.
4. **Clarity** - Specs unambiguous? Acceptance criteria testable?
5. **Architecture** - Right abstractions? Clean boundaries?
6. **Risks** - Blockers identified? Security gaps? Mitigation?
7. **Scope** - Right-sized? Over/under-engineering?
8. **Task sizing** - M tasks preferred. Flag over-splitting: 7+ tasks? Sequential S tasks that should be combined?
9. **Testability** - How will we verify this works?
10. **Consistency** - Do task specs align with epic spec?

## Output Format

For each issue:
- **Severity**: Critical / Major / Minor / Nitpick
- **Location**: Which task or section (e.g., "fn-1.3 Description" or "Epic Acceptance #2")
- **Problem**: What's wrong
- **Suggestion**: How to fix

**Structured findings (preferred):** Return findings as a JSON array inside a `<findings>` block. Each finding must include the fields below. Suppress findings with confidence < 0.6 unless severity is P0.

<findings>
[
  {
    "title": "Short description of the issue",
    "severity": "P0 | P1 | P2 | P3",
    "file": "path/to/file.rs or spec section",
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

Do NOT skip this tag. The automation depends on it.
```

### Execute Review

**Follow `skills/_shared/rp-review-protocol.md`** — RP Backend: context_builder Review (Steps 1-3) and Fix Loop.

Use `context_builder(instructions=REVIEW_CONTEXT + PROMPT_CRITERIA, response_type="review")` for initial review. On NEEDS_WORK, use `oracle_send(chat_id, message)` for re-reviews.

### Plan-Specific Fix Actions

When fixing NEEDS_WORK issues:
1. Update epic spec: `$FLOWCTL epic plan <EPIC_ID> --file - --json`
2. Sync affected task specs: `$FLOWCTL task spec <TASK_ID> --file - --json`
3. **Recovery**: If context compaction occurred, restore: `$FLOWCTL checkpoint restore --epic <EPIC_ID> --json`

**Anti-pattern**: Re-reviewing without calling `epic plan` first. This wastes reviewer time and loops forever.
**Anti-pattern**: Updating epic spec without syncing affected task specs. Causes reviewer to flag consistency issues again.

---

**For anti-patterns and general protocol rules, see `skills/_shared/rp-review-protocol.md`.**
