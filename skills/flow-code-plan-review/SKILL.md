---
name: flow-code-plan-review
description: "Use when reviewing Flow epic specs or design docs. Triggers on /flow-code:plan-review."
user-invocable: false
deprecated: true
---

**Deprecated**: Use `/flow-code:run` instead.

# Plan Review Mode

**Read [workflow.md](workflow.md) for detailed phases and anti-patterns.**

Conduct a John Carmack-level review of epic plans.

**Role**: Code Review Coordinator (NOT the reviewer)
**Backends**: RepoPrompt (rp) or Codex CLI (codex)

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Backend Selection

**Priority** (first match wins):
1. `--review=rp|codex|export|none` argument
2. `FLOW_REVIEW_BACKEND` env var (`rp`, `codex`, `none`)
3. `.flow/config.json` → `review.backend`
4. **Error** - no auto-detection

### Parse from arguments first

Check $ARGUMENTS for:
- `--review=rp` or `--review rp` → use rp
- `--review=codex` or `--review codex` → use codex
- `--review=export` or `--review export` → use export
- `--review=none` or `--review none` → skip review

If found, use that backend and skip all other detection.

### Otherwise read from config

```bash
# Priority: --review flag > env > config
BACKEND=$($FLOWCTL review-backend)

if [[ "$BACKEND" == "ASK" ]]; then
  echo "Error: No review backend configured."
  echo "Run /flow-code:setup to configure, or pass --review=rp|codex|none"
  exit 1
fi

echo "Review backend: $BACKEND (override: --review=rp|codex|none)"
```

## Critical Rules

**For rp backend:**
1. **DO NOT REVIEW THE PLAN YOURSELF** - you coordinate, RepoPrompt reviews
2. **MUST WAIT for actual RP response** - never simulate/skip the review
3. **MUST use `setup-review`** - handles window selection + builder atomically
4. **DO NOT add --json flag to chat-send** - it suppresses the review response
5. **Re-reviews MUST stay in SAME chat** - omit `--new-chat` after first review

**For codex backend:**
1. Use `$FLOWCTL codex plan-review` exclusively
2. Pass `--receipt` for session continuity on re-reviews
3. Parse verdict from command output

**For all backends:**
- If `REVIEW_RECEIPT_PATH` set: write receipt after review (any verdict)
- Any failure → output `<promise>RETRY</promise>` and stop

**FORBIDDEN**:
- Self-declaring SHIP without actual backend verdict
- Mixing backends mid-review (stick to one)
- Skipping review when backend is "none" without user consent

## Input

Arguments: $ARGUMENTS
Format: `<flow-epic-id> [focus areas]`

## Capability Gaps Pre-Check

**Before any backend runs the review**, verify capability-scout output:

```bash
EPIC_ID="${1:-}"
CAP_GAPS_FILE=".flow/epics/${EPIC_ID}/capability-gaps.md"

if [[ -f "$CAP_GAPS_FILE" ]]; then
  echo "Capability gaps file present: $CAP_GAPS_FILE"
  # Check for unresolved required gaps in the registry
  UNRESOLVED=$($FLOWCTL gap list --epic "$EPIC_ID" --json 2>/dev/null \
    | python3 -c 'import sys,json; d=json.load(sys.stdin); print(sum(1 for g in d if g.get("source")=="capability-scout" and g.get("priority")=="required" and not g.get("resolved")))' 2>/dev/null || echo "0")
  if [[ "$UNRESOLVED" -gt 0 ]]; then
    echo "BLOCK SHIP: $UNRESOLVED unresolved required capability gap(s). Resolve via 'flowctl gap resolve' or downgrade priority with justification before SHIP."
    # Record as a blocking finding; do not exit — let reviewer also see context
  fi
fi
```

**Rules:**
- If `capability-gaps.md` is missing AND capability-scout was not explicitly skipped (`--no-capability-scan`), note as a warning but do not block (scout may have failed open).
- If unresolved `required`-priority gaps with `source=capability-scout` exist in the gap registry, the final verdict MUST NOT be SHIP until gaps are resolved or downgraded with justification.
- Downgrade path: `flowctl gap resolve <gap-id>` after addressing, OR epic spec must explicitly justify why the gap is acceptable (and gap re-registered at lower priority).

Include the capability-gaps.md contents (if present) in the context sent to the backend reviewer so it can factor gaps into its verdict.

## Workflow

**See [workflow.md](workflow.md) for full details on each backend.**

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
```

### Step 0: Detect Backend

Run backend detection from SKILL.md above. Then branch:

### Codex Backend

```bash
EPIC_ID="${1:-}"
RECEIPT_PATH="${REVIEW_RECEIPT_PATH:-/tmp/plan-review-receipt.json}"

# Save checkpoint before review (recovery point if context compacts)
$FLOWCTL checkpoint save --epic "$EPIC_ID" --json

# --files: comma-separated CODE files for reviewer context
# Epic/task specs are auto-included; pass files the plan will CREATE or MODIFY
# How to identify: read the epic spec, find files mentioned or directories affected
# Example: epic touches auth → pass existing auth files for context
#
# Dynamic approach (if epic mentions specific paths):
#   CODE_FILES=$(grep -oE 'src/[^ ]+\.(ts|py|js)' .flow/specs/${EPIC_ID}.md | sort -u | paste -sd,)
# Or list key files manually:
CODE_FILES="src/main.py,src/config.py"

