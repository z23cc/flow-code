# flowctl CLI Reference

CLI for `.flow/` task tracking. Agents must use flowctl for all writes.

> **Note:** This is the full human reference. Agents should read `.flow/usage.md` (created by `/flow-code:setup`).

## Available Commands

```
init, detect, status, doctor, validate, state-path, review-backend, parse-findings,
guard, worker-prompt, dag, estimate, replay, diff, plan-depth,
config, epic, task, dep, approval, gap, log, memory, outputs, checkpoint,
stack, invariants, ralph, scout-cache, skill, rp, codex, hook, stats, worker-phase,
show, epics, tasks, list, cat, files, lock, unlock, heartbeat, lock-check,
ready, next, queue, start, done, restart, block, fail,
export, import, completions,
graph, find, edit, index
```

## Multi-User Safety

Works out of the box for parallel branches. No setup required.

- **ID allocation**: Scans existing files to determine next ID (merge-safe)
- **Soft claims**: Tasks have `assignee` field to prevent duplicate work
- **Actor resolution**: `FLOW_ACTOR` env → git email → git name → `$USER` → "unknown"
- **Local validation**: `flowctl validate --all` catches issues before commit

**Optional**: Add CI gate with `docs/ci-workflow-example.yml` to block bad PRs.

## File Structure

```
.flow/
├── meta.json               # {schema_version, next_epic}
├── epics/fn-N-slug.md      # Epic state (YAML frontmatter + markdown)
├── specs/fn-N-slug.md      # Epic spec (markdown)
├── tasks/fn-N-slug.M.md    # Task state + spec (YAML frontmatter + markdown)
├── memory/                 # Agent memory (v2 atomic entries)
├── reviews/                # Review receipts (JSON)
├── checklists/             # Machine-readable acceptance checklists
└── .state/                 # SQLite DB (source of truth, in .git/flow-state/)
```

Storage: SQLite is the authoritative store. Markdown files are the human-readable format.
`flowctl import` rebuilds the DB from Markdown; `flowctl export` writes Markdown from DB.

Flowctl accepts schema v1 and v2; new fields are optional and defaulted.

Frontmatter fields:
- Epic: `plan_review`, `completion_review`, `depends_on_epics`, `branch_name`
- Task: `priority`, `domain`, `depends_on`, `files`

## ID Format

- **Epic**: `fn-N-slug` where `slug` is derived from the title (e.g., `fn-1-add-oauth`, `fn-2-fix-login-bug`)
- **Task**: `fn-N-slug.M` (e.g., `fn-1-add-oauth.1`, `fn-2-fix-login-bug.2`)

**Backwards compatibility**: Legacy formats `fn-N` (no suffix) and `fn-N-xxx` (random 3-char suffix) are still supported.

## Commands

### init

Initialize `.flow/` directory.

```bash
flowctl init [--json]
```

### detect

Check if `.flow/` exists and is valid.

```bash
flowctl detect [--json]
```

Output:
```json
{"success": true, "exists": true, "valid": true, "path": "/repo/.flow"}
```

### epic create

Create new epic.

```bash
flowctl epic create --title "Epic title" [--branch "fn-1-epic-title"] [--json]
```

Output:
```json
{"success": true, "id": "fn-1-epic-title", "title": "Epic title", "spec_path": ".flow/specs/fn-1-epic-title.md"}
```

### epic plan

Overwrite epic spec from file.

```bash
flowctl epic plan fn-1 --file plan.md [--json]
```

Use `-` as file to read from stdin.

### epic review

Set plan review status.

```bash
flowctl epic review fn-1 ship|needs_work|unknown [--json]
```

### epic completion

Set completion review status.

```bash
flowctl epic completion fn-1 ship|needs_work|unknown [--json]
```

### epic branch

Set epic branch name.

```bash
flowctl epic branch fn-1 fn-1-epic [--json]
```

### epic close

Close epic (requires all tasks done).

```bash
flowctl epic close fn-1 [--json]
```

### epic set-backend

Set default backend specs for impl/review/sync workers. Used by orchestration products (e.g., flow-swarm).

```bash
flowctl epic set-backend fn-1 --impl codex:gpt-5.4 [--json]
flowctl epic set-backend fn-1 --impl codex:gpt-5.4-high --review claude:opus [--json]
flowctl epic set-backend fn-1 --impl "" [--json]  # Clear impl (inherit from config)
```

Options:
- `--impl SPEC`: Default impl backend (e.g., `codex:gpt-5.4-high`, `claude:opus`)
- `--review SPEC`: Default review backend (e.g., `claude:opus`, `agent:opus-4.5-thinking`)
- `--sync SPEC`: Default sync backend (e.g., `claude:haiku`, `gemini:gemini-2.5-flash`)

Format: `backend:model` where backend is a CLI name and model is backend-specific.

### task create

Create task under epic.

```bash
flowctl task create --epic fn-1 --title "Task title" [--deps fn-1.2,fn-1.3] [--acceptance-file accept.md] [--priority 10] [--json]
```

Output:
```json
{"success": true, "id": "fn-1.4", "epic": "fn-1", "title": "Task title", "depends_on": ["fn-1.2", "fn-1.3"]}
```

### task spec

Set task spec: full file or individual sections.

