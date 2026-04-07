# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is This

Flow-Code is a Claude Code plugin for structured, plan-first development. It provides a unified entry point (`/flow-code:run`) plus individual slash commands, skills, and agents that orchestrate task tracking via a `.flow/` directory. Core engine is a Rust binary (`flowctl`) with file-based JSON storage and MCP server support.

## Core Architecture

```
commands/flow-code/*.md  → Slash command definitions (user-invocable entry points)
skills/*/SKILL.md        → Skill implementations (loaded by Skill tool, never Read directly)
agents/*.md              → Subagent definitions (research scouts, worker, plan-sync, etc.)
bin/flowctl               → Rust binary (built from flowctl/ workspace)
flowctl/                  → Rust Cargo workspace (4 crates: core, db, service, cli)
hooks/hooks.json         → Ralph workflow guards (active when FLOW_RALPH=1)
docs/                    → Architecture docs, CI examples
```

**Skills**: See `docs/skills.md` for the full classification. Core workflow: `flow-code-run` (unified phase loop via `flowctl phase next/done`).

**Key invariant**: The `bin/flowctl` Rust binary is the single source of truth for `.flow/` state. Always invoke as:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
$FLOWCTL <command>
```

## Primary Workflow

`/flow-code:run "description"` — drives the entire pipeline (plan → plan-review → work → impl-review → close) via `flowctl phase next/done`. One command, zero manual phase transitions.

Ralph (`/flow-code:ralph-init`) is the autonomous harness that runs this loop unattended.

## Testing

```bash
# Smoke tests (flowctl core)
bash scripts/smoke_test.sh

# Full CI tests (flowctl + ralph helpers + symbol extraction)
bash scripts/ci_test.sh

# Teams e2e tests (file locking, ownership, protocol)
bash scripts/teams_e2e_test.sh

# Ralph e2e tests
bash scripts/ralph_e2e_test.sh
bash scripts/ralph_e2e_rp_test.sh    # RepoPrompt backend
bash scripts/ralph_e2e_short_rp_test.sh
```

All tests create temp directories and clean up after themselves. They must NOT be run from the plugin repo root (safety check enforced).

**Storage runtime**: State is stored in JSON/JSONL files in the `.flow/` directory, readable by any tool. The `flowctl-db` crate provides synchronous file-based storage with no external database dependencies.

## Code Quality

```bash
# Build and test Rust flowctl
cd flowctl && cargo build --release && cargo test --all

