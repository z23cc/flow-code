# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What Is This

Flow-Code is a Claude Code plugin for structured, plan-first development. The primary execution entry point is `/flow-code:go` (full autopilot: brainstorm → plan → work → review → close), plus individual slash commands, skills, and agents that orchestrate task tracking via a `.flow/` directory. Core engine is a Rust binary (`flowctl`) with file-based JSON storage and MCP server support.

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

**Skills**: Use `skills/flow-code-guide/SKILL.md` as the discovery index and browse `skills/*/SKILL.md` + `codex/skills/*/SKILL.md` for full coverage. Core workflow: `flow-code-run` (unified phase loop via `flowctl phase next/done`).

**Key invariant**: The `bin/flowctl` Rust binary is the single source of truth for `.flow/` state. Always invoke as:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/bin/flowctl"
$FLOWCTL <command>
```

## Primary Workflow

`/flow-code:go "idea"` — the execution/autopilot front door from raw idea to PR. Runs brainstorm (AI self-interview) → plan → plan-review → work → impl-review → close via `flowctl phase next/done`. Zero human input. Also use `go` to resume existing epics (`fn-N-*`) or continue from spec files into plan/work. If execution is not wanted yet, prefer `/flow-code:plan`; if you are still shaping the problem, prefer `/flow-code:brainstorm`.

Ralph (`/flow-code:ralph-init`) is the autonomous harness that runs this loop unattended.

## High-Traffic Discovery Entry Points

Use these front doors intentionally:

- `/flow-code:go "idea"` or `/flow-code:go fn-N-*` — full autopilot / execution path, including resume.
- `/flow-code:plan "idea"` — planning-only: research + DAG/task breakdown with no implementation yet. `go --plan-only` is for staying on the `go` path while stopping after plan.
- `/flow-code:brainstorm "idea"` — open-ended exploration and pressure-test before committing to a spec or plan.
- `/flow-code:spec "idea / change / refactor"` — artifact-first requirements capture that feeds later planning/work.
- `/flow-code:adr "decision"` — durable architecture decision capture with alternatives and consequences.
- `skills/flow-code-deprecation/SKILL.md` — skill surface (no slash command) for replacement/removal guidance when the main question is how to safely retire an old surface.

## Quality Gates (Three Layers)

Every epic passes through three independent, non-overlapping review gates:

| Layer | Tool | When | What it catches |
|-------|------|------|-----------------|
| **1. Guard** | `flowctl guard` | Worker Phase 6, integration checkpoint, close phase | Lint, type errors, test failures |
| **2. RP Plan-Review** | RP context_builder or Codex | Plan phase | Spec-code misalignment, missing requirements |
| **3. Codex Adversarial** | `flowctl codex adversarial` | Epic completion | Security, concurrency, edge cases (different model family) |

All three must pass (or be skipped via `flowctl config set review.backend none`). Layers are complementary — guard catches syntax, RP catches spec drift, Codex catches blind spots. **Zero-findings-halt rule**: review cycles stop immediately when no findings remain, eliminating unnecessary re-review iterations. Conversely, zero findings from an adversarial review triggers re-analysis (review may be insufficient).

## Canonical RP/MCP Orchestration Guidance

For RepoPrompt / MCP workflow behavior, use `skills/_shared/rp-mcp-orchestration.md` as the canonical guidance layer.

That shared guide is the source of truth for:
- when to use `context_builder` versus direct repo tools;
- how `ask_oracle` / `oracle_send` relates to the current selection;
- when `manage_selection` is appropriate;
- how `prompt` / `workspace_context` exports fit;
- when `agent_run` should be used for delegated parallel work.

If a skill or protocol doc restates RP/MCP behavior, treat this section plus the shared guide as authoritative unless the other doc is intentionally describing a narrower workflow-specific rule.

## Command Flags

| Flag | Accepted by | Effect |
|------|-------------|--------|
| `--auto` | brainstorm | AI self-interview, zero human input |
| `--plan-only` | go, run | Stop the execution pipeline after plan |
| `--no-pr` | go, run | Skip draft PR creation at close |
| `--tdd` | work, run | Force test-first development (worker Phase 4) |
| `--interactive` | plan | Opt-in interview before planning |
| `--no-capability-scan` | plan | Skip capability-scout |
| `--research=rp\|grep` | plan | Override research backend |
| `--depth=short\|standard\|deep` | plan | Override plan depth |
| `--review=rp\|codex\|none` | go, plan, run, epic-review | Override review backend |
| `--review=rp\|codex\|export\|none` | work, plan-review, impl-review | Override or export review backend |
| `--skip-gap-check` | epic-review | Bypass the capability-gap pre-check with a warning |
| `--quick` | go, run | Fast path for trivial changes (skip brainstorm, plan review, impl review) |
| `--interactive` | go, run | Pause at key decisions for user confirmation |

## Worker Protocol (RP Session Mode)

Workers are spawned as RP agents via `agent_run` in isolated git worktrees registered as RP workspaces. The coordinator uses RP session operations for all communication:

| RP Operation | Purpose |
|-------------|---------|
| `agent_run(start, detach:true)` | Spawn worker in worktree |
| `agent_run(wait, session_ids)` | Batch wait for completion |
| `agent_run(poll, session_id)` | Check individual worker status |
| `agent_run(steer, session_id)` | Inject instructions mid-execution |
| `agent_run(cancel, session_id)` | Terminate timed-out worker |
| `agent_manage(cleanup_sessions)` | Clean up after the integration checkpoint / epic |

Workers output structured status (STATUS/TASK_ID/SUMMARY/FILES_CHANGED/TESTS) that the coordinator parses from session output. `steer` replaces SendMessage for spec conflict resolution, file access grants, and cross-worker coordination.

## Testing

```bash
# Smoke tests (flowctl core)
bash scripts/smoke_test.sh

