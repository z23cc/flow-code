# Shared RP Review Protocol

Shared workflow for all three review skills (plan-review, impl-review, epic-review).
This file is the **review-specific extension** of `skills/_shared/rp-mcp-orchestration.md`.
Follow the shared orchestration guide for general rules about builder/oracle ownership, selection management, exports, and delegated agents.

Each calling skill MUST define these variables before following this protocol:

```
Required variables (set by calling skill):

REVIEW_TYPE       — "plan" | "impl" | "epic"
REVIEW_ENTITY_ID  — the epic or task ID being reviewed
REVIEW_SUMMARY    — 1-2 sentence description for context_builder
REVIEW_CONTEXT    — context gathered in Phase 1 (spec, diff, etc.)
PROMPT_CRITERIA   — the review-specific prompt template content
RECEIPT_TYPE      — receipt JSON type field ("plan_review" | "impl_review" | "completion_review")
FIX_ACTION        — what to do on NEEDS_WORK:
                     plan:  update epic spec via flowctl epic plan
                     impl:  fix code + commit
                     epic:  fix code + commit
STATUS_CMD_SHIP   — flowctl command for SHIP verdict
STATUS_CMD_FAIL   — flowctl command for NEEDS_WORK verdict
PARSE_SOURCE      — source tag for parse-findings ("plan-review" | "impl-review" | "epic-review")
```

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

## RP Backend: context_builder Review

Use when `BACKEND="rp"`.

After the calling skill has gathered context (Phase 1) and set all required variables, use `context_builder` with `response_type="review"` for the review.
Per the shared orchestration guide, `context_builder` owns the initial discovery + selection and Oracle follow-ups stay in the same chat unless review scope materially changes.

### Step 1: Execute Review via context_builder

Build instructions for context_builder that include the review context and criteria:

```
instructions = """
${REVIEW_CONTEXT}

---

${PROMPT_CRITERIA}
"""
```

Call context_builder:

```
context_builder(
  instructions=<instructions above>,
  response_type="review"
)
```

This returns the review response with a `chat_id` for follow-up messages.

Save the returned `chat_id` and response text.

**WAIT** for response. Takes 1-5+ minutes.

### Step 2: Parse Verdict

Extract verdict from response:

```bash
VERDICT="$(echo "$REVIEW_RESPONSE" \
  | tr -d '\r' \
  | grep -oE '<verdict>(SHIP|NEEDS_WORK|MAJOR_RETHINK)</verdict>' \
  | tail -n 1 \
  | sed -E 's#</?verdict>##g')"

if [[ -z "$VERDICT" ]]; then
  echo "No verdict tag found in response"
  echo "<promise>RETRY</promise>"
  exit 0
fi

echo "VERDICT=$VERDICT"
```

### Step 3: Receipt + Status

#### Write receipt (if REVIEW_RECEIPT_PATH set)

```bash
if [[ -n "${REVIEW_RECEIPT_PATH:-}" ]]; then
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  mkdir -p "$(dirname "$REVIEW_RECEIPT_PATH")"
  cat > "$REVIEW_RECEIPT_PATH" <<EOF
{"type":"${RECEIPT_TYPE}","id":"${REVIEW_ENTITY_ID}","mode":"rp","verdict":"${VERDICT}","timestamp":"$ts"}
EOF
  echo "REVIEW_RECEIPT_WRITTEN: $REVIEW_RECEIPT_PATH"
fi
```

#### Update status

```bash
# If SHIP
${STATUS_CMD_SHIP}

# If NEEDS_WORK or MAJOR_RETHINK
${STATUS_CMD_FAIL}
```

---

## Fix Loop

**CRITICAL: Do NOT ask user for confirmation. Automatically fix ALL valid issues and re-review. Never use AskUserQuestion in this loop.**

**CRITICAL: You MUST apply fixes BEFORE re-reviewing. Never re-review without making changes.**

**MAX ITERATIONS**: Limit fix+re-review cycles to **${MAX_REVIEW_ITERATIONS:-3}** iterations (default 3, configurable in Ralph's config.env). If still NEEDS_WORK after max rounds, output `<promise>RETRY</promise>` and stop.

If verdict is NEEDS_WORK:

1. **Parse issues** - Extract ALL issues by severity (Critical -> Major -> Minor). Register findings as gaps:
   ```bash
   echo "$REVIEW_RESPONSE" > /tmp/review-response.txt
   FINDINGS_RESULT="$($FLOWCTL parse-findings --file /tmp/review-response.txt --epic "$EPIC_ID" --register --source ${PARSE_SOURCE} --json)"
   REGISTERED="$(echo "$FINDINGS_RESULT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("registered",0))' 2>/dev/null || echo 0)"
   echo "Registered $REGISTERED findings as gaps"
   ```

2. **Apply fixes** - Execute the `FIX_ACTION` for this review type:
   - **plan**: Update epic spec via `$FLOWCTL epic plan <EPIC_ID> --file - --json`. Sync affected task specs.
   - **impl**: Fix code, run tests, commit: `git add -A && git commit -m "fix: address review feedback"`
   - **epic**: Fix code, run tests, commit: `git add -A && git commit -m "fix: address completion review gaps"`

3. **Re-review** via the same Oracle chat (`ask_oracle` / `oracle_send`, depending on host surface) — no new context build:
   ```
   # use `ask_oracle` or `oracle_send`, depending on what your host surface exposes
   ask_oracle(
     chat_id=<chat_id from Step 1>,
     message="Issues addressed. Please re-review.\n\n**REQUIRED**: End with <verdict>SHIP</verdict> or <verdict>NEEDS_WORK</verdict> or <verdict>MAJOR_RETHINK</verdict>"
   )
   ```

4. **Repeat** until SHIP or max iterations reached.

---

## Anti-patterns

**All backends:**
- **Reviewing yourself** - You coordinate; the backend reviews
- **No receipt** - If REVIEW_RECEIPT_PATH is set, you MUST write receipt
- **Ignoring verdict** - Must extract and act on verdict tag
- **Mixing backends** - Stick to one backend for the entire review session

**RP backend only:**
- **Re-running context_builder** - After initial review, use the same Oracle chat for follow-ups unless review scope truly changed
- **Summarizing fixes in re-review** - RP auto-refreshes file contents; just request re-review

**Codex backend only:**
- **Using `--last` flag** - Conflicts with parallel usage; use `--receipt` instead
- **Direct codex calls** - Must use `flowctl codex` wrappers