```bash
# Full spec from file
flowctl task spec fn-1.2 --file spec.md [--json]

# Individual sections
flowctl task spec fn-1.2 --desc desc.md --accept accept.md [--json]

# With investigation targets
flowctl task spec fn-1.2 --investigation targets.md [--json]
```

Options:
- `--file FILE`: Full spec file (replaces entire body)
- `--desc FILE`: Description section file (alias: `--description`)
- `--accept FILE`: Acceptance section file (alias: `--acceptance`)
- `--investigation FILE`: Investigation targets section file

All section flags are optional; supply one or more.

### task reset

Reset task to `todo` status, clearing assignee and completion data.

```bash
flowctl task reset fn-1.2 [--cascade] [--json]
```

Use `--cascade` to also reset dependent tasks within the same epic.

### task set-backend

Set backend specs for impl/review/sync workers. Used by orchestration products (e.g., flow-swarm).

```bash
flowctl task set-backend fn-1.1 --impl codex:gpt-5.4-high [--json]
flowctl task set-backend fn-1.1 --impl codex:gpt-5.4-high --review claude:opus [--json]
flowctl task set-backend fn-1.1 --impl "" [--json]  # Clear impl (inherit from epic/config)
```

Options:
- `--impl SPEC`: Impl backend (e.g., `codex:gpt-5.4-high`, `claude:opus`)
- `--review SPEC`: Review backend (e.g., `claude:opus`, `agent:opus-4.5-thinking`)
- `--sync SPEC`: Sync backend (e.g., `claude:haiku`, `gemini:gemini-2.5-flash`)

Format: `backend:model` where backend is a CLI name and model is backend-specific.

### task show-backend

Show effective backend specs for a task. Reports task-level and epic-level specs only (config-level resolution happens in flow-swarm).

```bash
flowctl task show-backend fn-1.1 [--json]
```

Output (text):
```
impl: codex:gpt-5.4-high (task)
review: claude:opus (epic)
sync: null
```

Output (json):
```json
{
  "success": true,
  "id": "fn-1.1",
  "epic": "fn-1",
  "impl": {"spec": "codex:gpt-5.4-high", "source": "task"},
  "review": {"spec": "claude:opus", "source": "epic"},
  "sync": {"spec": null, "source": null}
}
```

### dep add

Add single dependency to task.

```bash
flowctl dep add fn-1.3 fn-1.2 [--json]
```

Dependencies must be within same epic.

### task set-deps

Set multiple dependencies for a task (convenience command).

```bash
flowctl task set-deps fn-1.3 --deps fn-1.1,fn-1.2 [--json]
```

Equivalent to multiple `dep add` calls. Dependencies must be within same epic.

### gap add

Register a requirement gap on an epic. Idempotent — adding the same capability twice returns the existing gap.

```bash
flowctl gap add --epic fn-1-add-auth --capability "Missing CSRF protection" --priority required --source flow-gap-analyst [--task fn-1-add-auth.2] [--json]
```

Priority: `required` (default), `important`, `nice-to-have`. Gap ID is a content-hash of epic + capability.

### gap list

List gaps for an epic, with optional status filter.

```bash
flowctl gap list --epic fn-1-add-auth [--status open|resolved] [--json]
```

### gap resolve

Mark a gap as resolved with evidence. Idempotent — resolving an already-resolved gap is a no-op.

```bash
flowctl gap resolve --epic fn-1-add-auth --capability "Missing CSRF protection" --evidence "Added in middleware.py:42" [--json]
```

### gap check

Gate check: fails (exit 1) if any `required` or `important` gaps are unresolved. `nice-to-have` gaps do not block.

```bash
flowctl gap check --epic fn-1-add-auth [--json]
# JSON output: {"gate": "pass"|"fail", "open_blocking": [...], ...}
```

Also enforced by `epic close` — closing an epic with unresolved blocking gaps fails unless `--skip-gap-check` is passed.

### show

Show epic or task details.

```bash
flowctl show fn-1 [--json]     # Epic with tasks
flowctl show fn-1.2 [--json]   # Task only
```

Epic output includes `tasks` array with id/title/status/priority/depends_on.

### epics

List all epics.

```bash
flowctl epics [--json]
```

Output:
```json
{"success": true, "epics": [{"id": "fn-1", "title": "...", "status": "open", "tasks": 5, "done": 2}], "count": 1}
```

Human-readable output shows progress: `[open] fn-1: Title (2/5 tasks done)`

### tasks

List tasks, optionally filtered.

```bash
flowctl tasks [--json]                    # All tasks
flowctl tasks --epic fn-1 [--json]        # Tasks for specific epic
flowctl tasks --status todo [--json]      # Filter by status
flowctl tasks --epic fn-1 --status done   # Combine filters
```

Status options: `todo`, `in_progress`, `blocked`, `done`

Output:
```json
{"success": true, "tasks": [{"id": "fn-1.1", "epic": "fn-1", "title": "...", "status": "todo", "priority": null, "depends_on": []}], "count": 1}
```

### list

List all epics with their tasks grouped together.

```bash
flowctl list [--json]
```

Human-readable output:
```
Flow Status: 2 epics, 5 tasks (2 done)

[open] fn-1: Add auth system (1/3 done)
    [done] fn-1.1: Create user model
    [in_progress] fn-1.2: Add login endpoint
    [todo] fn-1.3: Add logout endpoint

[open] fn-2: Add caching (1/2 done)
    [done] fn-2.1: Setup Redis
    [todo] fn-2.2: Cache API responses
```

