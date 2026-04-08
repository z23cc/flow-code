# Step 1: Resolve Input

**CRITICAL**: If you are about to create:
- a markdown TODO list,
- a task list outside `.flow/`,
- or any plan files outside `.flow/`,

**STOP** and instead:
- create/update tasks in `.flow/` using `flowctl`,
- record details in the epic/task spec markdown.

## Setup

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Detect Input Type

Detect input type in this order (first match wins):

1. **Flow task ID** `fn-N-slug.M` (e.g., fn-1-add-oauth.3) or legacy `fn-N.M`/`fn-N-xxx.M` -> **SINGLE_TASK_MODE**
2. **Flow epic ID** `fn-N-slug` (e.g., fn-1-add-oauth) or legacy `fn-N`/`fn-N-xxx` -> **EPIC_MODE**
3. **Spec file** `.md` path that exists on disk -> **EPIC_MODE**
4. **Idea text** everything else -> **EPIC_MODE**

**Track the mode** — it controls looping in the Wave Loop (Steps 3-13).

---

### Flow task ID (fn-N-slug.M or legacy fn-N.M/fn-N-xxx.M) -> SINGLE_TASK_MODE

- Read task: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get epic from task data for context: `$FLOWCTL show <epic-id> --json && $FLOWCTL cat <epic-id>`
- **This is the only task to execute** — no loop to next task

### Flow epic ID (fn-N-slug or legacy fn-N/fn-N-xxx) -> EPIC_MODE

- Clear auto-execute marker (confirms work has started): `$FLOWCTL epic auto-exec <id> --done --json`
- Read epic: `$FLOWCTL show <id> --json`
- Read spec: `$FLOWCTL cat <id>`
- Get first ready task: `$FLOWCTL ready <id> --json`

### Spec file start (.md path that exists)

1. Check file exists: `test -f "<path>"` — if not, treat as idea text
2. Initialize: `$FLOWCTL init --json`
3. Read file and extract title from first `# Heading` or use filename
4. Create epic: `$FLOWCTL epic create --title "<extracted-title>" --json`
5. Set spec from file: `$FLOWCTL epic plan <epic-id> --file <path> --json`
6. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <title>" --json`
7. Continue with epic-id

### Spec-less start (idea text)

1. Initialize: `$FLOWCTL init --json`
2. Create epic: `$FLOWCTL epic create --title "<idea>" --json`
3. Create single task: `$FLOWCTL task create --epic <epic-id> --title "Implement <idea>" --json`
4. Continue with epic-id

## Next Step

Read `steps/step-02-setup.md` and execute.
