# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is This

Flow-Code is a Claude Code plugin for structured, plan-first development. The primary entry point is `/flow-code:go` (full autopilot: brainstorm → plan → work → review → close), plus individual slash commands, skills, and agents that orchestrate task tracking via a `.flow/` directory. Core engine is a Rust binary (`flowctl`) with file-based JSON storage and MCP server support.

## Core Architecture

```
commands/flow-code/*.md  → Slash command definitions (user-invocable entry points)
skills/*/SKILL.md        → Utility & assessment skills (brainstorm, interview, debug, map, etc.)
codex/skills/*/SKILL.md  → Core workflow skills (plan, work, plan-review, impl-review, epic-review)
agents/*.md              → Subagent definitions (research scouts, worker, plan-sync, etc.)
bin/flowctl               → Rust binary (built from flowctl/ workspace)
flowctl/                  → Rust Cargo workspace (2 crates: core, cli)
hooks/hooks.json         → Ralph workflow guards (active when FLOW_RALPH=1)
docs/                    → Architecture docs, CI examples
```

**Skill directories**: Both `skills/` and `codex/skills/` are scanned. Core workflow skills (plan, work, reviews) live in `codex/skills/` for Codex sync compatibility. Both directories are authoritative.

**Skills**: See `docs/skills.md` for the full classification. Core workflow: `flow-code-run` (unified phase loop via `flowctl phase next/done`).

**Key invariant**: The `bin/flowctl` Rust binary is the single source of truth for `.flow/` state. Always invoke as:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
$FLOWCTL <command>
```

## Primary Workflow

`/flow-code:go "idea"` — full autopilot from raw idea to PR. Runs brainstorm (AI self-interview) → plan → plan-review → work → impl-review → close via `flowctl phase next/done`. Zero human input. For existing epics or spec files, brainstorm is auto-skipped.

Ralph (`/flow-code:ralph-init`) is the autonomous harness that runs this loop unattended.

## Quality Gates (Three Layers)

Every epic passes through three independent, non-overlapping review gates:

| Layer | Tool | When | What it catches |
|-------|------|------|-----------------|
| **1. Guard** | `flowctl guard` | Worker Phase 6, wave checkpoint, close phase | Lint, type errors, test failures |
| **2. RP Plan-Review** | RP context_builder or Codex | Plan phase | Spec-code misalignment, missing requirements |
| **3. Codex Adversarial** | `flowctl codex adversarial` | Epic completion | Security, concurrency, edge cases (different model family) |

All three must pass (or be skipped via `flowctl config set review.backend none`). Layers are complementary — guard catches syntax, RP catches spec drift, Codex catches blind spots. **Zero-findings-halt rule**: review cycles stop immediately when no findings remain, eliminating unnecessary re-review iterations. Conversely, zero findings from an adversarial review triggers re-analysis (review may be insufficient).

## Command Flags

| Flag | Accepted by | Effect |
|------|-------------|--------|
| `--auto` | brainstorm | AI self-interview, zero human input |
| `--plan-only` | go, plan, run | Stop after plan phase |
| `--no-pr` | go, run | Skip draft PR creation at close |
| `--tdd` | work, run | Force test-first development (worker Phase 4) |
| `--interactive` | plan | Opt-in interview before planning |
| `--no-capability-scan` | plan | Skip capability-scout |
| `--research=rp\|grep` | plan | Override research backend |
| `--depth=short\|standard\|deep` | plan | Override plan depth |
| `--review=rp\|codex\|none` | plan, run | Override review backend |
| `--quick` | go, run | Fast path for trivial changes (skip brainstorm, plan review, impl review) |
| `--interactive` | go, run | Pause at key decisions for user confirmation |

## Worker Protocol (Teams Mode)

Workers communicate with the coordinator via SendMessage with summary prefixes:

| Worker → Coordinator | Coordinator Response |
|---------------------|---------------------|
| `"Task complete: fn-N.M"` | Verify status=done, unlock files, advance wave |
| `"Blocked: fn-N.M"` | Log reason, skip task in current wave |
| `"Spec conflict: fn-N.M"` | Fix spec → `"Spec updated: fn-N.M"` or skip → `"Task skipped: fn-N.M"` |
| `"Need file access: path"` | `"Access granted: path"` or `"Access denied: path"` |
| `"Need mutation: fn-N.M"` | Execute split/skip/dep change, reply with result |

Approval timeouts: file access 120s, spec conflict 120s, mutation 300s. On timeout → worker self-blocks and stops.

## Testing

```bash
# Smoke tests (flowctl core)
bash scripts/smoke_test.sh

# Full CI tests (flowctl + ralph helpers + symbol extraction)
bash scripts/ci_test.sh

# Teams e2e tests (file locking, ownership, protocol)
bash scripts/teams_e2e_test.sh

# Wave checkpoint tests (lock lifecycle, dependency unblock, stale lock recovery)
bash scripts/wave_checkpoint_test.sh

