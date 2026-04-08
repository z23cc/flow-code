---
name: flow-code-work
description: "Use when implementing a plan or working through a spec. Triggers on /flow-code:work with Flow IDs."
tier: 3
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

If no input provided and invoked standalone: ask for it.
If invoked from `/flow-code:go` pipeline: input is always the epic ID from the plan phase — never block on user input.

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
  - Layer 1 (guard): runs per-commit in Worker Phase 6 — always on
  - Layer 3 (codex adversarial): runs at epic completion in Step 14 — auto-detects codex CLI
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

## Handling Worker Messages

When workers send messages via SendMessage, the coordinator (this skill) handles them by summary prefix:

| Worker Summary Prefix | Coordinator Action |
|----------------------|-------------------|
| **"Task complete: fn-N.M"** | Verify via `$FLOWCTL show <id> --json` (status=done). Unlock files. Update wave progress. |
| **"Blocked: fn-N.M"** | Log reason. Move to next wave. Task stays blocked for manual review. |
| **"Spec conflict: fn-N.M"** | Read the conflict details. Then either: (a) fix the spec and reply `SendMessage(summary: "Spec updated: fn-N.M", message: "<what changed>")` — worker re-anchors and resumes; or (b) skip the task via `$FLOWCTL task skip <id> --reason "<why>"` and reply `SendMessage(summary: "Task skipped: fn-N.M")` — worker stops. |
| **"Need file access: path"** | Check lock registry. Grant or deny via `SendMessage(summary: "Access granted: path")` or `SendMessage(summary: "Access denied: path")`. |
| **"Need mutation: fn-N.M"** | Evaluate the mutation request. Execute via `$FLOWCTL task split/skip/dep rm` if appropriate, then reply with result. |

**Spec conflict resolution is NOT optional.** If a worker reports a spec conflict, the coordinator must respond within 120s or the worker will self-block.

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

## Review Backend Resolution

All review phases use the same priority chain to determine which backend to use:

1. `--review=<backend>` flag (highest priority, overrides everything)
2. `FLOW_REVIEW_BACKEND` environment variable
3. `.flow/config.json` → `review.backend` setting
4. Default: `none` (skip review)

| Review Phase | Supported Backends | Notes |
|-------------|-------------------|-------|
| Plan Review | rp, codex, export, none | RP uses context_builder; Codex uses codex CLI |
| Impl Review | rp, codex, none | Per-task review during work phase |
| Epic Review | rp, codex, none | Adversarial review at epic completion |

The `--no-review` flag is equivalent to `--review=none` and always wins over config settings.

### Review Circuit Breakers

| Review Phase | Max Iterations | Rationale |
|-------------|---------------|-----------|
| Plan Review | 2 | Plans are high-level; 2 rounds suffice for spec alignment |
| Impl Review | 3 | Code fixes may need multiple passes for correctness |
| Epic Review | 2 | Adversarial review catches systemic issues; fixes are broad |

After max iterations, the pipeline proceeds with a warning. This prevents infinite NEEDS_WORK loops.

## Guardrails

- Don't start without plan/epic
- Don't skip tests
- Don't leave tasks half-done
- Never use TodoWrite for task tracking
- Never create plan files outside `.flow/`
