# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is This

Flow-Code is a Claude Code plugin for structured, plan-first development. It provides slash commands (`/flow-code:plan`, `/flow-code:work`, etc.), skills, and agents that orchestrate task tracking via a `.flow/` directory. Zero external dependencies — pure Python + Bash.

## Core Architecture

```
commands/flow-code/*.md  → Slash command definitions (user-invocable entry points)
skills/*/SKILL.md        → Skill implementations (loaded by Skill tool, never Read directly)
agents/*.md              → Subagent definitions (research scouts, worker, plan-sync, etc.)
scripts/flowctl.py       → Core engine (~9200 lines) — all .flow/ state management
scripts/flowctl          → Shell wrapper for flowctl.py
hooks/hooks.json         → Ralph workflow guards (active when FLOW_RALPH=1)
docs/                    → Architecture docs, CI examples
```

**Key invariant**: `flowctl.py` is the single source of truth for `.flow/` state. Skills and agents call it via the bundled wrapper — it is NOT installed globally. Always invoke as:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl"
$FLOWCTL <command>
```

## Primary Workflow

1. `/flow-code:plan "description"` → creates epic + tasks in `.flow/`
2. `/flow-code:plan-review` → Carmack-level review via RepoPrompt or Codex
3. `/flow-code:work <epic-id>` → executes tasks sequentially with worker subagents
4. `/flow-code:impl-review` → post-implementation review
5. `/flow-code:epic-review` → final review before closing

Ralph (`/flow-code:ralph-init`) is the autonomous harness that runs this loop unattended.

## Testing

```bash
# Smoke tests (flowctl core)
bash scripts/smoke_test.sh

# Full CI tests (flowctl + ralph helpers + symbol extraction)
bash scripts/ci_test.sh

# Ralph e2e tests
bash scripts/ralph_e2e_test.sh
bash scripts/ralph_e2e_rp_test.sh    # RepoPrompt backend
bash scripts/ralph_e2e_short_rp_test.sh
```

All tests create temp directories and clean up after themselves. They must NOT be run from the plugin repo root (safety check enforced).

## Code Quality

```bash
# Validate flowctl.py
python3 -m py_compile scripts/flowctl.py

# Validate JSON
python3 -c "import json; json.load(open('hooks/hooks.json'))"
```

No linter or formatter is configured. No TypeScript, no npm, no build step.

## Key Design Decisions

- **flowctl outputs JSON** (`--json` flag) for machine consumption by skills/agents
- **State machine**: tasks follow `todo → in_progress → done` (with `blocked` side-state)
- **Evidence-based completion**: `flowctl done` requires `--summary-file` and `--evidence-json`
- **Wave-Checkpoint-Wave**: workers execute task batches in parallel with checkpoint gates
- **Plan review gating**: `flowctl next --require-plan-review` blocks work until plan is reviewed
- **Architecture invariants**: immutable rules registered via `flowctl invariant add` with verify commands
- **Gap registry**: epics carry a `gaps` field managed via `flowctl gap`, enforced at epic close
- **Task restart**: `flowctl restart <task-id>` resets a task and cascades to all downstream dependents (`--dry-run`, `--force`)
- **Git diff snapshots**: worker agent captures baseline rev before implementation and `workspace_changes` in evidence
- **Review comparison**: `flowctl review-backend --compare <files>` or `--epic <id>` detects consensus/conflict across review receipts (auto-archived to `.flow/reviews/`)
- **Domain tagging**: `flowctl task create --domain <domain>` tags tasks (frontend/backend/architecture/testing/docs/ops/general), filterable via `tasks --domain`
- **Epic archival**: `flowctl epic archive <id>` moves closed epic + tasks + specs + reviews to `.flow/.archive/`; `flowctl epic clean` archives all closed epics at once
- **Learning loop**: plan injects memory (Step 1b), worker saves lessons (Phase 5b), epic close prompts retro, retro verifies stale entries via `flowctl memory verify <id>`
- **Task duration**: `flowctl done` auto-tracks `duration_seconds` from start to completion, rendered in evidence

## Files to Never Commit

- `ref/` — reference/backup repos
- `*.upstream` — upstream backup files
- `.claude-plugin/` — local plugin config
- `.tasks/` — runtime state
- `__pycache__/` — Python cache
- `.flow/` — per-project task state (runtime, not part of plugin)
