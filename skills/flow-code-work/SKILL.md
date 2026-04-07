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
FLOWCTL="$HOME/.flow/bin/flowctl"
$FLOWCTL <command>
```

**Hard requirements (non-negotiable):**
- You MUST run `flowctl done` for each completed task and verify the task status is `done`.
- You MUST stage with `git add -A` (never list files). This ensures `.flow/` and `scripts/ralph/` (if present) are included.
- Do NOT claim completion until `flowctl show <task>` reports `status: done`.
- Do NOT invoke `/flow-code:impl-review` until tests/Quick commands are green.
- Default parallel mode: Worktree isolation + Teams coordination (both always active).
  Workers spawn in isolated worktrees with TeamCreate + team_name + coordination loop.

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
- `--branch=current|new|worktree`, `--review=rp|codex|export|none`, `--interactive`, `--tdd`, `--rp-context`, `--no-rp-context`

Proceed to Workflow immediately.

## Workflow

Read [phases.md](phases.md) and execute each phase in order.

**Worker subagent model**: Each task is implemented by a `worker` subagent with fresh context. This prevents context bleed between tasks and keeps re-anchor info with the implementation. The main conversation handles task selection and looping; worker handles implementation, commits, and reviews.

If user chose review, pass the review mode to the worker. The worker invokes `/flow-code:impl-review` after implementation and loops until SHIP.

**Completion review gate**: When all tasks in an epic are done, if `--require-completion-review` is configured (via `flowctl next`), the work skill invokes `/flow-code:epic-review` before allowing the epic to close. This verifies the combined implementation satisfies the spec. The epic-review skill handles the fix loop internally until SHIP.

## Teams Mode: Approval Protocol

When TEAM_MODE=true, workers request permission for out-of-ownership edits or DAG
mutations via a two-tier protocol:

**Approval API path (Teams mode):**
- Worker calls `flowctl approval create --task <id> --kind file_access|mutation --payload '{...}'`
  to register a pending approval.
- Worker blocks on `flowctl approval show <id> --wait --timeout 600`, which polls
  until the approval resolves (or times out after ≤10 minutes).
- Supervisor resolves via `flowctl approval approve|reject <id>`.
- On `status: approved` the worker proceeds; on `status: rejected` the worker
  emits a `Blocked:` summary and finds an alternative.

**SendMessage path (non-Teams mode):**
- Worker sends `SendMessage(summary: "Need file access: …")` or `"Need mutation: …"`.
- Team lead responds with `"Access granted:"` / `"Access denied:"` summary-prefix reply.

See `agents/worker.md` for the full protocol.

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

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "Spec is good enough, skip re-anchor" | Specs change during plan-review. Always re-read before implementing |
| "Guard passed, definitely no issues" | Guards may not cover your specific change. Manual verification still needed |
| "Small change, no test needed" | Small changes are the #1 source of regressions |
| "I'll test after all tasks are done" | Cross-task integration bugs are 10x harder to diagnose than per-task bugs |
| "One more attempt will fix it" | 3+ failed attempts = architectural problem. Escalate, don't retry |

## Red Flags

- ≥2 tasks failed in the same wave (systemic issue, stop and investigate)
- Same file needs 3+ fixes across iterations
- Guard passes but manual test reveals broken behavior
- Worker reports "spec unclear" but continues implementing
- >100 lines changed without running any tests
- Wave checkpoint skipped or results not reviewed

## Verification

- [ ] All tasks in epic are done or explicitly skipped
- [ ] Guards pass on final state (`flowctl guard`)
- [ ] Evidence files exist for each completed task
- [ ] No unresolved spec conflicts
- [ ] Wave checkpoints completed for each wave
