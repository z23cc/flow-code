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
- Default parallel mode: Worktree isolation + RP agent_run (both always active).
  Workers spawn in manually-created worktrees bound as RP workspaces via agent_run.

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

Execute steps from `steps/` directory one at a time (JIT loading — only read the current step):
1. Read `steps/step-01-resolve-input.md` and execute
2. Read `steps/step-02-setup.md` and execute
3. Read `steps/step-03-find-ready.md` and execute
4. Read `steps/step-04-spawn-workers.md` and execute
5. Read `steps/step-05-wave-checkpoint.md` and execute

**Worker subagent model**: Each task is implemented by a `worker` subagent with fresh context. This prevents context bleed between tasks and keeps re-anchor info with the implementation. The main conversation handles task selection and looping; worker handles implementation, commits, and reviews.

If user chose review, pass the review mode to the worker. The worker invokes `/flow-code:impl-review` after implementation and loops until SHIP.

**Completion review gate**: When all tasks in an epic are done, if `--require-completion-review` is configured (via `flowctl next`), the work skill invokes `/flow-code:epic-review` before allowing the epic to close. This verifies the combined implementation satisfies the spec. The epic-review skill handles the fix loop internally until SHIP.

## RP Session Coordination

Workers are spawned as RP agents via `agent_run` in isolated worktrees. The coordinator monitors and coordinates via RP session operations:

| RP Operation | Purpose | Replaces |
|-------------|---------|----------|
| `agent_run(start, detach:true)` | Spawn worker | `Agent(isolation:"worktree")` |
| `agent_run(wait, session_ids)` | Batch wait for completion | Waiting for Agent tool returns |
| `agent_run(poll, session_id)` | Check individual status | N/A (new capability) |
| `agent_run(steer, session_id)` | Inject instructions mid-run | `SendMessage` |
| `agent_run(cancel, session_id)` | Terminate worker | N/A (new capability) |
| `agent_manage(cleanup_sessions)` | Clean up after wave | N/A (automatic before) |

## Handling Worker Output

When workers complete, parse their session output for structured status:

| Worker STATUS | Coordinator Action |
|--------------|-------------------|
| **complete** | Verify via `$FLOWCTL show <id> --json` (status=done). Unlock files. Merge worktree. |
| **blocked** | Log BLOCK_REASON. Skip task in wave. Clean up worktree. |
| **spec_conflict** | Read details. Fix spec and `steer` worker: "Spec updated, re-anchor." Or skip task. |
| **needs_file_access** | Check lock registry. `steer` worker: "Access granted: <file>" or "Access denied." |

**Spec conflict resolution via steer:**
```
mcp__RepoPrompt__agent_run({
  op: "steer",
  session_id: "<worker-session>",
  message: "Spec updated: <TASK_ID>. Re-read spec via Phase 2 and resume."
})
```

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
