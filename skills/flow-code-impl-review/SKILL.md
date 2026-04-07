---
name: flow-code-impl-review
description: "Use when reviewing code changes, PRs, or implementations. Triggers on /flow-code:impl-review."
user-invocable: false
---

# Implementation Review Mode

**Read [workflow.md](workflow.md) for detailed phases and anti-patterns.**

Conduct a John Carmack-level review of implementation changes on the current branch.

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
1. **DO NOT REVIEW CODE YOURSELF** - you coordinate, RepoPrompt reviews
2. **MUST WAIT for actual RP response** - never simulate/skip the review
3. **MUST use `setup-review`** - handles window selection + builder atomically
4. **DO NOT add --json flag to chat-send** - it suppresses the review response
5. **Re-reviews MUST stay in SAME chat** - omit `--new-chat` after first review

**For codex backend:**
1. Use `$FLOWCTL codex impl-review` exclusively
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
Format: `[task ID] [--base <commit>] [focus areas]`

- `--base <commit>` - Compare against this commit instead of main/master (for task-scoped reviews)
- Task ID - Optional, for context and receipt tracking
- Focus areas - Optional, specific areas to examine

**Scope behavior:**
- With `--base`: Reviews only changes since that commit (task-scoped)
- Without `--base`: Reviews entire branch vs main/master (full branch review)

## Workflow

**See [workflow.md](workflow.md) for full details on each backend.**

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
```

### Step 0: Parse Arguments

Parse $ARGUMENTS for:
- `--base <commit>` → `BASE_COMMIT` (if provided, use for scoped diff)
- First positional arg matching `fn-*` → `TASK_ID`
- Remaining args → focus areas

If `--base` not provided, `BASE_COMMIT` stays empty (will fall back to main/master).

### Step 1: Detect Backend

Run backend detection from SKILL.md above. Then branch:

### Codex Backend

```bash
RECEIPT_PATH="${REVIEW_RECEIPT_PATH:-/tmp/impl-review-receipt.json}"

# Use BASE_COMMIT if provided, else fall back to main
if [[ -n "$BASE_COMMIT" ]]; then
  $FLOWCTL codex impl-review "$TASK_ID" --base "$BASE_COMMIT" --receipt "$RECEIPT_PATH"
else
  $FLOWCTL codex impl-review "$TASK_ID" --base main --receipt "$RECEIPT_PATH"
fi
# Output includes VERDICT=SHIP|NEEDS_WORK|MAJOR_RETHINK
```

On NEEDS_WORK: fix code, commit, re-run (receipt enables session continuity).

### RepoPrompt Backend

**⚠️ STOP: You MUST read and execute [workflow.md](workflow.md) now.**

Go to the "RepoPrompt Backend Workflow" section in workflow.md and execute those steps. Do not proceed here until workflow.md phases are complete.

The workflow covers:
1. Identify changes (use `BASE_COMMIT` if provided)
2. Atomic setup (setup-review) → sets `$W` and `$T`
3. Augment selection and build review prompt
4. Send review and parse verdict

**Return here only after workflow.md execution is complete.**

## Fix Loop (INTERNAL - do not exit to Ralph)

**CRITICAL: Do NOT ask user for confirmation. Automatically fix ALL valid issues and re-review — our goal is production-grade world-class software and architecture. Never use AskUserQuestion in this loop.**

**MAX ITERATIONS** (severity-based limits):
- **P0/P1 findings** (critical/major): max **3** fix iterations
- **P2/P3 findings** (minor/trivial): max **2** fix iterations
- **Subjective findings** (naming, style, architecture opinion): max **1** iteration, then defer

Default fallback: **${MAX_REVIEW_ITERATIONS:-3}** iterations. If still NEEDS_WORK after max rounds, stop the fix loop and return to the worker with status NEEDS_WORK — the worker will report SPEC_CONFLICT.

### Finding Classification

Classify each finding as **deterministic** or **subjective**:
- **Deterministic**: lint error, type error, missing test, compilation failure, spec violation, security flaw — objectively verifiable
- **Subjective**: naming preference, architecture opinion, style choice, code organization suggestion — reasonable people disagree

If all remaining unresolved findings are **subjective**, issue **SHIP** verdict with recorded concerns rather than continuing the fix loop. Log: "Review circuit breaker: all remaining findings are subjective. Issuing SHIP with recorded concerns."

### Regression Detection

Track the **finding count per iteration**. If iteration N+1 produces **MORE** findings than iteration N, the fixes are introducing new problems. Break the loop immediately with:
> "Review circuit breaker: regression detected (findings increased from X to Y). Stopping fix loop."

### Oscillation Detection

Compare finding titles/descriptions across iterations. If a finding from iteration N **reappears** in iteration N+2 (was fixed then reintroduced), break immediately with:
> "Review circuit breaker: oscillation detected (finding 'X' reappeared). Stopping fix loop."

### Fix Loop Steps

If verdict is NEEDS_WORK, loop internally until SHIP:

1. **Parse issues** from reviewer feedback (Critical → Major → Minor)
2. **Classify findings** as deterministic or subjective (see above)
3. **Check regression**: compare finding count against previous iteration — break if increased
4. **Check oscillation**: compare finding descriptions against all prior iterations — break if any reappeared
5. **Fix code** and run tests/lints
6. **Commit fixes** (mandatory before re-review)
7. **Re-review**:
   - **Codex**: Re-run `flowctl codex impl-review` (receipt enables context)
   - **RP**: `$FLOWCTL rp chat-send --window "$W" --tab "$T" --message-file /tmp/re-review.md` (NO `--new-chat`)
8. **Repeat** until `<verdict>SHIP</verdict>` or circuit breaker triggers

**CRITICAL**: For RP, re-reviews must stay in the SAME chat so reviewer has context. Only use `--new-chat` on the FIRST review.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Reviewer said fix, so it must be right" | Review feedback needs technical verification too. Reviewers can be wrong |
| "This is just cosmetic" | 3 cosmetic issues often signal a structural problem underneath |
| "3 iterations is overkill for this" | The circuit breaker exists for a reason. If you hit it, the plan was wrong |
| "I already fixed the important ones" | "Important" is defined by severity, not your estimate of effort |
| "The code works, reviewer is being pedantic" | Working code is the minimum bar, not the finish line |

## Red Flags

- Same issue appears across 3+ review iterations (underlying design problem)
- Fix introduces a new Critical finding
- SHIP verdict issued with zero test evidence
- Review only checks code quality but ignores spec compliance
- ≥3 Critical findings in a single review pass

## Verification

- [ ] All Critical findings addressed with specific fixes
- [ ] No fix introduced new Critical or Important issues
- [ ] Tests still pass after all fixes applied
- [ ] Review receipt saved to .flow/reviews/
- [ ] Acceptance criteria verified (not just code quality)
