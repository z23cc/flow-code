---
name: flow-code-work
description: "Use when implementing a plan or working through a spec. Triggers on /flow-code:work with Flow IDs."
user-invocable: false
---

# Flow work

Execute a plan systematically. Focus on finishing.

Follow this skill and linked workflows exactly. Deviations cause drift, bad gates, retries, and user frustration.

**IMPORTANT**: This plugin uses `.flow/` for ALL task tracking. Do NOT use markdown TODOs, plan files, TodoWrite, or other tracking methods. All task state must be read and written via `flowctl`.

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl.py"
$FLOWCTL <command>
```

**Hard requirements (non-negotiable):**
- You MUST run `flowctl done` for each completed task and verify the task status is `done`.
- You MUST stage with `git add -A` (never list files). This ensures `.flow/` and `scripts/ralph/` (if present) are included.
- Do NOT claim completion until `flowctl show <task>` reports `status: done`.
- Do NOT invoke `/flow-code:impl-review` until tests/Quick commands are green.
- When 2+ tasks are ready with no file conflicts, you MUST use Teams mode
  (TeamCreate + team_name + flowctl lock + coordination loop).
- Do NOT spawn independent background agents without team_name.

**Role**: execution lead, plan fidelity first.
**Goal**: complete every task in order with tests.

## Ralph Mode Rules (always follow)

If `REVIEW_RECEIPT_PATH` is set or `FLOW_RALPH=1`:
- **Must** use `flowctl done` and verify task status is `done` before committing.
- **Must** stage with `git add -A` (never list files).
- **Do NOT** use TodoWrite for tracking.

## Input

Full request: $ARGUMENTS

Accepts:
- Flow epic ID `fn-N-slug` (e.g., `fn-1-add-oauth`) or legacy `fn-N`/`fn-N-xxx` to work through all tasks
- Flow task ID `fn-N-slug.M` (e.g., `fn-1-add-oauth.2`) or legacy `fn-N.M`/`fn-N-xxx.M` to work on single task
- Markdown spec file path (creates epic from file, then executes)
- Idea text (creates minimal epic + single task, then executes)
- Chained instructions like "then review with /flow-code:impl-review"

Examples:
- `/flow-code:work fn-1-add-oauth`
- `/flow-code:work fn-1-add-oauth.3`
- `/flow-code:work fn-1` (legacy formats fn-1, fn-1-xxx still supported)
- `/flow-code:work docs/my-feature-spec.md`
- `/flow-code:work Add rate limiting`
- `/flow-code:work fn-1-add-oauth then review via /flow-code:impl-review`

If no input provided, ask for it.

## Context Analysis (replaces setup questions)

Read context before proceeding — no questions asked:
```bash
CURRENT_BRANCH=$(git branch --show-current)
GIT_STATUS=$(git status --porcelain)
REVIEW_BACKEND=$($FLOWCTL review-backend)
```

Based on context, decide:
- **Branch**: on feature branch → stay (`current`). on main/master → create worktree (`worktree`). dirty working tree → `current`.
- **Per-task review**: `none` by default. Three-layer quality system handles review at the right levels:
  - Layer 1 (guard): runs per-commit in worker Phase 2.5 — always on
  - Layer 3 (codex adversarial): runs at epic completion in Phase 3j — auto-detects codex CLI
  - Per-task Codex/RP review only if explicitly requested via `--review=rp|codex`

Output one line:
```
Branch: <current|worktree> | Review: none (Layer 1 guard + Layer 3 adversarial)
```

### Explicit flag overrides

These flags override the corresponding AI decision without entering the analysis flow:
- `--branch=current|new|worktree`, `--review=rp|codex|export|none`, `--interactive`, `--tdd`, `--worktree-parallel`

Proceed to Workflow immediately.

## Workflow

Read [phases.md](phases.md) and execute each phase in order.

**Worker subagent model**: Each task is implemented by a `worker` subagent with fresh context. This prevents context bleed between tasks and keeps re-anchor info with the implementation. The main conversation handles task selection and looping; worker handles implementation, commits, and reviews.

If user chose review, pass the review mode to the worker. The worker invokes `/flow-code:impl-review` after implementation and loops until SHIP.

**Completion review gate**: When all tasks in an epic are done, if `--require-completion-review` is configured (via `flowctl next`), the work skill invokes `/flow-code:epic-review` before allowing the epic to close. This verifies the combined implementation satisfies the spec. The epic-review skill handles the fix loop internally until SHIP.

## Recovery

If a task fails or needs to be re-done after completion:
```bash
# Restart a single task + all downstream dependents
$FLOWCTL restart <task-id>

# Preview what would be reset (no changes)
$FLOWCTL restart <task-id> --dry-run

# Force restart even if task is in_progress
$FLOWCTL restart <task-id> --force
```

## Guardrails

- Don't start without plan/epic
- Don't skip tests
- Don't leave tasks half-done
- Never use TodoWrite for task tracking
- Never create plan files outside `.flow/`