JSON output:
```json
{"success": true, "epics": [...], "tasks": [...], "epic_count": 2, "task_count": 5}
```

### cat

Print spec markdown (no JSON mode).

```bash
flowctl cat fn-1      # Epic spec
flowctl cat fn-1.2    # Task spec
```

### ready

List tasks ready to start, in progress, and blocked.

```bash
flowctl ready fn-1 [--json]
```

> `--epic` flag is still accepted for backwards compatibility.

Output:
```json
{
  "success": true,
  "epic": "fn-1",
  "actor": "user@example.com",
  "ready": [{"id": "fn-1.3", "title": "...", "depends_on": []}],
  "in_progress": [{"id": "fn-1.1", "title": "...", "assignee": "user@example.com"}],
  "blocked": [{"id": "fn-1.4", "title": "...", "blocked_by": ["fn-1.2"]}]
}
```

### next

Select next plan/work unit.

```bash
flowctl next [--epics-file epics.json] [--require-plan-review] [--require-completion-review] [--json]
```

Output:
```json
{"status":"plan|work|completion_review|none","epic":"fn-12","task":"fn-12.3","reason":"needs_plan_review|needs_completion_review|resume_in_progress|ready_task|none|blocked_by_epic_deps","blocked_epics":{"fn-12":["fn-3"]}}
```

The `--require-completion-review` flag gates epic closure on completion review. When all tasks are done but `completion_review_status != ship`, returns `status: completion_review`.

### start

Start task (set status=in_progress). Sets assignee to current actor.

```bash
flowctl start fn-1.2 [--force] [--note "..."] [--json]
```

Validates:
- Status is `todo` (or `in_progress` if resuming own task)
- Status is not `blocked` unless `--force`
- All dependencies are `done`
- Not claimed by another actor

Use `--force` to skip checks and take over from another actor.
Use `--note` to add a claim note (auto-set on takeover).

### done

Complete task with summary and evidence. Requires `in_progress` status.

```bash
flowctl done fn-1.2 --summary-file summary.md --evidence-json evidence.json [--force] [--json]
```

Use `--force` to skip status check.

Evidence JSON format:
```json
{"commits": [], "tests": ["test_foo"], "prs": ["#42"]}
```

### block

Block a task and record a reason in the task spec.

```bash
flowctl block fn-1.2 --reason-file reason.md [--json]
```

### validate

Validate epic structure (specs, deps, cycles).

```bash
flowctl validate --epic fn-1 [--json]
flowctl validate --all [--json]
```

Single epic output:
```json
{"success": false, "epic": "fn-1", "valid": false, "errors": ["..."], "warnings": [], "task_count": 5}
```

All epics output:
```json
{
  "success": false,
  "valid": false,
  "epics": [{"epic": "fn-1", "valid": true, ...}],
  "total_epics": 2,
  "total_tasks": 10,
  "total_errors": 1
}
```

Checks:
- Epic/task specs exist
- Task specs have required headings
- Task statuses are valid (`todo`, `in_progress`, `blocked`, `done`)
- Dependencies exist and are within epic
- No dependency cycles
- Done status consistency

Exits with code 1 if validation fails (for CI use).

### config

Manage project configuration stored in `.flow/config.json`.

```bash
# Get a config value
flowctl config get memory.enabled [--json]
flowctl config get review.backend [--json]

# Set a config value
flowctl config set memory.enabled true [--json]
flowctl config set review.backend codex [--json]  # rp, codex, or none

# Toggle boolean config
flowctl config toggle memory.enabled [--json]
```

**Available settings:**

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `memory.enabled` | bool | `false` | Enable memory system |
| `planSync.enabled` | bool | `false` | Enable plan-sync after task completion |
| `scouts.github` | bool | `false` | Enable github-scout during planning (requires gh CLI) |
| `review.backend` | string | `null` | Default review backend (`rp`, `codex`, `none`). If unset, review commands require `--review` or `FLOW_REVIEW_BACKEND`. |
| `stack` | object | `{}` | Tech stack profile (auto-detected on `init`). See `flowctl stack show`. |

Priority: `--review=...` argument > `FLOW_REVIEW_BACKEND` env > `.flow/config.json` > error.

No auto-detect. Run `/flow-code:setup` (or `flowctl config set review.backend ...`) to configure.

### guard

Run all test/lint/typecheck commands from the stack config. Auto-detects stack if not configured.

```bash
# Run all guards
flowctl guard [--json]

# Run guards for a specific layer
flowctl guard --layer backend [--json]
flowctl guard --layer frontend [--json]
```

Exits non-zero if any guard fails. Output includes per-command pass/fail status.

Workers use this for baseline check (Phase 2) and verification (Phase 10) — one command replaces manual test/lint/typecheck invocations.

### invariants

Architecture invariant registry — rules that must never be violated.

```bash
# Create template
flowctl invariants init [--force] [--json]

# Show current invariants
flowctl invariants show [--json]

# Run all verify commands from invariants.md
flowctl invariants check [--json]
```

