---
name: flow-code-retro
description: Use after completing an epic or major feature to capture structured lessons learned, review what worked and what didn't
tier: 4
context: fork
---

# Epic Retrospective

Structured post-epic review that extracts actionable lessons and persists them to project memory.

## When to Use

- After `/flow-code:epic-review` returns SHIP
- After completing a significant feature (even without formal epic)
- When prompted: "retro", "retrospective", "what did we learn", "lessons learned"

## Process

### 1. Gather Evidence

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"

# Epic summary
$FLOWCTL show <epic-id> --json
$FLOWCTL cat <epic-id>

# All tasks and their evidence
$FLOWCTL tasks --epic <epic-id> --json

# Git history for the epic
git log --oneline <epic-branch>..HEAD
```

### 1b. Analyze Review Feedback Patterns

Read review receipts from `.flow/reviews/` to extract issue patterns:

```bash
ls .flow/reviews/*-<epic-id>.*-*.json 2>/dev/null
```

For each receipt file, read the `review` field (contains full reviewer feedback):

```bash
cat .flow/reviews/<receipt-file>
```

**Extract from each review:**
- Issue categories (security, test coverage, error handling, types, performance, architecture)
- Severity distribution (Critical / Major / Minor)
- Which files were flagged most often
- Whether the same issue type appears across multiple tasks

**Pattern detection — ask:**
- Did 2+ tasks get NEEDS_WORK for the same reason? → That's a **systemic gap**
- Are certain file paths flagged repeatedly? → Needs refactoring or better specs
- Is one issue category dominant (e.g., 80% test coverage)? → Planning should require it upfront

Save systemic patterns as memory in Step 4.

### 1c. Analyze Task Duration

Check task execution times for anomalies:

```bash
# Show all tasks with runtime state (includes duration_seconds)
$FLOWCTL tasks --epic <epic-id> --json
# For each task, get full state:
$FLOWCTL show <task-id> --json
```

**Flag anomalies:**
- Tasks taking >3x the median duration → What went wrong? Spec too vague? Dependencies missing?
- Tasks with 0 duration → Likely skipped or force-completed
- Overall epic duration vs task count → Is per-task time increasing (sign of growing complexity)?

Note duration anomalies in the summary (Step 3) and save insights as memory if the cause is non-obvious.

### 2. Analyze Three Dimensions

**What went well:**
- Tasks that completed smoothly (no review rework, no spec conflicts)
- Patterns that should be repeated
- Tools/approaches that saved time

**What didn't go well:**
- Tasks that required multiple review cycles (NEEDS_WORK count)
- Spec conflicts that caused worker SPEC_CONFLICT returns
- Guard failures that weren't caught early
- Unexpected dependencies or scope changes

**What to change:**
- Spec writing improvements (missing acceptance criteria, vague descriptions)
- Planning gaps (missing dependencies, wrong task sizing)
- Stack config adjustments (missing guard commands)
- Process improvements

### 3. Generate Summary

Output format:

```markdown
## Retrospective: <epic-title>

### Stats
- Tasks: N total, M first-pass SHIP, K required rework
- Review cycles: total across all tasks
- Spec conflicts: count and which tasks
- Duration: total Xm, median Ym/task, slowest: fn-N.M (Zm)
- Review patterns: top issue categories (e.g., "test coverage: 3 tasks, security: 1 task")

### What Went Well
- [bullet points]

### What Didn't Go Well
- [bullet points]

### Action Items
- [ ] [specific, actionable improvements]
```

### 4. Persist to Memory

For each non-obvious lesson, save to project memory:

```bash
# Pitfalls discovered
$FLOWCTL memory add pitfall "<lesson>"

# Conventions discovered
$FLOWCTL memory add convention "<pattern>"

# Decisions made
$FLOWCTL memory add decision "<choice and rationale>"
```

**Rules:**
- Only save lessons that apply beyond this specific epic
- Don't save obvious things ("tests should pass")
- 1-3 entries per retro is normal; zero is fine if nothing surprising
- Check existing memory first to avoid duplicates:
  ```bash
  $FLOWCTL memory list --json
  ```

### 4b. Verify Existing Memory (Staleness Check)

Review existing entries and verify they're still valid:

```bash
$FLOWCTL memory list --json
```

For each entry, ask: "Is this still true given what we learned in this epic?"

- **Still valid** → verify it:
  ```bash
  $FLOWCTL memory verify <id>
  ```
- **No longer valid** (code changed, approach superseded) → remove or update:
  ```bash
  # Remove outdated entry
  $FLOWCTL memory gc --days 0 --dry-run  # preview
  ```
- **Entries marked [stale]** (not verified in 90+ days) deserve extra scrutiny

**Rules:**
- Don't blindly verify everything — actually consider each entry
- 0-3 verifications per retro is normal
- If an entry is wrong, removing it is better than leaving stale knowledge

### 5. Suggest Process Improvements

If analysis reveals systemic issues, suggest:
- Stack config changes (`flowctl config set stack.*`)
- Planning template improvements
- New guard commands
- Skill updates

### 6. Next Steps

```
Retro complete. Next:
1) Start next epic: `/flow-code:plan <idea>`
2) Check project readiness: `/flow-code:prime`
3) View all epics: `flowctl epics --json`
```