# Full CI tests (flowctl + ralph helpers + symbol extraction)
bash scripts/ci_test.sh

# Teams e2e tests (file locking, ownership, protocol)
bash scripts/teams_e2e_test.sh

# Integration checkpoint tests (legacy script name; lock lifecycle, dependency unblock, stale lock recovery)
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
- **Continuous scheduling + integration checkpoint**: workers execute ready tasks in parallel, merge as they finish, and pass a final integration checkpoint before leaving the Work phase
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
- **File locking**: `flowctl lock --task <id> --files <paths>` acquires runtime file locks; `flowctl unlock --task <id>` releases on completion; `flowctl lock-check --file <path>` inspects lock state; `flowctl unlock --all` clears all locks between waves
- **RP agent_run mode**: `/flow-code:go` spawns workers as RP agents via `agent_run` in isolated git worktrees registered as RP workspaces. Coordinator uses `wait`/`poll` for monitoring, `steer` for mid-execution coordination, and `cancel` for timeouts. Workers output structured status fields (STATUS/TASK_ID/SUMMARY/FILES_CHANGED). File lock enforcement via flowctl remains unchanged
- **Adversarial review**: `flowctl codex adversarial --base main [--focus "area"]` runs Codex in adversarial mode — tries to break the code, not validate it. Returns SHIP/NEEDS_WORK with grounded findings
- **Three-layer quality system**: Layer 1: `flowctl guard` (deterministic lint/type/test — runs at Worker Phase 6, the work integration checkpoint, and close phase). Layer 2: RP plan-review (code-aware spec validation, invoked via `/flow-code:go` plan-review phase — RP sees full codebase via context_builder). Layer 3: `flowctl codex adversarial` (cross-model adversarial, epic completion — different model family catches blind spots). Spec conflicts and blockers forwarded to Codex for autonomous decision-making.
- **Review circuit breaker**: Plan review max 2 iterations, impl review max 3, epic review max 2 — prevents infinite NEEDS_WORK cycles. After max iterations, pipeline proceeds with warning
- **Review backend resolution**: All review phases use the same priority chain: `--review` flag > `FLOW_REVIEW_BACKEND` env > `.flow/config.json` > default `none`. The `--no-review` flag is equivalent to `--review=none` and always wins
- **Auto-improve analysis-driven**: generates custom program.md from codebase analysis (hotspots, lint, coverage, memory) with Action Catalog ranked by impact — not static templates
- **Auto-improve quantitative**: captures before/after metrics per experiment, commit messages include delta `[lint:23→21]`
- **Worker self-review**: Phase 6 runs guard + structured diff review (correctness, quality, performance, testing) before commit
- **Execution vs planning boundary**: `/flow-code:go` is the auto-executing path and resume surface. Use `/flow-code:plan` when you explicitly want planning-only; use `--plan-only` when you need the `go` pipeline to stop after planning.
- **Goal-backward verification**: worker Phase 10 re-reads acceptance criteria and verifies each is actually satisfied before completing
- **Full-auto by default**: `/flow-code:go` requires zero interactive questions — AI reads git state, `.flow/` config, and request context to make branch, review, and research decisions autonomously. Default mode is Worktree + RP agent_run + Phase-Gate (all three active). Work resumes from `.flow/` state on every startup (not a special "resume mode"). All tasks done → auto push + draft PR (`--no-pr` to skip)
- **Cross-platform**: flowctl is a single Rust binary (macOS/Linux). RP plan-review auto-degrades to Codex on platforms where rp-cli is unavailable. Bash hooks degrade gracefully on Windows (skip, don't block)
- **Session start**: CLAUDE.md instruction (not an enforced hook) — if `.flow/` exists, run `flowctl status --interrupted` to check for unfinished work from a previous session and resume with the suggested `/flow-code:work <id>` command
- **DAG cycle detection**: `flowctl dep add` validates that adding a dependency does not create a cycle in the task dependency graph. If a cycle would be created, the command fails with an error. The DAG is validated using topological sort via the `petgraph` crate
- **Concurrency-safe state**: All read-modify-write operations on shared JSON state files (pipeline, phases, locks) use advisory file locks via `fs2` to prevent lost updates under concurrent access (e.g., Ralph daemon)
- **Worker timeout**: Workers have a 30-minute default timeout per task (configurable via `worker.timeout_minutes`). On timeout: task marked failed, file locks released, scheduler continues
- **Stale lock recovery**: Runs at scheduler start and on worker completion — detects locks held by done/failed/blocked tasks and releases them to prevent deadlocks
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