**Format** (in `.flow/invariants.md`):
```markdown
## [Concept Name]
- **Rule:** [what must always hold]
- **Verify:** `shell command that exits 0 if invariant holds`
- **Fix:** [how to fix if violated]
```

Workers check invariants in Phase 2 (baseline) and Phase 10 (verification). Planners check during Step 4 to ensure tasks don't violate constraints.

### stack

Manage the project's tech stack profile. Auto-detected on `init`.

```bash
# Auto-detect from project files (pyproject.toml, package.json, Dockerfile, etc.)
flowctl stack detect [--dry-run] [--json]

# Show current stack config
flowctl stack show [--json]

# Set from JSON file (manual override)
flowctl stack set --file stack.json [--json]
flowctl stack set --file - [--json]   # stdin
```

**Auto-detection** runs during `flowctl init` if no stack is configured. Detects:

| Layer | Frameworks | Commands |
|-------|-----------|----------|
| backend | Django, Flask, FastAPI, Go (Gin/Echo/Fiber) | pytest/ruff/mypy from pyproject.toml |
| frontend | React, Vue, Svelte, Angular + Next.js/Nuxt/Remix | test/lint/typecheck from package.json scripts |
| infra | Docker, Compose, Terraform, Pulumi | — |

Also detects package manager (pnpm/yarn/bun/npm) and adds `cd <subdir> &&` prefix for non-root package.json.

### memory

Manage persistent learnings in `.flow/memory/`.

```bash
# Initialize memory directory
flowctl memory init [--json]

# Add entries
flowctl memory add --type pitfall "Always use flowctl rp wrappers" [--json]
flowctl memory add --type convention "Tests in __tests__ dirs" [--json]
flowctl memory add --type decision "SQLite for simplicity" [--json]

# Query
flowctl memory list [--json]
flowctl memory search "pattern" [--json]
flowctl memory read --type pitfalls [--json]
```

Types: `pitfall`, `convention`, `decision`

### parse-findings

Extract structured findings from review output and optionally register them as gaps.

```bash
# Extract findings from a review output file
flowctl parse-findings --file /tmp/review-output.txt [--json]

# Extract and auto-register as gaps on an epic
flowctl parse-findings --file /tmp/review-output.txt --epic fn-1-add-auth --register --source plan-review [--json]

# Read from stdin
echo "$REVIEW_OUTPUT" | flowctl parse-findings --file - --epic fn-1 --register --source impl-review --json
```

Options:
- `--file FILE` (required): Review text file, or `-` for stdin
- `--epic EPIC_ID`: Required when `--register` is used
- `--register`: Auto-call `gap add` for each critical/major finding
- `--source SOURCE`: Gap source label (default: `manual`). Typical values: `plan-review`, `impl-review`, `epic-review`
- `--json`: JSON output

**Extraction strategy** (tiered, no external deps):
1. Regex `<findings>...</findings>` tag
2. Fallback: bare JSON array `[{...}]`
3. Fallback: markdown code block `` ```json...``` ``
4. Graceful empty: returns `[]` with warning if no findings found

**Severity-to-priority mapping** (used with `--register`):
| Severity | Priority |
|----------|----------|
| critical | required |
| major | important |
| minor | nice-to-have |
| nitpick | nice-to-have |

Output:
```json
{
  "success": true,
  "findings": [
    {
      "title": "Missing input validation",
      "severity": "major",
      "location": "src/auth.py:42",
      "recommendation": "Add input sanitization"
    }
  ],
  "count": 1,
  "registered": 1,
  "warnings": []
}
```

Without `--register`, the `registered` field is omitted.

### rp

RepoPrompt wrappers (preferred for reviews). Requires RepoPrompt 1.5.68+.

**Primary entry point** (handles window selection + builder atomically):

```bash
# Atomic setup - picks window by repo root and creates builder tab
eval "$(flowctl rp setup-review --repo-root "$REPO_ROOT" --summary "Review a plan to ...")"
# Returns: W=<window> T=<tab>

# With --create: auto-creates RP window if none matches (RP 1.5.68+)
eval "$(flowctl rp setup-review --repo-root "$REPO_ROOT" --summary "..." --create)"
```

**Post-setup commands** (use $W and $T from setup-review):

```bash
flowctl rp prompt-get --window "$W" --tab "$T"
flowctl rp prompt-set --window "$W" --tab "$T" --message-file /tmp/review-prompt.md
flowctl rp select-add --window "$W" --tab "$T" path/to/file
flowctl rp chat-send --window "$W" --tab "$T" --message-file /tmp/review-prompt.md
flowctl rp prompt-export --window "$W" --tab "$T" --out /tmp/export.md
```

**Low-level commands** (prefer setup-review instead):

```bash
flowctl rp windows [--json]
flowctl rp pick-window --repo-root "$REPO_ROOT"
flowctl rp ensure-workspace --window "$W" --repo-root "$REPO_ROOT"
flowctl rp builder --window "$W" --summary "Review a plan to ..."
```

### codex

OpenAI Codex CLI wrappers — cross-platform alternative to RepoPrompt.

**Requirements:**
```bash
npm install -g @openai/codex
codex auth
```

**Model:** Uses GPT 5.2 High by default (no user config needed). Override with `FLOW_CODEX_MODEL` env var.

**Commands:**

