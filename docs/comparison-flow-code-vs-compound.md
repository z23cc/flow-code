# Deep Comparison: flow-code vs compound-engineering-plugin

> flow-code v0.1.42 vs compound-engineering v2.63.1 | April 2026

---

## Executive Summary

Both plugins aim to bring structured, plan-first development to AI coding assistants, but they take fundamentally different architectural paths:

- **flow-code**: Rust binary (`flowctl`) as single source of truth, formal state machine, DAG-based task orchestration, zero-interaction by default, 3-layer quality gates, Teams mode with file locking.
- **compound-engineering**: Pure Markdown/YAML, relies on Claude Code's native Task system, interactive by default, multi-platform conversion (10+ targets), knowledge compounding system, beta skills framework.

Neither is strictly "better" — they optimize for different tradeoffs.

---

## 1. Architecture

| Dimension | flow-code | compound-engineering |
|-----------|-----------|---------------------|
| **State engine** | Rust binary (`flowctl`, ~20MB) with 37 CLI commands | No binary — pure Markdown skills + YAML frontmatter |
| **Storage** | JSON/JSONL in `.flow/` (advisory file locks via `fs2`) | Markdown in `docs/plans/`, `docs/brainstorms/`, `docs/solutions/` |
| **Concurrency safety** | Advisory file locks on all read-modify-write ops | No formal locking — relies on Claude's single-thread execution |
| **DAG management** | `petgraph` crate — cycle detection, topological sort | Task dependencies via `blockedBy/blocks` metadata (no formal validation) |
| **State machine** | 8 task states (todo, in_progress, done, blocked, skipped, failed, up_for_retry, upstream_failed) | Implicit via TaskCreate/TaskUpdate (pending, in_progress, completed) |
| **Plugin structure** | `commands/` + `skills/` + `codex/skills/` + `agents/` + `bin/flowctl` | `plugins/compound-engineering/skills/` + `agents/` (no binary) |
| **Config** | `.flow/config.json` with dotted-key paths | YAML frontmatter in plan/skill files |

### Analysis

flow-code's Rust binary provides formal guarantees (cycle-free DAGs, concurrent-safe state, file locking) at the cost of build complexity and a platform-specific binary. compound-engineering trades those guarantees for simplicity — anyone can author a skill with just Markdown, no compilation needed.

**Key tradeoff**: Correctness guarantees vs authoring simplicity.

---

## 2. Workflow Orchestration

### Phase Systems

| Aspect | flow-code | compound-engineering |
|--------|-----------|---------------------|
| **Epic-level phases** | 6 phases: brainstorm → plan → plan_review → work → impl_review → close | 4-5 phases per skill (Phase 0-4/5), skill-specific |
| **Worker-level phases** | 12 internal phases (verify → re-anchor → investigate → TDD → implement → verify → commit → evidence → goal-verify → memory → complete → cleanup) | No formal worker phases — sequential task execution |
| **Phase enforcement** | `flowctl phase next/done` — mandatory sequencing | Soft enforcement via skill instructions |
| **Parallel execution** | Teams mode: multiple Agent workers per wave, file locking, wave checkpoints | Swarm mode: Team + TaskCreate primitives, no lock enforcement |
| **Default autonomy** | Zero-interaction (`/flow-code:go` never asks questions) | Interactive by default (AskUserQuestion for decisions) |

### Worker Protocol

| Feature | flow-code | compound-engineering |
|---------|-----------|---------------------|
| **Communication** | Plain-text SendMessage with prefix routing ("Task complete:", "Blocked:", "Need file access:") | Team + Task system |
| **File locking** | `flowctl lock/unlock` with stale recovery | No file locking |
| **Timeouts** | 30min default per task (configurable) | No formal timeouts |
| **Task mutation** | `flowctl task split/skip`, `dep rm` at runtime | Manual task editing |
| **Approval protocol** | `flowctl approval create/approve/reject` with wait+timeout | AskUserQuestion blocking |

### Analysis

flow-code's 2-level phase system (epic + worker) with formal enforcement provides stronger guarantees about execution order and recovery. compound-engineering's per-skill phases are more flexible but less uniform. flow-code's Teams mode with file locking enables true parallel workers without conflicts; compound's swarm mode relies on convention.

---

## 3. Quality Gates

### flow-code: Three Independent Layers