# Ralph e2e tests
bash scripts/ralph_e2e_test.sh
bash scripts/ralph_e2e_rp_test.sh    # RepoPrompt backend
bash scripts/ralph_e2e_short_rp_test.sh
```

All tests create temp directories and clean up after themselves. They must NOT be run from the plugin repo root (safety check enforced).

**Storage runtime**: All state is JSON/JSONL files in `.flow/`, readable by any tool (MCP, Read, Grep). No database, no async runtime. The `json_store` module in `flowctl-core` handles all file I/O.

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
- **File ownership**: `flowctl task create --files <paths>` declares owned files; `flowctl files <id>` shows ownership map + conflict detection
- **File locking (Teams)**: `flowctl lock --task <id> --files <paths>` acquires runtime file locks; `flowctl unlock --task <id>` releases on completion; `flowctl lock-check --file <path>` inspects lock state; `flowctl unlock --all` clears all locks between waves
- **Agent Teams mode**: `/flow-code:go` (or legacy `/flow-code:work`) spawns workers as Agent Team teammates with plain-text protocol messages (summary-prefix routing: "Task complete:", "Spec conflict:", "Blocked:", "Need file access:", "New task:", "Access granted/denied:", native `shutdown_request`) and file lock enforcement
- **Adversarial review**: `flowctl codex adversarial --base main [--focus "area"]` runs Codex in adversarial mode — tries to break the code, not validate it. Returns SHIP/NEEDS_WORK with grounded findings
- **Three-layer quality system**: Layer 1: `flowctl guard` (deterministic lint/type/test — runs at Worker Phase 6, wave checkpoint, and close phase). Layer 2: RP plan-review (code-aware spec validation, invoked via `/flow-code:go` plan-review phase — RP sees full codebase via context_builder). Layer 3: `flowctl codex adversarial` (cross-model adversarial, epic completion — different model family catches blind spots). Spec conflicts and blockers forwarded to Codex for autonomous decision-making.
- **Review circuit breaker**: Plan review max 2 iterations, impl review max 3, epic review max 2 — prevents infinite NEEDS_WORK cycles. After max iterations, pipeline proceeds with warning
- **Review backend resolution**: All review phases use the same priority chain: `--review` flag > `FLOW_REVIEW_BACKEND` env > `.flow/config.json` > default `none`. The `--no-review` flag is equivalent to `--review=none` and always wins
- **Auto-improve analysis-driven**: generates custom program.md from codebase analysis (hotspots, lint, coverage, memory) with Action Catalog ranked by impact — not static templates
- **Auto-improve quantitative**: captures before/after metrics per experiment, commit messages include delta `[lint:23→21]`
- **Worker self-review**: Phase 6 runs guard + structured diff review (correctness, quality, performance, testing) before commit
- **Plan auto-execute**: `/flow-code:go` (or legacy `/flow-code:plan`) defaults to auto-execute work after planning (Teams mode handles any task count); `--plan-only` to opt out
- **Goal-backward verification**: worker Phase 10 re-reads acceptance criteria and verifies each is actually satisfied before completing
- **Full-auto by default**: `/flow-code:go` requires zero interactive questions — AI reads git state, `.flow/` config, and request context to make branch, review, and research decisions autonomously. Default mode is Worktree + Teams + Phase-Gate (all three active). Work resumes from `.flow/` state on every startup (not a special "resume mode"). All tasks done → auto push + draft PR (`--no-pr` to skip)
- **Cross-platform**: flowctl is a single Rust binary (macOS/Linux). RP plan-review auto-degrades to Codex on platforms where rp-cli is unavailable. Bash hooks degrade gracefully on Windows (skip, don't block)
- **Session start**: CLAUDE.md instruction (not an enforced hook) — if `.flow/` exists, run `flowctl status --interrupted` to check for unfinished work from a previous session and resume with the suggested `/flow-code:work <id>` command
- **DAG cycle detection**: `flowctl dep add` validates that adding a dependency does not create a cycle in the task dependency graph. If a cycle would be created, the command fails with an error. The DAG is validated using topological sort via the `petgraph` crate
- **Concurrency-safe state**: All read-modify-write operations on shared JSON state files (pipeline, phases, locks) use advisory file locks via `fs2` to prevent lost updates under concurrent access (e.g., Ralph daemon)
- **Worker timeout**: Workers have a 30-minute default timeout per task (configurable via `worker.timeout_minutes`). On timeout: task marked failed, file locks released, wave continues
- **Stale lock recovery**: Runs at wave start AND on worker completion — detects locks held by done/failed/blocked tasks and releases them to prevent deadlocks
- **Worker phase mapping**: Workers execute 12 internal phases (via `flowctl worker-phase next/done`) within the epic "Work" phase. Epic phases and worker phases are independent systems operating at different levels
- **Project context**: Optional `.flow/project-context.md` (template in `templates/project-context.md`) provides shared technical standards (stack, rules, architecture decisions, non-goals) that all worker agents read during Phase 2 re-anchoring. Keeps agents aligned on conventions code alone can't convey
- **Code graph persistent index**: `flowctl graph build` constructs symbol-level + file-level reference graph with forward/reverse edges, persisted to `.flow/graph.bin` via bincode. `graph refs` finds all references (<16ms), `graph impact` traces transitive dependents (BFS depth 3). Incremental update via `git diff --name-only`
- **Intent-level API**: `flowctl find` auto-routes queries (regex→index regex, symbol→graph refs, literal→trigram, fallback→fuzzy). `flowctl edit` tries exact str::replacen then fuzzy fudiff fallback. Reduces 7 raw tools to 4 intent commands for agent clarity
- **N-gram bincode optimization**: Index serialized as bincode (6.2MB→502KB, 12x smaller). Candidate verification via memchr::memmem (2-5x faster). Regex→trigram extraction via regex-syntax for indexed regex search
- **ADR enforcement**: 10 ADRs in docs/decisions/ with YAML frontmatter (verify + scope). 5 verify commands registered as .flow/invariants.md, checked by `flowctl invariants check` and `flowctl guard`

## Files to Never Commit

- `ref/` — reference/backup repos
- `*.upstream` — upstream backup files
- `.tasks/` — runtime state
- `__pycache__/` — Python cache (hook scripts only)
- `.flow/` — per-project task state (runtime, not part of plugin)