```bash
# Verify codex is available
flowctl codex check [--json]

# Implementation review (reviews code changes for a task)
flowctl codex impl-review <task-id> --base <branch> [--sandbox <mode>] [--receipt <path>] [--json]
# Example: flowctl codex impl-review fn-1.3 --base main --sandbox auto --receipt /tmp/impl-fn-1.3.json

# Plan review (reviews epic spec before implementation)
flowctl codex plan-review <epic-id> --files <file1,file2,...> [--sandbox <mode>] [--receipt <path>] [--json]
# Example: flowctl codex plan-review fn-1 --files "src/auth.ts,src/config.ts" --sandbox auto --receipt /tmp/plan-fn-1.json
# Note: Epic/task specs are included automatically; --files should be CODE files for repository context.

# Completion review (reviews epic implementation against spec)
flowctl codex completion-review <epic-id> [--sandbox <mode>] [--receipt <path>] [--json]
# Example: flowctl codex completion-review fn-1 --sandbox auto --receipt /tmp/completion-fn-1.json
# Runs after all tasks done; verifies implementation matches spec requirements
```

**How it works:**

1. **Gather context hints** — Analyzes changed files, extracts symbols (functions, classes), finds references in unchanged files
2. **Build review prompt** — Uses same Carmack-level criteria as RepoPrompt (7 criteria each for plan/impl)
3. **Run codex** — Executes `codex exec` with the prompt (or `codex exec resume` for session continuity)
4. **Parse verdict** — Extracts `<verdict>SHIP|NEEDS_WORK|MAJOR_RETHINK</verdict>` from output
5. **Write receipt** — If `--receipt` provided, writes JSON for Ralph gating

**Context hints example:**
```
Changed files: src/auth.py, src/handlers.py
Symbols: authenticate(), UserSession, validate_token()
References: src/middleware.py:45 (calls authenticate), tests/test_auth.py:12
```

**Review criteria (identical to RepoPrompt):**

| Review | Criteria |
|--------|----------|
| Plan | Completeness, Feasibility, Clarity, Architecture, Risks, Scope, Testability |
| Impl | Correctness, Simplicity, DRY, Architecture, Edge Cases, Tests, Security |

**Receipt schema (Ralph-compatible):**

Impl review receipt:
```json
{
  "type": "impl_review",
  "id": "fn-1.3",
  "mode": "codex",
  "verdict": "SHIP",
  "session_id": "thread_abc123",
  "timestamp": "2026-01-11T10:30:00Z"
}
```

Completion review receipt:
```json
{
  "type": "completion_review",
  "id": "fn-1",
  "mode": "codex",
  "verdict": "SHIP",
  "session_id": "thread_xyz456",
  "timestamp": "2026-01-11T10:30:00Z"
}
```

**Session continuity:** Receipt includes `session_id` (thread_id from codex). Subsequent reviews read the existing receipt and resume the conversation, maintaining full context across fix → re-review cycles.

**Embedding budget (`FLOW_CODEX_EMBED_MAX_BYTES`):** Optional limit on the total bytes of file contents embedded into the review prompt (diff excluded). Default `0` (unlimited). Set to a value like `500000` (500KB) to cap prompt size.

**Sandbox mode (`--sandbox`):** Controls Codex CLI's file system access. Available modes:
- `read-only` (default on Unix) — Can only read files
- `workspace-write` — Can write files in workspace
- `danger-full-access` — Full file system access (required for Windows)
- `auto` — Resolves to `danger-full-access` on Windows, `read-only` on Unix

**Windows users:** Codex CLI's `read-only` sandbox blocks ALL shell commands on Windows (including reads). Use `--sandbox auto` or `--sandbox danger-full-access` for Windows compatibility.

**Note:** After plugin update, re-run `/flow-code:setup` or `/flow-code:ralph-init` to get sandbox fixes.

### worker-prompt

Generate a trimmed worker prompt based on mode flags. Used by orchestration to bootstrap worker agents with minimal tokens.

```bash
# Full worker prompt (trimmed by mode flags)
flowctl worker-prompt --task fn-1.1 [--tdd] [--review rp|codex] [--json]

# Bootstrap mode: minimal ~200 token prompt for phase-gate execution
flowctl worker-prompt --task fn-1.1 --bootstrap [--tdd] [--review rp|codex] [--json]
```

Options:
- `--task ID` (required): Task ID for context
- `--bootstrap`: Output minimal ~200 token prompt that instructs the worker to call `worker-phase next` in a loop
- `--tdd`: Include TDD Phase 4 in the prompt
- `--review rp|codex`: Include review Phase 4
- `--team`: Include Teams mode instructions (default in phase-gate)
- `--json`: JSON output with `prompt` and `estimated_tokens` fields

### worker-phase

Phase-gate sequential execution for workers. Workers call `next` to get the current phase instructions, execute them, then call `done` to advance.

```bash
# Get next uncompleted phase
flowctl worker-phase next --task fn-1.1 [--tdd] [--review rp|codex] --json

# Mark phase complete
flowctl worker-phase done --task fn-1.1 --phase <PHASE_ID> [--tdd] [--review rp|codex] --json
```

**Default phase sequence**: `1 → 2 → 5 → 6 → 7 → 10 → 12`
- With `--tdd`: adds Phase 4 (test-first)
- With `--review`: adds Phase 8 (impl-review)
- Canonical order: `1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12`