| Layer | Tool | When | What |
|-------|------|------|------|
| 1. Guard | `flowctl guard` | Worker Phase 6, wave checkpoint, close | Lint, type errors, test failures |
| 2. RP Plan-Review | RP context_builder or Codex | Plan phase | Spec-code misalignment |
| 3. Codex Adversarial | `flowctl codex adversarial` | Epic completion | Security, concurrency, edge cases (different model family) |

**Circuit breaker**: Plan review max 2 iterations, impl review max 3, epic review max 2.

### compound-engineering: Tiered Review Personas

| Aspect | Detail |
|--------|--------|
| **Reviewer count** | 20+ specialized personas (correctness, security, performance, language-specific, design, adversarial) |
| **Output format** | Structured JSON: severity (P0-P3), autofix_class (safe_auto/gated_auto/manual/advisory), confidence score, evidence |
| **Modes** | Interactive (default), autofix, report-only, headless |
| **Routing** | safe_auto → auto-applied, gated_auto → human decision, manual → downstream work, advisory → report-only |

### Comparison

| Dimension | flow-code | compound-engineering |
|-----------|-----------|---------------------|
| **Review diversity** | 3 layers (deterministic + RP + adversarial) | 20+ personas per review |
| **Cross-model** | Yes (Codex adversarial — different model family) | Single-model (Claude only) |
| **Auto-fix** | Guard fixes lint/format; review is advisory | Structured autofix_class routing |
| **Confidence calibration** | Min threshold: 0.5 (P0), 0.6 (P1-P3) | Per-finding confidence scores |
| **Dedup** | `flowctl review-backend --compare` consensus | Synthesis agent merges + dedup |
| **Iteration limits** | Circuit breaker (2-3 max) | No formal limits |

### Analysis

flow-code's 3-layer system provides cross-model diversity (guard catches syntax, RP catches spec drift, Codex catches blind spots from a different model family). compound-engineering's 20+ persona system provides broader coverage within a single model but lacks cross-model validation. flow-code's circuit breaker prevents infinite review loops; compound lacks this safety mechanism.

---

## 4. Knowledge Management

| Feature | flow-code | compound-engineering |
|---------|-----------|---------------------|
| **Memory system** | `flowctl memory write/read/list/verify` — per-epic lessons, staleness verification | `.claude/MEMORY.md` supplementary context |
| **Auto-capture** | Worker Phase 10 saves lessons; `hook auto-memory` on session stop | Manual via `/ce:compound` skill |
| **Knowledge compounding** | Memory injection at re-anchor (Phase 2) | Dedicated `/ce:compound` skill: parallel agents (Context Analyzer + Solution Extractor + Related Docs Finder), bug-track vs knowledge-track schemas |
| **Staleness** | `flowctl memory verify <id>` detects stale entries | `/ce:compound-refresh` refreshes outdated learnings |
| **Storage** | `.flow/memory/` JSON entries | `docs/solutions/<category>/` Markdown with YAML frontmatter |
| **Discoverability** | Memory auto-injected into worker context | Solutions linked from AGENTS.md/CLAUDE.md; discoverability check enforced |
| **Retro** | `/flow-code:retro` post-epic analysis | No formal retrospective skill |

### Analysis

compound-engineering's `/ce:compound` system is more sophisticated for documenting solved problems — it has parallel extraction agents, dual-track schemas (bug vs knowledge), category-based organization, and a refresh mechanism. flow-code's memory is more automated (auto-capture in worker phases, staleness verification) but less structured. flow-code has formal retrospectives; compound does not.

---

## 5. Extensibility & Multi-Platform

| Dimension | flow-code | compound-engineering |
|-----------|-----------|---------------------|
| **Skill count** | 44+ skills + 24 core workflow skills | 40+ skills |
| **Agent count** | 24 subagents (specialized scouts) | 50+ subagents (6 categories: review, research, design, docs, workflow, document-review) |
| **Command count** | 23 user-invocable commands | ~30 user-invocable skills (`/ce:*` + standalone) |
| **Multi-platform** | Codex sync via `flowctl codex sync` (`.md` → `.toml`) | 10+ platform targets: Claude, Codex, Cursor, Gemini CLI, OpenCode, Droid, GitHub Copilot, Kiro, Windsurf, OpenClaw, Qwen |
| **Conversion system** | Single-target (Codex only) | Full TypeScript CLI converter (`bunx @every-env/compound-plugin install`) with per-platform type mappings |
| **Beta framework** | No formal beta skill system | `-beta` suffix skills for safe rollouts with `disable-model-invocation: true` |
| **Skill architecture** | Skills reference shared `_shared/preamble.md`; cross-skill references allowed | Self-contained skills (no cross-skill imports); large blocks in `references/` |
| **Plugin packaging** | `.claude-plugin/` manifest | `.claude-plugin/` + `.cursor-plugin/` manifests with marketplace metadata |

