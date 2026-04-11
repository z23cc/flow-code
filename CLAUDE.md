# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## What Is This

Flow-Code is a Claude Code / Codex plugin that provides a **goal-driven adaptive development engine**. The entire interface is 3 MCP tools — the Rust engine handles all orchestration, the LLM focuses on actual work.

## Architecture

```
bin/flowctl              → Rust binary (3 commands: serve, init, guard)
flowctl/                 → Rust Cargo workspace (3 crates: core, mcp, cli)
  crates/flowctl-core/   → Domain, Engine, Storage, Knowledge, Quality
  crates/flowctl-mcp/    → MCP server (3 tools)
  crates/flowctl-cli/    → Minimal CLI entry point
.mcp.json                → MCP server registration
docs/                    → Architecture docs
```

## How It Works

The engine exposes **3 MCP tools**. Claude Code calls them via the MCP server registered in `.mcp.json`:

### `flow_drive(request)`

Start or resume a goal. The engine:
1. Creates/finds the goal
2. Generates a plan if needed
3. Assembles a self-contained **ActionSpec** with: objective, relevant file slices, knowledge patterns, constraints, and quality expectations

```
result = flow_drive("add OAuth login with Google")
→ ActionSpec { type: "implement", objective: "...", context: { files, patterns, constraints }, ... }
```

### `flow_submit(action_id, status, summary, files_changed)`

Submit work results. The engine automatically:
- Records the attempt
- Runs quality guard at risk-proportional depth
- Releases file locks
- Records learnings
- Schedules the next node
- Returns the next ActionSpec (or "complete" when done)

### `flow_query(question, goal_id?)`

Ask about goal status, knowledge base, or codebase. Natural language queries.

## The Workflow Loop

```
action = flow_drive("add OAuth login")
while action.type != "complete":
    # Do the work described in action.objective using action.context
    action = flow_submit(action.action_id, "done", "summary", ["files..."])
```

The LLM is stateless. The engine tracks everything.

## Engine Internals

| Component | Purpose |
|---|---|
| **Orchestrator** | Central brain — drive/submit/query entry points |
| **GoalEngine** | Goal lifecycle, PlanningMode x SuccessModel classification |
| **Planner** | Generates PlanVersion with RiskProfile per node |
| **Scheduler** | DAG scheduling, ready-node detection, status transitions |
| **EscalationEngine** | Three-level escalation (retry → change strategy → replan) |
| **ContextAssembler** | Assembles file slices + patterns + constraints per action |
| **GuardRunner** | Internalized quality gates, auto-detects project type |
| **Learner** | Three-layer knowledge (Learning → Pattern → Methodology) |
| **PolicyEngine** | MCP + Hook policy enforcement |
| **CodeGraph** | Symbol-level code graph with refs/impact queries |
| **NgramIndex** | Trigram search index with regex optimization |

## Storage

All state lives in `.flow/` as JSON files:

```
.flow/
├── goals/{id}/          Goal-scoped storage
│   ├── goal.json        Goal definition
│   ├── plans/           Immutable plan versions (0001.json, 0002.json...)
│   ├── attempts/        Per-node attempt history
│   └── events.jsonl     Append-only event log
├── knowledge/
│   ├── learnings/       Raw learnings from completed work
│   ├── patterns/        Distilled patterns (auto-compounded)
│   └── rules/           Methodology rules
├── .state/
│   └── locks.json       File locks for concurrent nodes
├── graph.bin            Code graph (symbols + deps)
├── index/ngram.bin      Trigram search index
└── config.json          Project configuration
```

## CLI Commands

```bash
flowctl serve    # Start MCP server on stdio
flowctl init     # Initialize .flow/ directory
flowctl guard    # Run quality checks (auto-detects project type)
```

## Code Quality

```bash
cd flowctl && cargo build --release && cargo test --all
```

130 tests. Rust only — no TypeScript, no npm, no Python.

## flowctl × RepoPrompt Collaboration Protocol

When both MCP servers are available, use them together for maximum effectiveness:

```
┌─ flowctl (编排层) ─────────────┐   ┌─ RepoPrompt (代码理解层) ────────┐
│ flow_drive  → 规划/调度/升级    │   │ get_code_structure → 函数签名     │
│ flow_submit → 质量门禁/知识沉淀 │   │ file_search → 搜索文件/内容       │
│ flow_query  → 状态/知识/文件    │   │ context_builder → AI 深度分析     │
│                                │   │ apply_edits → 精确代码编辑        │
│                                │   │ git → diff/blame/log             │
│                                │   │ read_file → 按行读取              │
└────────────────────────────────┘   └──────────────────────────────────┘
```

### Recommended Workflow (per ActionSpec node)

1. **Understand** (RP): Follow `recommended_workflow` in ActionSpec
   - `file_search` — find related files
   - `get_code_structure` — understand APIs without reading full files
   - `context_builder` — for complex tasks, AI-powered deep discovery
2. **Implement** (RP):
   - `apply_edits` — multi-edit transactions, auto-repair whitespace
   - `read_file` — read specific line ranges when needed
3. **Verify** (flowctl + RP):
   - `git diff` — review changes before submitting
   - `flow_submit` — engine runs guard, records learnings, advances to next node

### What NOT to duplicate

- Do NOT use flowctl's `flow_query('read file ...')` when RP `read_file` is available — RP supports line ranges
- Do NOT manually search for files — use RP `file_search` instead of grep/glob
- Do NOT read full files for context — use RP `get_code_structure` for signatures

## Key Design Decisions

- **Engine drives, LLM executes**: The Rust engine owns the state machine, scheduling, quality gates, and knowledge. The LLM focuses on coding and reviewing.
- **3 MCP tools replace 97 CLI commands + 56 skills + 25 agents**: Token cost drops from ~41K to ~500 per goal for orchestration.
- **ActionSpec protocol**: Self-contained work packages with everything the LLM needs — no file searching required.
- **Goal-scoped storage**: Each goal has its own directory with plans, attempts, and events.
- **Immutable plan versions**: Replan creates a new version, never mutates existing ones.
- **Risk-proportional guard**: GuardDepth (Trivial/Standard/Thorough) based on node RiskProfile.
- **Three-level escalation**: WorkerRetry (1-2 fails) → StrategyChange (3-4) → Replan (5+).
- **Knowledge pyramid**: Learnings (raw) → Patterns (distilled, with decay) → Methodology (rules).
- **Concurrency-safe state**: File locks via `fs2` for concurrent access.
- **Cross-platform**: Same 3-tool protocol works with Claude Code (MCP) and Codex (AGENTS.md).

## Files to Never Commit

- `ref/` — reference/backup repos
- `.flow/` — per-project runtime state
- `*.upstream` — upstream backup files
