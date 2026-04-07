---
name: flow-code-worktree-kit
description: Manage git worktrees (create/list/switch/cleanup) and copy .env files. Use for parallel feature work, isolated review, clean workspace, or when user mentions worktrees.
tier: 1
---

# Worktree kit

Use the manager script for all worktree actions.

```bash
bash ${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/skills/flow-code-worktree-kit/scripts/worktree.sh <command> [args]
```

Commands:
- `create <name> [base]`
- `list`
- `switch <name>` (prints path)
- `cleanup`
- `copy-env <name>`
- `merge-back <branch> [target]` — merge branch into target (default: current), --no-ff, abort on conflict
- `remove <name> [--delete-branch]` — non-interactive single worktree removal

Safety notes:
- `create` does not change the current branch
- `cleanup` does not force-remove worktrees and does not delete branches
- `cleanup` deletes the worktree directory (including ignored files); removal fails if the worktree is not clean
- `.env*` is copied with no overwrite (symlinks skipped)
- refuses to operate if `.worktrees/` or any worktree path component is a symlink
- `copy-env` only targets registered worktrees
- `origin` fetch is optional; local base refs are allowed
- fetch from `origin` only when base looks like a branch
- Worktrees live under `.worktrees/`
- `remove` does not force-remove; fails if worktree has uncommitted changes
- `merge-back` aborts and restores on conflict (no partial merges)