### Analysis

compound-engineering's multi-platform conversion system is a significant differentiator — one source, 10+ targets. flow-code currently only syncs to Codex. compound's beta skills framework enables safe rollouts; flow-code has no equivalent. However, flow-code's Codex sync is tightly integrated with the Rust binary (type-safe conversion), while compound's TypeScript converter is more flexible but requires a Node/Bun runtime.

---

## 6. Developer Experience

| Dimension | flow-code | compound-engineering |
|-----------|-----------|---------------------|
| **Primary entry** | `/flow-code:go "idea"` — zero interaction | `/ce:brainstorm` → `/ce:plan` → `/ce:work` — interactive |
| **Full-auto mode** | Default (Ralph harness for unattended operation) | `/lfg` (autonomous pipeline), `/slfg` (swarm) |
| **CLI richness** | 37 flowctl commands with `--json` output | No CLI beyond `bunx` converter |
| **Worktree support** | Built into Teams mode; `flowctl lock/unlock` | `/git-worktree` skill with `.env` auto-copy |
| **Debugging** | `/flow-code:debug` (systematic investigation) | No dedicated debugging skill |
| **Codebase mapping** | `/flow-code:map` generates architecture docs | No equivalent |
| **Readiness assessment** | `/flow-code:prime` (8 pillars, 48 criteria) | No equivalent |
| **Auto-improve** | `/flow-code:auto-improve` (analysis-driven optimization loops) | No equivalent |
| **Design review** | `/flow-code:design-review` + `/flow-code:qa` (browser-based) | Agent-browser integration for testing |
| **Non-software planning** | Not supported | Universal planning (trips, study plans, events) |
| **Onboarding** | CLAUDE.md + docs/ | AGENTS.md + `/ce:onboarding` skill |
| **Ideation** | `/flow-code:brainstorm` | `/ce:ideate` (divergent ideation + adversarial filtering) |

### Analysis

flow-code provides richer CLI tooling (39 commands with JSON output), more autonomous operation by default, and specialized tools (debug, map, prime, auto-improve) that compound lacks. compound-engineering provides better interactive workflows, universal planning for non-software tasks, and structured ideation.

---

## 7. Unique Strengths

### flow-code Only

| Feature | Description | Files |
|---------|-------------|-------|
| **Rust state engine** | Formal state machine with 8 task states, DAG cycle detection, advisory file locks | `flowctl/crates/flowctl-core/src/state_machine.rs`, `dag.rs` |
| **3-layer quality gates** | Guard + RP + Codex adversarial (cross-model diversity) | `commands/stack.rs`, `commands/codex/` |
| **Review circuit breaker** | Prevents infinite NEEDS_WORK loops (2-3 max iterations) | `flowctl/crates/flowctl-core/src/review_protocol.rs` |
| **Architecture invariants** | Registered rules with verify commands | `commands/stack.rs` |
| **Gap management** | Track and enforce resolution of missing requirements | `commands/gap.rs` |
| **Task restart + cascade** | Reset task and all downstream dependents | `flowctl/crates/flowctl-core/src/lifecycle.rs` |
| **Evidence-based completion** | Structured evidence (commits, tests, duration, workspace_changes) | `types.rs` (Evidence struct) |
| **Auto-improve** | Analysis-driven optimization loops with before/after metrics | `skills/flow-code-auto-improve/` |
| **Ralph harness** | Full autonomous operation with loop monitoring | `commands/flow-code/ralph-init.md` |
| **Codebase mapping** | Architecture documentation generation | `skills/flow-code-map/` |
| **Prime assessment** | 8-pillar readiness scoring (48 criteria) | `skills/flow-code-prime/` |
| **Stale lock recovery** | Detects and releases locks held by done/failed/blocked tasks | Worker phase logic |
| **Event sourcing** | EpicEvent, TaskEvent, FlowEvent with replay | `flowctl-core/src/events.rs` |
| **Output filtering** | 8-stage TOML-based pipeline (compile-time embedded) | `flowctl-core/src/compress.rs` |
| **Worker re-anchoring** | Memory injection + spec re-read at task start (Phase 2) | Worker skill |

### compound-engineering Only

