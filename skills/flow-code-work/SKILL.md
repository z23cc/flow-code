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
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl"
$FLOWCTL <command>
```

**Hard requirements (non-negotiable):**
- You MUST run `flowctl done` for each completed task and verify the task status is `done`.
- You MUST stage with `git add -A` (never list files). This ensures `.flow/` and `scripts/ralph/` (if present) are included.
- Do NOT claim completion until `flowctl show <task>` reports `status: done`.
- Do NOT invoke `/flow-code:impl-review` until tests/Quick commands are green.

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

## FIRST: Parse Options or Ask Questions

Check configured backend:
```bash
REVIEW_BACKEND=$($FLOWCTL review-backend)
```
Returns: `ASK` (not configured), or `rp`/`codex`/`none` (configured).

### Option Parsing (skip questions if found in arguments)

Parse the arguments for these patterns. If found, use them and skip corresponding questions:

**Branch mode**:
- `--branch=current` or `--current` or "current branch" or "stay on this branch" → current branch
- `--branch=new` or `--new-branch` or "new branch" or "create branch" → new branch
- `--branch=worktree` or `--worktree` or "isolated worktree" or "worktree" → isolated worktree

**Review mode**:
- `--review=codex` or "review with codex" or "codex review" or "use codex" → Codex CLI (GPT 5.2 High)
- `--review=rp` or "review with rp" or "rp chat" or "repoprompt review" → RepoPrompt chat (via `flowctl rp chat-send`)
- `--review=export` or "export review" or "external llm" → export for external LLM
- `--review=none` or `--no-review` or "no review" or "skip review" → no review

**Parallel mode**:
- `--parallel` or "run tasks in parallel" or "parallel execution" → spawn all ready tasks simultaneously with git worktree isolation; branches merged back after batch completes (only for EPIC_MODE, skipped for single task)

**Teams mode** (requires `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`):
- `--teams` or "use teams" or "team mode" or "agent teams" → spawn workers as Agent Team teammates with inter-worker communication via SendMessage and file ownership enforcement. Implies parallel execution. Only for EPIC_MODE. Falls back to `--parallel` if teams feature unavailable.

**Interactive mode**:
- `--interactive` or "step by step" or "pause between tasks" → pause for human confirmation at each checkpoint (post-plan, post-impl, post-review). Default: off (autonomous). When enabled, print checkpoint summary and wait for user confirmation before proceeding to next phase.

**TDD mode**:
- `--tdd` or "test first" or "test driven" or "red green refactor" → enforce test-first development. Worker writes failing tests before implementation code. Default: off. When enabled, worker executes Phase 2a (TDD Red-Green) before Phase 2 (Implement).

### If options NOT found in arguments

**If REVIEW_BACKEND is rp, codex, or none** (already configured): Only ask branch question. Show override hint:

```
Quick setup: Where to work?
a) Current branch  b) New branch  c) Isolated worktree

(Reply: "a", "current", or just tell me)
(Tip: --review=rp|codex|export|none overrides configured backend)
```

**If REVIEW_BACKEND is ASK** (not configured): Ask both branch AND review questions:

```
Quick setup before starting:

1. **Branch** — Where to work?
   a) Current branch
   b) New branch
   c) Isolated worktree

2. **Review** — Run Carmack-level review after?
   a) Codex CLI
   b) RepoPrompt
   c) Export for external LLM
   d) None (configure later with --review flag)

(Reply: "1a 2a", "current branch, codex", or just tell me naturally)
```

Wait for response. Parse naturally — user may reply terse or ramble via voice.

**Defaults when empty/ambiguous:**
- Branch = `new`
- Review = configured backend if set, else `none` (no auto-detect fallback)

**Do NOT read files or write code until user responds.**

## Workflow

After setup questions answered, read [phases.md](phases.md) and execute each phase in order.

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

- Don't start without asking branch question
- Don't start without plan/epic
- Don't skip tests
- Don't leave tasks half-done
- Never use TodoWrite for task tracking
- Never create plan files outside `.flow/`