Phase progress is stored per-task in runtime state. `next` returns `{"phase": "<id>", "content": "...", "all_done": false}`. When all phases are complete, returns `{"phase": null, "all_done": true}`.

### checkpoint

Save and restore epic state (used during review-fix cycles).

```bash
# Save epic state to .flow/.checkpoint-fn-1.json
flowctl checkpoint save --epic fn-1 [--json]

# Restore epic state from checkpoint
flowctl checkpoint restore --epic fn-1 [--json]

# Delete checkpoint
flowctl checkpoint delete --epic fn-1 [--json]
```

Checkpoints preserve full epic + task state. Useful when compaction occurs during plan-review cycles.

### status

Show `.flow/` state summary.

```bash
flowctl status [--json]
```

Output:
```json
{"success": true, "epic_count": 2, "task_count": 5, "done_count": 2, "active_runs": []}
```

Human-readable output shows epic/task counts and any active Ralph runs.

### state-path

Show the resolved state directory path (useful for debugging parallel worktree setups).

```bash
flowctl state-path [--json]
```

Output:
```json
{"success": true, "state_dir": "/repo/.git/flow-state", "source": "git-common-dir"}
```

Source values:
- `env` — `FLOW_STATE_DIR` environment variable
- `git-common-dir` — `git --git-common-dir` (shared across worktrees)
- `fallback` — `.flow/state` (non-git or old git)

### doctor

Run comprehensive state health diagnostics.

```bash
flowctl doctor [--workflow] [--json]
```

Options:
- `--workflow`: Run workflow-specific health checks (backend config, tools, locks)

### review-backend

Get review backend and compare review receipts.

```bash
# Detect configured backend
flowctl review-backend [--json]

# Compare receipts from specific files
flowctl review-backend --compare receipt1.json,receipt2.json [--json]

# Auto-discover receipts for an epic
flowctl review-backend --epic fn-1 [--json]
```

Options:
- `--compare FILES`: Comma-separated review receipt file paths
- `--epic EPIC_ID`: Auto-discover review receipts for epic

### dag

Render ASCII DAG of task dependencies for an epic.

```bash
flowctl dag fn-1 [--json]
```

Also available via `flowctl status --dag --epic fn-1`.

### estimate

Estimate remaining time for an epic based on historical task durations.

```bash
flowctl estimate fn-1 [--json]
```

> `--epic` flag is still accepted for backwards compatibility.

### replay

Replay an epic: reset all tasks to `todo` for re-execution.

```bash
flowctl replay fn-1 [--dry-run] [--force] [--json]
```

Options:
- `--dry-run`: Show what would be reset without doing it
- `--force`: Allow replay even if tasks are `in_progress`

### diff

Show git diff summary for an epic's branch.

```bash
flowctl diff fn-1 [--json]
```

### plan-depth

Classify request depth for adaptive plan step selection.

```bash
flowctl plan-depth --request "Add OAuth support" [--json]
```

### approval

Approval commands for requesting/resolving blocking decisions (Teams mode).

```bash
# Create a pending approval
flowctl approval create --task fn-1.2 --kind file_access --payload '{"file":"src/auth.rs"}' [--json]
flowctl approval create --task fn-1.2 --kind mutation --payload @request.json [--json]

# List approvals
flowctl approval list [--pending] [--json]

# Show a single approval (optionally wait for resolution)
flowctl approval show <id> [--wait] [--timeout 300] [--json]

# Approve or reject
flowctl approval approve <id> [--json]
flowctl approval reject <id> [--reason "..."] [--json]
```

Approval kinds: `file_access`, `mutation`, `generic`. Payload accepts inline JSON or `@path/to/file.json`.

### log

Decision logging for workflow traceability.

```bash
# Record a decision
flowctl log decision --key "review_backend" --value "rp-mcp" --reason "RP available" [--epic fn-1] [--task fn-1.2] [--json]

# Query stored decisions
flowctl log decisions [--epic fn-1] [--limit 20] [--json]
```

### outputs

Narrative handoff between tasks. Workers write outputs in Phase 9; successors read them during Phase 2 re-anchor.

```bash
# Write output for a task (from file or stdin)
flowctl outputs write fn-1.3 --file output.md [--json]
flowctl outputs write fn-1.3 --file - [--json]  # stdin

# List outputs for an epic (newest-first)
flowctl outputs list --epic fn-1 [--limit 10] [--json]

# Show full output content
flowctl outputs show fn-1.3 [--json]
```

### ralph

Ralph autonomous run control.

```bash
flowctl ralph pause [--run <id>] [--json]
flowctl ralph resume [--run <id>] [--json]
flowctl ralph stop [--run <id>] [--json]
flowctl ralph status [--run <id>] [--json]
```

Run ID is auto-detected if only one active run exists.

### scout-cache

Scout result cache commands. Caches keyed by scout type + git commit hash.

```bash
# Get cached result
flowctl scout-cache get --scout-type repo [--commit <hash>] [--json]

# Set (cache) a result
flowctl scout-cache set --scout-type repo --result '{"findings":[]}' [--commit <hash>] [--json]
flowctl scout-cache set --scout-type capability --result @result.json [--json]

# Clear all cached results
flowctl scout-cache clear [--json]
```

Commit hash auto-detected from HEAD if omitted. Result accepts inline JSON or `@path/to/file.json`.