| Feature | Description | Files |
|---------|-------------|-------|
| **Multi-platform conversion** | One source → 10+ agent platform targets | `src/converters/`, `src/targets/` |
| **Beta skills framework** | `-beta` suffix for safe rollouts, `disable-model-invocation` | `plugins/compound-engineering/skills/ce-plan-beta/` |
| **Knowledge compounding** | `/ce:compound` with parallel extraction agents, dual-track schemas | `skills/ce-compound/SKILL.md` |
| **Structured review JSON** | Severity + autofix_class + owner + confidence per finding | `skills/ce-review/SKILL.md` |
| **Universal planning** | Non-software task planning (trips, study plans, events) | `skills/ce-plan/SKILL.md` |
| **Plan deepening** | 5+ parallel agents (scope-guardian, feasibility, design-lens, security-lens, product-lens) | `skills/ce-plan/SKILL.md` |
| **Ideation skill** | Divergent ideation + adversarial filtering | `skills/ce-ideate/SKILL.md` |
| **Autofix routing** | safe_auto → auto-applied, gated_auto → human, manual → downstream | `skills/ce-review/SKILL.md` |
| **Solution documentation** | Category-based docs with bug-track and knowledge-track schemas | `docs/solutions/` |
| **Release automation** | Linked versioning, conventional commits, multi-component releases | `src/release/components.ts` |
| **Marketplace metadata** | Claude + Cursor marketplace listings | `.claude-plugin/`, `.cursor-plugin/` |
| **Coding tutor plugin** | Educational plugin included | `plugins/coding-tutor/` |

---

## 8. Cross-Pollination Opportunities

### What flow-code Could Adopt from compound-engineering

| Opportunity | Impact | Effort | Notes |
|-------------|--------|--------|-------|
| **Multi-platform conversion** | High | High | TypeScript/Bun converter for 10+ targets — biggest gap |
| **Beta skills framework** | Medium | Low | `-beta` suffix + `disable-model-invocation` for safe rollouts |
| **Structured review JSON** | Medium | Medium | autofix_class routing (safe_auto/gated_auto/manual/advisory) |
| **Knowledge compounding** | Medium | Medium | Parallel extraction agents, dual-track schemas (bug vs knowledge) |
| **Plan deepening** | Medium | Medium | 5+ parallel agents for gap analysis on existing plans |
| **Universal planning** | Low | Low | Non-software task support (trips, events, learning) |
| **Ideation skill** | Low | Low | Divergent ideation + adversarial filtering before brainstorm |
| **Marketplace metadata** | Low | Low | Cursor marketplace listing |

### What compound-engineering Could Adopt from flow-code

| Opportunity | Impact | Effort | Notes |
|-------------|--------|--------|-------|
| **Binary state engine** | High | High | Formal state machine, concurrency safety, DAG validation |
| **File locking** | High | Medium | Prevent parallel workers from editing same files |
| **3-layer quality gates** | High | Medium | Cross-model adversarial review catches blind spots |
| **Review circuit breaker** | Medium | Low | Prevent infinite NEEDS_WORK loops |
| **Architecture invariants** | Medium | Low | Registered rules with verify commands |
| **Gap management** | Medium | Low | Track missing requirements, enforce at close |
| **Auto-improve** | Medium | Medium | Analysis-driven optimization with before/after metrics |
| **Evidence-based completion** | Medium | Low | Structured proof of task completion |
| **Worker re-anchoring** | Medium | Low | Memory injection prevents drift from plan |
| **Codebase mapping** | Low | Low | Architecture documentation generation |
| **Prime assessment** | Low | Medium | Readiness scoring across 8 pillars |

---

## Summary Matrix

| Dimension | flow-code | compound-engineering |
|-----------|-----------|---------------------|
| **Architecture** | Rust binary, formal state machine | Pure Markdown, no binary |
| **Concurrency** | Advisory file locks, DAG validation | No formal guarantees |
| **Autonomy** | Zero-interaction by default | Interactive by default |
| **Quality** | 3 layers + circuit breaker | 20+ personas + autofix routing |
| **Knowledge** | Auto-capture + staleness check | Compounding + dual-track schemas |
| **Platforms** | Claude + Codex sync | 10+ platform targets |
| **CLI** | 39 commands with --json | No CLI (converter only) |
| **Safety** | File locking + invariants + gaps | Convention-based |
| **Learning** | Retro + memory + auto-improve | Compound + refresh |
| **Rollout** | No beta framework | Beta skills + safe rollouts |

---

*Generated 2026-04-08 by flow-code pipeline (fn-1-deep-comparison-flow-code-vs-compound)*
