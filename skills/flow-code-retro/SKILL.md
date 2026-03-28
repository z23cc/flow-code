---
name: flow-code-retro
description: Use after completing an epic or major feature to capture structured lessons learned, review what worked and what didn't
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
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl"

# Epic summary
$FLOWCTL show <epic-id> --json
$FLOWCTL cat <epic-id>

# All tasks and their evidence
$FLOWCTL tasks --epic <epic-id> --json

# Git history for the epic
git log --oneline <epic-branch>..HEAD
```

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
