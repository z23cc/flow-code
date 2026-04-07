---
name: flow-code-autoplan
description: "Use when doing comprehensive plan review from multiple perspectives. Chains CEO/product, engineering, design, and DX reviews sequentially. Triggers on /flow-code:autoplan, 'full review', 'multi-perspective review'."
tier: 4
user-invocable: true
---

# Multi-Perspective Autoplan Review

Chains four review perspectives on the current epic's plan, scores each 0-10, and applies auto-decision logic to produce a SHIP/NEEDS_WORK verdict.

**CRITICAL: flowctl is BUNDLED.** Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Input

Arguments: $ARGUMENTS

Accepts:
- Flow epic ID: `fn-1-add-oauth` (review that epic's plan)
- Empty: auto-detect the current active epic

## Phase 0: Resolve Epic

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"

# If an epic ID was provided, use it; otherwise find the active one
if [[ -n "$ARGUMENTS" ]]; then
  EPIC_ID="$ARGUMENTS"
else
  EPIC_ID=$($FLOWCTL epics --json | python3 -c "
import json, sys
epics = json.load(sys.stdin).get('epics', [])
active = [e for e in epics if e['status'] == 'open']
if active:
    print(active[0]['id'])
else:
    print('')
" 2>/dev/null)
fi

if [[ -z "$EPIC_ID" ]]; then
  echo "No epic found. Provide an epic ID or create one with /flow-code:run"
  exit 1
fi

echo "Reviewing epic: $EPIC_ID"
```

## Phase 1: Gather Context

Read the epic spec and task list so all four perspectives have the same input.

```bash
# Epic spec (the plan)
EPIC_SPEC=$($FLOWCTL cat "$EPIC_ID")

# Task breakdown
TASKS_JSON=$($FLOWCTL tasks --epic "$EPIC_ID" --json)

# Git context (recent changes)
GIT_LOG=$(git log --oneline -10 2>/dev/null || echo "no git history")
```

Present a brief summary of what will be reviewed:

```
Reviewing plan for: <epic title>
Tasks: <count>
Perspectives: Product/CEO, Engineering, Design, DX
```

## Phase 2: Run Four Perspectives

Execute each perspective sequentially. For each, evaluate the plan against the criteria and assign a score from 0 to 10.

---

### Perspective 1: Product/CEO Review (Scope and Value)

Evaluate the epic plan as a product leader would:

**Criteria:**
- **Right problem?** Does the plan solve the actual user pain, or is it a solution looking for a problem?
- **Scope appropriate?** Is it too narrow (misses obvious value) or too ambitious (scope creep risk)?
- **User value?** Does this create real, measurable value for users? What's the before/after?
- **Prioritization?** Is this the highest-leverage thing to build right now?

**Output format:**
```
## 1. Product/CEO Review

**Score: X/10**

**Assessment:**
<2-3 sentences on scope, value, and problem-solution fit>

**Issues (if any):**
- <specific issue with suggested fix>

**What would make this a 10:**
<one sentence describing the gap to a perfect score>
```

---

### Perspective 2: Engineering Review (Architecture and Execution)

Evaluate the plan as a senior engineer would:

**Criteria:**
- **Architecture sound?** Does the technical approach fit the codebase? Are there simpler alternatives?
- **Dependencies correct?** Are task dependencies properly ordered? Any missing or circular deps?
- **Edge cases covered?** Does the plan account for error handling, concurrency, data migration?
- **Task breakdown realistic?** Are tasks appropriately sized? Any that should be split or merged?
- **Testing strategy?** Are test requirements specified? Is the testing approach adequate?

**Output format:**
```
## 2. Engineering Review

**Score: X/10**

**Assessment:**
<2-3 sentences on architecture, dependencies, and execution risk>

**Issues (if any):**
- <specific technical issue with suggested fix>

**What would make this a 10:**
<one sentence describing the gap to a perfect score>
```

---

### Perspective 3: Design Review (UX and Consistency)

Evaluate any user-facing aspects of the plan. If the epic has no UI or user-facing changes, score N/A.

**Criteria:**
- **User-facing changes well-thought-out?** Are interactions, flows, and feedback states described?
- **Consistent with existing patterns?** Does it follow established UI conventions in the project?
- **Error states and edge cases?** What does the user see when things go wrong?
- **Accessibility?** Are a11y considerations addressed (if applicable)?

**Output format:**
```
## 3. Design Review

**Score: X/10** (or **N/A** if no user-facing changes)

**Assessment:**
<2-3 sentences on UX quality and consistency, or "No user-facing changes in this plan.">

**Issues (if any):**
- <specific UX issue with suggested fix>

**What would make this a 10:**
<one sentence describing the gap to a perfect score>
```

---

### Perspective 4: DX Review (Developer Experience)

Evaluate the plan from the perspective of the developer who will implement it:

**Criteria:**
- **Easy to implement?** Is the plan clear enough to implement without ambiguity? Are specs detailed?
- **Testing strategies adequate?** Are testing approaches defined? Are they practical?
- **Maintenance burden?** Will this create ongoing maintenance cost? Tech debt implications?
- **Documentation?** Are docs updated as part of the plan? Will future contributors understand this?

**Output format:**
```
## 4. DX Review

**Score: X/10**

**Assessment:**
<2-3 sentences on implementability, testing, and maintenance>

**Issues (if any):**
- <specific DX issue with suggested fix>

**What would make this a 10:**
<one sentence describing the gap to a perfect score>
```

---

## Phase 3: Auto-Decision Logic

Collect all four scores and apply the decision rules.

### Score Collection

```
Product/CEO:  X/10
Engineering:  X/10
Design:       X/10 (or N/A)
DX:           X/10
```

Compute the effective scores (exclude N/A perspectives from the calculation).

### Decision Rules

| Condition | Verdict | Action |
|-----------|---------|--------|
| ALL scores >= 7 | **SHIP** | Auto-approve with summary |
| ANY score < 5 | **NEEDS_WORK** | Auto-reject with specific issues |
| All scores 5-10, at least one in 5-6 range | **TASTE** | Present to user for decision |

### SHIP Path

If all scores are 7 or above:

```
## Verdict: SHIP

All perspectives scored 7+. Plan is ready for implementation.

| Perspective | Score |
|-------------|-------|
| Product/CEO | X/10  |
| Engineering | X/10  |
| Design      | X/10  |
| DX          | X/10  |

**Summary:** <one sentence overall assessment>

Next step: Run `/flow-code:run <epic-id>` to begin implementation.
```

### NEEDS_WORK Path

If any score is below 5:

```
## Verdict: NEEDS_WORK

One or more perspectives scored below 5. The plan needs revision.

| Perspective | Score | Status |
|-------------|-------|--------|
| Product/CEO | X/10  | OK / NEEDS_WORK |
| Engineering | X/10  | OK / NEEDS_WORK |
| Design      | X/10  | OK / NEEDS_WORK |
| DX          | X/10  | OK / NEEDS_WORK |

**Critical Issues:**
1. <issue from lowest-scoring perspective>
2. <issue from next lowest-scoring perspective>

**Suggested Fixes:**
1. <actionable fix for issue 1>
2. <actionable fix for issue 2>

Next step: Revise the plan and re-run `/flow-code:autoplan <epic-id>`.
```

### TASTE Path (Scores in 5-6 Range)

If all scores are between 5 and 10, but at least one falls in the 5-6 range (the "taste zone"):

```
## Verdict: YOUR CALL

Scores are mixed. Some perspectives raised concerns that may or may not be blockers depending on your priorities.

| Perspective | Score | Status |
|-------------|-------|--------|
| Product/CEO | X/10  | OK / BORDERLINE |
| Engineering | X/10  | OK / BORDERLINE |
| Design      | X/10  | OK / BORDERLINE |
| DX          | X/10  | OK / BORDERLINE |

**Borderline Issues:**
1. <issue from borderline perspective, with context on why it might be acceptable>

**Options:**
- **Proceed as-is**: Accept the tradeoffs and start implementation
- **Revise first**: Address the borderline issues before building
- **Partial fix**: Address only the critical borderline items
```

Use `AskUserQuestion` to get the user's decision:
> The plan scores are in the taste zone (5-6 on some dimensions). Would you like to (1) proceed as-is, (2) revise the plan first, or (3) address only the critical items?

## Phase 4: Write Receipt

If `REVIEW_RECEIPT_PATH` is set (by a calling pipeline), write a receipt:

```bash
if [[ -n "${REVIEW_RECEIPT_PATH:-}" ]]; then
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  mkdir -p "$(dirname "$REVIEW_RECEIPT_PATH")"
  cat > "$REVIEW_RECEIPT_PATH" <<EOF
{"type":"autoplan_review","id":"${EPIC_ID}","verdict":"${VERDICT}","scores":{"product":${PRODUCT_SCORE},"engineering":${ENG_SCORE},"design":"${DESIGN_SCORE}","dx":${DX_SCORE}},"timestamp":"$ts"}
EOF
  echo "Receipt written: $REVIEW_RECEIPT_PATH"
fi
```

## Anti-Patterns

| Mistake | Fix |
|---------|-----|
| Scoring without reading the spec | Always read the full epic spec and task list first |
| Giving all 10s to avoid conflict | Each perspective should be genuinely critical; a score of 10 means zero issues |
| N/A on design when there ARE user-facing changes | CLI output, error messages, and config formats are user-facing too |
| Skipping the taste zone conversation | When scores are 5-6, the user MUST decide; don't auto-resolve |
| Reviewing code instead of the plan | This reviews the plan/spec, not implementation; use `/flow-code:run` impl-review for code |
| Running all perspectives in parallel | Perspectives run sequentially so each can build on prior observations |

## Examples

```bash
# Review a specific epic
/flow-code:autoplan fn-5-add-auth

# Review the current active epic
/flow-code:autoplan

# After revision, re-review
/flow-code:autoplan fn-5-add-auth
```