$FLOWCTL codex plan-review "$EPIC_ID" --files "$CODE_FILES" --receipt "$RECEIPT_PATH"
# Output includes VERDICT=SHIP|NEEDS_WORK|MAJOR_RETHINK
```

On NEEDS_WORK: fix plan via `$FLOWCTL epic set-plan` AND sync affected task specs via `$FLOWCTL task spec`, then re-run (receipt enables session continuity).

**Note**: `codex plan-review` automatically includes task specs in the review prompt.

### RepoPrompt Backend

**⚠️ STOP: You MUST read and execute [workflow.md](workflow.md) now.**

Go to the "RepoPrompt Backend Workflow" section in workflow.md and execute those steps. Do not proceed here until workflow.md phases are complete.

The workflow covers:
1. Get plan content and save checkpoint
2. Atomic setup (setup-review) → sets `$W` and `$T`
3. Augment selection (epic + task specs)
4. Send review and parse verdict

**Return here only after workflow.md execution is complete.**

## Fix Loop (INTERNAL - do not exit to Ralph)

**CRITICAL: Do NOT ask user for confirmation. Automatically fix ALL valid issues and re-review — our goal is production-grade world-class software and architecture. Never use AskUserQuestion in this loop.**

**MAX ITERATIONS** (severity-based limits):
- **P0/P1 findings** (critical/major): max **3** fix iterations
- **P2/P3 findings** (minor/trivial): max **2** fix iterations
- **Subjective findings** (naming, style, architecture opinion): max **1** iteration, then defer

Default fallback: **${MAX_REVIEW_ITERATIONS:-3}** iterations (consistent with impl-review and shared protocol). If still NEEDS_WORK after max rounds, stop the fix loop and log: "Plan review: MAX_REVIEW_ITERATIONS reached. Proceeding with remaining concerns as gaps."

### Finding Classification

Classify each finding as **deterministic** or **subjective**:
- **Deterministic**: missing acceptance criteria, spec contradiction, feasibility gap, security concern, missing dependency — objectively verifiable
- **Subjective**: naming preference, architecture opinion, organizational style, scope suggestion — reasonable people disagree

If all remaining unresolved findings are **subjective**, issue **SHIP** verdict with recorded concerns rather than continuing the fix loop. Log: "Review circuit breaker: all remaining findings are subjective. Issuing SHIP with recorded concerns."

### Regression Detection

Track the **finding count per iteration**. If iteration N+1 produces **MORE** findings than iteration N, the fixes are introducing new problems. Break the loop immediately with:
> "Review circuit breaker: regression detected (findings increased from X to Y). Stopping fix loop."

### Oscillation Detection

Compare finding titles/descriptions across iterations. If a finding from iteration N **reappears** in iteration N+2 (was fixed then reintroduced), break immediately with:
> "Review circuit breaker: oscillation detected (finding 'X' reappeared). Stopping fix loop."

### Fix Loop Steps

If verdict is NEEDS_WORK, loop internally until SHIP or circuit breaker triggers:

1. **Parse issues** from reviewer feedback
2. **Classify findings** as deterministic or subjective (see above)
3. **Check regression**: compare finding count against previous iteration — break if increased
4. **Check oscillation**: compare finding descriptions against all prior iterations — break if any reappeared
5. **Fix epic spec** (stdin preferred, temp file if content has single quotes):
   ```bash
   # Preferred: stdin heredoc
   $FLOWCTL epic plan <EPIC_ID> --file - --json <<'EOF'
   <updated epic spec content>
   EOF

   # Or temp file
   $FLOWCTL epic plan <EPIC_ID> --file /tmp/updated-plan.md --json
   ```
6. **Sync affected task specs** - If epic changes affect task specs, update them:
   ```bash
   $FLOWCTL task spec <TASK_ID> --file - --json <<'EOF'
   <updated task spec content>
   EOF
   ```
   Task specs need updating when epic changes affect:
   - State/enum values referenced in tasks
   - Acceptance criteria that tasks implement
   - Approach/design decisions tasks depend on
   - Lock/retry/error handling semantics
   - API signatures or type definitions
7. **Re-review**:
   - **Codex**: Re-run `flowctl codex plan-review` (receipt enables context)
   - **RP**: `$FLOWCTL rp chat-send --window "$W" --tab "$T" --message-file /tmp/re-review.md` (NO `--new-chat`)
8. **Repeat** until `<verdict>SHIP</verdict>` or circuit breaker triggers

**Recovery**: If context compaction occurred during review, restore from checkpoint:
```bash
$FLOWCTL checkpoint restore --epic <EPIC_ID> --json
```

**CRITICAL**: For RP, re-reviews must stay in the SAME chat so reviewer has context. Only use `--new-chat` on the FIRST review.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "The plan looks reasonable, ship it" | Reasonable-looking plans fail at implementation. Review forces you to find the gaps before code exists. |
| "We already discussed this verbally" | Verbal agreement evaporates. Written review catches assumptions that felt obvious in conversation but aren't. |
| "Reviewer doesn't know our codebase" | External perspective catches blind spots. Codebase familiarity causes pattern blindness — fresh eyes find structural issues. |
| "Review is blocking progress" | Review prevents rework. A 30-minute review saves days of implementing the wrong design. |
| "The spec is too detailed to review" | Over-detailed specs hide weak architecture behind volume. If it's too complex to review, it's too complex to implement. |
| "Minor plan issues, we'll fix during implementation" | Plan issues compound during implementation. A wrong assumption in the spec becomes wrong code in every task. |
| "Just one more iteration and it's perfect" | Diminishing returns are real. Hit the circuit breaker, ship what's good enough, and capture remaining concerns as gaps. |

## Red Flags

- Plan approved without any reviewer reading the actual task specs
- SHIP verdict with zero questions asked (rubber-stamp review)
- Review feedback ignored because "we know better"
- Same structural issue found in implementation that was present in the plan (review missed it)
- Plan-review skipped because "we're behind schedule"
- Reviewer only checked formatting, not technical feasibility