### skill

Skill registry commands with semantic vector search.

```bash
# Scan and register skills from skills/*/SKILL.md
flowctl skill register [--dir /path/to/plugin] [--json]

# Semantic search against registered skills
flowctl skill match "implement OAuth" [--limit 5] [--threshold 0.70] [--json]
```

Options for `match`:
- `--limit N`: Maximum results (default: 5)
- `--threshold F`: Minimum cosine similarity (default: 0.70)

### hook

Claude Code hook scripts (invoked automatically by hooks.json, not manually).

```bash
flowctl hook auto-memory        # Extract session memories (Stop hook)
flowctl hook ralph-guard        # Enforce Ralph workflow rules
flowctl hook commit-gate        # Gate commit on guard pass
flowctl hook pre-compact        # Inject .flow/ state into compaction
flowctl hook subagent-context   # Inject active task context for subagents
flowctl hook task-completed     # Sync Claude task completion with .flow/
flowctl hook rtk-rewrite        # Rewrite Bash commands via rtk optimizer
```

All hooks read JSON from stdin and use exit codes 0 (allow) and 2 (block).

### stats

Stats dashboard with summary, trends, tokens, and DORA metrics.

```bash
# Overall summary
flowctl stats summary [--json]

# Per-epic breakdown
flowctl stats epic [--id fn-1] [--json]

# Weekly trends
flowctl stats weekly [--weeks 8] [--json]

# Token/cost breakdown
flowctl stats tokens [--epic fn-1] [--json]

# Bottleneck analysis
flowctl stats bottlenecks [--limit 10] [--json]

# DORA metrics
flowctl stats dora [--json]

# Maintenance
flowctl stats rollup [--json]    # Generate monthly rollups
flowctl stats cleanup [--json]   # Delete old events/rollups
```

### files

Show file ownership map for an epic.

```bash
flowctl files fn-1 [--json]
```

> `--epic` flag is still accepted for backwards compatibility.

Shows which tasks own which files and detects ownership conflicts.

### lock

Lock files for a task (Teams mode). Prevents other workers from modifying locked files.

```bash
flowctl lock --task fn-1.2 --files src/auth.rs,src/config.rs [--mode write] [--json]
```

Options:
- `--task ID` (required): Task ID that owns the files
- `--files PATHS` (required): Comma-separated file paths
- `--mode MODE`: Lock mode — `read`, `write`, or `directory_add` (default: `write`)

### unlock

Unlock files for a task (Teams mode).

```bash
flowctl unlock --task fn-1.2 [--files src/auth.rs] [--json]
flowctl unlock --all [--json]
```

Options:
- `--task ID`: Task ID to unlock files for
- `--files PATHS`: Specific files to unlock (all task files if omitted)
- `--all`: Clear ALL file locks (used between waves)

### heartbeat

Extend lock TTL for a task (Teams mode heartbeat).

```bash
flowctl heartbeat --task fn-1.2 [--json]
```

### lock-check

Check file lock status (Teams mode).

```bash
flowctl lock-check [--file src/auth.rs] [--json]
```

Shows all active locks, or lock state for a specific file.

### queue

Show multi-epic queue status.

```bash
flowctl queue [--json]
```

### fail

Mark task as failed. Triggers `upstream_failed` propagation to downstream dependents.

```bash
flowctl fail fn-1.2 [--reason "..."] [--force] [--json]
```

Options:
- `--reason TEXT`: Reason for failure
- `--force`: Skip status checks

### restart

Restart task and cascade-reset downstream dependents.

```bash
flowctl restart fn-1.2 [--dry-run] [--force] [--json]
```

Options:
- `--dry-run`: Show what would be reset without doing it
- `--force`: Allow restart even if tasks are `in_progress`

### dep rm

Remove a dependency between tasks.

```bash
flowctl dep rm fn-1.3 fn-1.2 [--json]
```

### task skip

Skip a task (mark as permanently skipped). Downstream deps treat skipped as satisfied.

```bash
flowctl task skip fn-1.2 [--reason "Not needed after refactor"] [--json]
```

### task split

Split a task into sub-tasks (runtime DAG mutation).

```bash
flowctl task split fn-1.2 --titles "Parse config|Validate config|Apply config" [--chain] [--json]
```

Options:
- `--titles TEXT` (required): Sub-task titles separated by `|`
- `--chain`: Chain sub-tasks sequentially (each depends on the previous)

### epic reopen

Reopen a closed epic.

```bash
flowctl epic reopen fn-1 [--json]
```

### epic title

Rename an epic's title.

```bash
flowctl epic title fn-1 --title "New title" [--json]
```

### epic archive

Archive a closed epic to `.flow/.archive/`.

```bash
flowctl epic archive fn-1 [--force] [--json]
```

Options:
- `--force`: Archive even if not closed

### epic clean

Archive all closed epics at once.

```bash
flowctl epic clean [--json]
```

### epic audit

Audit epic task-coverage vs original spec (advisory only).

```bash
flowctl epic audit fn-1 [--force] [--json]
```

Assembles epic spec, task list, and prior audit context into a payload for the epic-auditor agent. Writes to `.flow/reviews/epic-audit-<id>-<timestamp>.json`. Never mutates epic/tasks/gaps.