# Validate JSON
python3 -c "import json; json.load(open('hooks/hooks.json'))"
```

Rust: clippy for linting, cargo test for tests. No TypeScript, no npm. Skills and agents are Markdown files (no build step).

## Key Design Decisions

- **flowctl outputs JSON** (`--json` flag) for machine consumption by skills/agents
- **State machine**: tasks follow `todo → in_progress → done` (with `blocked` and `skipped` side-states)
- **Evidence-based completion**: `flowctl done` requires `--summary-file` and `--evidence-json`
- **Wave-Checkpoint-Wave**: workers execute task batches in parallel with checkpoint gates
- **Plan review gating**: `flowctl next --require-plan-review` blocks work until plan is reviewed
- **Architecture invariants**: immutable rules registered via `flowctl invariant add` with verify commands
- **Gap registry**: epics carry a `gaps` field managed via `flowctl gap`, enforced at epic close
- **Task restart**: `flowctl restart <task-id>` resets a task and cascades to all downstream dependents (`--dry-run`, `--force`)
- **Runtime DAG mutation**: `flowctl task split <id> --titles "A|B|C" --chain` splits task into sub-tasks; `flowctl task skip <id> --reason` marks task as skipped (downstream deps treat as satisfied); `flowctl dep rm <task> <dep>` removes a dependency. Workers request mutations via "Need mutation:" protocol message
- **Git diff snapshots**: worker agent captures baseline rev before implementation and `workspace_changes` in evidence
- **Review comparison**: `flowctl review-backend --compare <files>` or `--epic <id>` detects consensus/conflict across review receipts (auto-archived to `.flow/reviews/`)
- **Domain tagging**: `flowctl task create --domain <domain>` tags tasks (frontend/backend/architecture/testing/docs/ops/general), filterable via `tasks --domain`
- **Epic archival**: `flowctl epic archive <id>` moves closed epic + tasks + specs + reviews to `.flow/.archive/`; `flowctl epic clean` archives all closed epics at once
- **Learning loop**: plan injects memory (Step 6), worker saves lessons (Phase 11, included in default sequence when memory.enabled is true), epic close prompts retro, retro verifies stale entries via `flowctl memory verify <id>`
- **Task duration**: `flowctl done` auto-tracks `duration_seconds` from start to completion, rendered in evidence
- **File ownership**: `flowctl task create --files <paths>` declares owned files; `flowctl files --epic <id>` shows ownership map + conflict detection
- **File locking (Teams)**: `flowctl lock --task <id> --files <paths>` acquires runtime file locks; `flowctl unlock --task <id>` releases on completion; `flowctl lock-check --file <path>` inspects lock state; `flowctl unlock --all` clears all locks between waves
- **Agent Teams mode**: `/flow-code:run` (or legacy `/flow-code:work`) spawns workers as Agent Team teammates with plain-text protocol messages (summary-prefix routing: "Task complete:", "Spec conflict:", "Blocked:", "Need file access:", "New task:", "Access granted/denied:", native `shutdown_request`) and file lock enforcement
- **Adversarial review**: `flowctl codex adversarial --base main [--focus "area"]` runs Codex in adversarial mode — tries to break the code, not validate it. Returns SHIP/NEEDS_WORK with grounded findings
- **Three-layer quality system**: Layer 1: `flowctl guard` (deterministic lint/type/test, every commit). Layer 2: RP plan-review (code-aware spec validation, invoked via `/flow-code:run` plan-review phase — RP sees full codebase via context_builder). Layer 3: `flowctl codex adversarial` (cross-model adversarial, epic completion — different model family catches blind spots). Spec conflicts and blockers forwarded to Codex for autonomous decision-making.
- **Review circuit breaker**: impl-review fix loop capped at `MAX_REVIEW_ITERATIONS` (default 3) — prevents infinite NEEDS_WORK cycles
- **Auto-improve analysis-driven**: generates custom program.md from codebase analysis (hotspots, lint, coverage, memory) with Action Catalog ranked by impact — not static templates
- **Auto-improve quantitative**: captures before/after metrics per experiment, commit messages include delta `[lint:23→21]`
- **Worker self-review**: Phase 6 runs guard + structured diff review (correctness, quality, performance, testing) before commit
- **Plan auto-execute**: `/flow-code:run` (or legacy `/flow-code:plan`) defaults to auto-execute work after planning (Teams mode handles any task count); `--plan-only` to opt out
- **Goal-backward verification**: worker Phase 10 re-reads acceptance criteria and verifies each is actually satisfied before completing
- **Full-auto by default**: `/flow-code:run` requires zero interactive questions — AI reads git state, `.flow/` config, and request context to make branch, review, and research decisions autonomously. Default mode is Worktree + Teams + Phase-Gate (all three active). Work resumes from `.flow/` state on every startup (not a special "resume mode"). All tasks done → auto push + draft PR (`--no-pr` to skip)
- **Cross-platform**: flowctl is a single Rust binary (macOS/Linux). RP plan-review auto-degrades to Codex on platforms where rp-cli is unavailable. Bash hooks degrade gracefully on Windows (skip, don't block)
- **Session start**: CLAUDE.md instruction (not an enforced hook) — if `.flow/` exists, run `flowctl status --interrupted` to check for unfinished work from a previous session and resume with the suggested `/flow-code:work <id>` command

## Files to Never Commit

- `ref/` — reference/backup repos
- `*.upstream` — upstream backup files
- `.tasks/` — runtime state
- `__pycache__/` — Python cache (hook scripts only)
- `.flow/` — per-project task state (runtime, not part of plugin)
