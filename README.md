<div align="center">

**[English](README.md)** | **[中文](README_CN.md)**

# Flow-Code

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](../../LICENSE)
[![Claude Code](https://img.shields.io/badge/Claude_Code-Plugin-blueviolet)](https://claude.ai/code)
[![Version](https://img.shields.io/badge/Version-0.1.48-green)](https://github.com/z23cc/flow-code/releases)

**A production-grade harness for Claude Code. Full-auto development from idea to PR.**

</div>

---

## What Is This?

One command goes from idea to draft PR — planning, parallel implementation, three-layer quality gates, and cross-model adversarial review, all fully automated.

```
/flow-code:go "Add OAuth login"
  → AI self-interview (auto-brainstorm)
  → Research scouts (repo, context, practice)
  → Dependency-ordered task DAG
  → Teams parallel workers (file locking)
  → Three-layer review (guard + RP + Codex adversarial)
  → Auto push + draft PR
```

Everything lives in `.flow/` state. No external services. Single Rust binary (`flowctl`, 70+ commands). Uninstall: delete `.flow/`.

## Quick Start

**Prerequisites**: `git`, `jq`, `gh` (GitHub CLI). Optional: `rp-cli` (Layer 2 review), `codex` (Layer 3 adversarial).

```bash
# Install
/plugin marketplace add https://github.com/z23cc/flow-code
/plugin install flow-code

# Setup (recommended — configures review backend, copies flowctl)
/flow-code:setup

# Go — full autopilot from idea to PR
/flow-code:go "add OAuth support"

# Quick — skip planning for trivial changes
/flow-code:go "fix typo in README" --quick

# Plan only — research + task breakdown, no execution yet
/flow-code:plan "add OAuth support"

# Resume — reads .flow state and continues
/flow-code:go fn-1

# Requirements-first handoff into planning/work
/flow-code:spec "introduce OAuth for admin login"

# Record architecture decision and alternatives
/flow-code:adr "choose token/session strategy"
```

## Pick the Right Front Door

| If you want to... | Use this |
|---|---|
| Execute the full path or resume existing work | `/flow-code:go "idea"` or `/flow-code:go fn-1` — full autopilot / execution path, including resume |
| Stop after planning | `/flow-code:plan "idea"` — planning-only; use `go --plan-only` only when you are already on the `go` path |
| Explore and pressure-test before committing | `/flow-code:brainstorm "idea"` — open-ended exploration first |
| Write a reusable requirements artifact first | `/flow-code:spec "idea / change / refactor"` — artifact-first requirements capture for later planning/work |
| Record a lasting architecture choice | `/flow-code:adr "decision"` — durable decision record with alternatives and consequences |
| Replace or remove an old surface safely | [`flow-code-deprecation`](skills/flow-code-deprecation/SKILL.md) — skill surface (no slash command) for replacement/removal guidance |

## Core Workflow

```
brainstorm → plan → plan_review → work → impl_review → close
```

| Phase | What happens |
|-------|-------------|
| **Brainstorm** | AI self-interview, structured deepening (Pre-mortem/First Principles/Inversion) |
| **Plan** | Parallel scouts research codebase, create task DAG with dependencies |
| **Plan Review** | RP context_builder or Codex validates spec-code alignment |
| **Work** | Teams mode: continuous parallel worker scheduling, file locking, and a final integration checkpoint |
| **Impl Review** | 3-layer parallel review: Blind Hunter + Edge Case Hunter + Acceptance Auditor |
| **Close** | Validate, guard, pre-launch checklist, push + draft PR |

Every task belongs to an epic (`fn-N`). Tasks are `fn-N.M`. Even one-off requests get an epic container for context and re-anchoring.

## Three-Layer Quality

| Layer | Tool | When | Catches |
|-------|------|------|---------|
| **1. Guard** | `flowctl guard` | Every commit | Lint, types, test failures |
| **2. RP Plan-Review** | RepoPrompt context_builder | Plan phase | Spec-code misalignment |
| **3. Codex Adversarial** | `flowctl codex adversarial` | Epic completion | Security, concurrency, edge cases |

Zero-findings rule: reviewers must find issues. Zero findings → halt and re-analyze. Circuit breaker: max 2-3 iterations.

## Key Commands

| Command | Purpose |
|---------|---------|
| `/flow-code:go "idea"` | Full autopilot / execution path: brainstorm → plan → work → review → PR |
| `/flow-code:go "fix" --quick` | Fast path for trivial changes |
| `/flow-code:go fn-1` | Resume an existing epic from its current phase |
| `/flow-code:plan "feature"` | Planning-only: research + task breakdown, no execution yet |
| `/flow-code:plan-review fn-1` | Run the formal plan review gate before work starts |
| `/flow-code:work fn-1` | Execute tasks for an epic |
| `/flow-code:impl-review fn-1.2 --base <commit>` | Review implementation changes for a task or branch scope |
| `/flow-code:epic-review fn-1` | Verify the completed epic against its spec before close |
| `/flow-code:brainstorm --auto "idea"` | Open-ended exploration and pressure-testing before plan/spec |
| `/flow-code:spec "idea / change / refactor"` | Produce an artifact-first, planning-ready requirements spec |
| `/flow-code:adr "decision"` | Capture a durable architecture decision + alternatives |
| `/flow-code:prime` | Assess codebase readiness (8 pillars, 48 criteria) |
| `/flow-code:map` | Generate architecture documentation |
| `/flow-code:auto-improve "goal"` | Autonomous code optimization loops |
| `/flow-code:ralph-init` | Scaffold autonomous unattended harness |
| `flowctl find "<query>"` | Smart search: auto-routes regex/symbol/literal/fuzzy |
| `flowctl graph refs <symbol>` | Who references this symbol? |
| `flowctl graph impact <path>` | What files break if I change this? |
| `flowctl edit --file <f> --old --new` | Smart edit: exact match + fuzzy fallback |

Command index: [commands/flow-code/](commands/flow-code/) | All flags: [CLAUDE.md](CLAUDE.md)

## flowctl CLI

Single Rust binary, 70+ top-level commands. All output `--json` for machine consumption.

```bash
flowctl init                          # Initialize .flow/
flowctl epic create --title "..."     # Create epic
flowctl task create --epic fn-1 ...   # Create task with deps
flowctl ready fn-1                    # List ready tasks
flowctl start fn-1.1                  # Start task
flowctl done fn-1.1 --summary "..."   # Complete with evidence
flowctl guard                         # Run lint/type/test
flowctl checklist verify --task fn-1.1 # Verify DoD checklist
flowctl dag fn-1                      # ASCII dependency graph
flowctl codex adversarial --base main # Cross-model review
flowctl write-file --path f --stdin   # Pipeline file I/O
```

CLI reference: [flowctl/README.md](flowctl/README.md) (plus `flowctl --help`)

## Architecture

```
commands/flow-code/*.md    → 20+ slash commands (user entry points, including spec/adr)
skills/*/SKILL.md          → 50+ skills (workflow + domain)
  └─ steps/*.md            → Step-file architecture (JIT loading)
agents/*.md                → 20+ subagents (scouts, workers, reviewers)
flowctl/                   → Rust Cargo workspace (core + cli)
  └─ bin/flowctl           → Single binary, 70+ commands
prompts/                   → Review templates (blind-hunter, edge-case, acceptance-auditor)
templates/                 → project-context.md template
.flow/                     → Runtime state (JSON/JSONL, per-project)
```

## Key Features

**Full-Auto** — `/flow-code:go` requires zero questions. AI reads git state and `.flow/` config to decide branch, review backend, research depth.

**Teams Mode** — Ready tasks spawn as parallel Agent workers with file locking, stale lock recovery, continuous scheduling, and a final integration checkpoint.

**Step-File Architecture** — Skills split into step files (`steps/step-01-init.md`, etc.) loaded JIT. Saves ~60% tokens per invocation.

**Project Context** — `.flow/project-context.md` provides shared technical standards all workers read during re-anchoring.

**Definition of Done** — `flowctl checklist` with 8 default items across 4 categories (context, implementation, testing, documentation).

**Ralph** — Autonomous harness for unattended operation. Runs the full pipeline in a loop with hooks for guard enforcement.

**Re-anchoring** — Every worker reads task spec + project context + memory before implementation. Survives context compaction.

**DAG Mutation** — `flowctl task split/skip`, `dep rm` at runtime. Workers request mutations via protocol messages.

## Detailed Documentation

| Document | Contents |
|----------|----------|
| [CLAUDE.md](CLAUDE.md) | Architecture, design decisions, command flags, testing |
| [commands/flow-code/](commands/flow-code/) | Slash command index (including `spec` and `adr`) |
| [skills/flow-code-guide/SKILL.md](skills/flow-code-guide/SKILL.md) | Skill/command discovery flowchart |
| [skills/flow-code-documentation/SKILL.md](skills/flow-code-documentation/SKILL.md) | Documentation patterns (spec/ADR/README/changelog) |
| [skills/flow-code-deprecation/SKILL.md](skills/flow-code-deprecation/SKILL.md) | Deprecation, replacement, and removal guidance |
| [CHANGELOG.md](CHANGELOG.md) | Version history |

## License

MIT
</div>
