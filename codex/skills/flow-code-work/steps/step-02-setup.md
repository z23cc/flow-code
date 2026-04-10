# Step 2: Branch & Environment Setup

## Apply Branch Choice

- **Worktree** (default when on main): use `skill: flow-code-worktree-kit` to create an isolated worktree. This keeps main clean and allows parallel work.
- **Current branch** (default when on feature branch or dirty tree): proceed in place.
- **New branch** (only if explicitly requested via `--branch=new`):
  ```bash
  git checkout main && git pull origin main
  git checkout -b <branch>
  ```

## Wave Loop Overview

**Default mode: Worktree + RP agent_run** — each worker gets an isolated git worktree registered as an RP workspace, spawned via `agent_run`. Worktree provides kernel-level file isolation; RP provides coordination (`steer` for mid-run instructions, `wait`/`poll` for monitoring, `cancel` for timeout).

**CRITICAL: When multiple tasks are ready, they MUST run in parallel. Do NOT execute them sequentially "for quality" or "one at a time." Parallel execution with isolation IS the quality mechanism.**

## Next Step

Read `steps/step-03-find-ready.md` and execute.