Options:
- `--force`: Force a new audit even if a recent (<24h) receipt exists

### epic add-dep

Add epic-level dependency.

```bash
flowctl epic add-dep fn-2 fn-1 [--json]
```

Makes `fn-2` depend on `fn-1`.

### epic rm-dep

Remove epic-level dependency.

```bash
flowctl epic rm-dep fn-2 fn-1 [--json]
```

### epic auto-exec

Set or clear auto-execute pending marker.

```bash
flowctl epic auto-exec fn-1 --pending [--json]
flowctl epic auto-exec fn-1 --done [--json]
```

### export

Export epics/tasks from DB to Markdown files.

```bash
flowctl export [--epic fn-1] [--format md] [--json]
```

Exports all epics if `--epic` is omitted.

### import

Import epics/tasks from Markdown files into DB (alias for reindex).

```bash
flowctl import [--json]
```

Rebuilds the DB from `.flow/` Markdown files.

### write-file

Write content to a file. Pipeline helper that bypasses Claude Code permission prompts.

```bash
# Inline content
flowctl write-file --path "path/to/file.md" --content "content here" --json

# Stdin (for long content via heredoc)
cat <<'EOF' | flowctl write-file --path "path/to/file.md" --stdin --json
Long content here...
EOF

# Append mode
flowctl write-file --path "path/to/file.md" --content "new line" --append --json
```

Options:
- `--path` (required) — Target file path. Creates parent directories if needed.
- `--content` — Inline content string
- `--stdin` — Read content from stdin
- `--append` — Append instead of overwrite

### checklist

Structured Definition of Done checklists for tasks. 8 default items across 4 categories (context, implementation, testing, documentation).

Subcommands:
- `checklist init --task <id>` — Create default DoD checklist for a task
- `checklist check --task <id> --item <key>` — Mark item as checked
- `checklist uncheck --task <id> --item <key>` — Unmark item
- `checklist verify --task <id>` — Verify all items; exits 1 if any missing
- `checklist show --task <id>` — Display current checklist state

Storage: `.flow/checklists/<task-id>.json`

### graph

Persistent code graph with symbol references and impact analysis. Stored at `.flow/graph.bin`.

Subcommands:
- `graph build [--json]` — Build graph from scratch (extract symbols, build edges, compute PageRank)
- `graph update [--json]` — Incremental update (re-index files changed since last commit)
- `graph status [--json]` — Show graph statistics (symbol count, edge count, file count)
- `graph refs <symbol> [--json]` — Find all references to a symbol (reverse edge lookup, <16ms)
- `graph impact <path> [--json]` — Transitive impact analysis: what files depend on this file (BFS depth 3)
- `graph map [--budget N] [--json]` — Output cached repo map (instant, no rebuild)

Storage: `.flow/graph.bin` (bincode binary format)

### find

Smart code search that auto-routes to the best backend.

```bash
flowctl find "<query>" [--limit N] [--json]
```

Routing logic:
- Regex pattern (contains `\s`, `.*`, `[^`, etc.) → trigram index regex search
- Known symbol name (in graph) → graph refs
- Literal string (≥3 chars) → trigram index search
- Fallback → nucleo fuzzy search with frecency

### edit

Smart code edit with exact match + fuzzy fallback.

```bash
flowctl edit --file <path> --old "<text>" --new "<text>" [--json]
```

Strategy:
1. Exact `str::replacen` (first occurrence)
2. Fuzzy fallback via `fudiff` (whitespace-normalized + context matching)

Output: `{"file": "...", "method": "exact|fuzzy", "bytes_written": N}`

### index

Trigram index for fast code search. Stored at `.flow/index.bin`.

Subcommands:
- `index build [--json]` — Build trigram index from scratch
- `index update [--json]` — Incremental update (re-index changed files)
- `index search <query> [--limit N] [--json]` — Trigram-accelerated literal search
- `index regex <pattern> [--limit N] [--json]` — Regex search with trigram pre-filtering. Extracts required trigrams from regex via `regex-syntax`, filters candidates, then runs full regex on matches only.
- `index status [--json]` — Show index statistics

### completions

Generate shell completions.

```bash
flowctl completions bash > ~/.bash_completion.d/flowctl
flowctl completions zsh > ~/.zfunc/_flowctl
flowctl completions fish > ~/.config/fish/completions/flowctl.fish
```

Supported shells: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

## Ralph Receipts

RepoPrompt review receipts are written by the review skills (not flowctl commands). Codex review receipts are written by `flowctl codex impl-review` and `flowctl codex completion-review` when `--receipt` is provided. Ralph sets `REVIEW_RECEIPT_PATH` to coordinate both.

See: [Ralph deep dive](ralph.md)

## JSON Output

All commands support `--json` (except `cat`). Wrapper format:

```json
{"success": true, ...}
{"success": false, "error": "message"}
```

Exit codes: 0=success, 1=general error, 2=tool/parse error, 3=sandbox configuration error.

## Error Handling

- Missing `.flow/`: "Run 'flowctl init' first"
- Invalid ID format: "Expected format: fn-N (epic) or fn-N.M (task)"
- File conflicts: Refuses to overwrite existing epics/tasks
- Dependency violations: Same-epic only, must exist, no cycles
- Status violations: Can't start non-todo, can't close with incomplete tasks
