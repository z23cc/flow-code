# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## What Is This

Flow-Code is a Claude Code plugin that provides a **goal-driven adaptive development engine**. The entire interface is 3 MCP tools — the Rust engine handles all orchestration, the LLM focuses on actual work.

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

Start or resume a goal. The engine auto-plans multi-step goals into a DAG with parallel execution where possible. Returns an ActionSpec with: objective, guard commands, progress.

### `flow_submit(action_id, status, summary, files_changed)`

Submit work results. The engine automatically: records the attempt, runs quality guard, releases file locks, records learnings, schedules the next node. Returns the next ActionSpec (or "complete" when done).

### `flow_query(question, goal_id?)`

Ask about goal status (per-node detail), knowledge patterns, or read file content.

## The Workflow Loop

```
action = flow_drive("add OAuth login")
while action.type != "complete":
    # Do the work described in action.objective
    action = flow_submit(action.action_id, "done", "summary", ["files..."])
```

The LLM is stateless. The engine tracks everything.

## flowctl × RepoPrompt — How to Use Both

The `ccx` launcher loads both MCP servers. Built-in file tools (Read/Write/Edit/Glob/Grep) are disabled — use RepoPrompt tools instead.

### When to Use What

```
Task Complexity      Tools to Use
─────────────────    ──────────────────────────────
Simple (1 file)      RP tools only, no flowctl needed
Complex (multi-step) flowctl + RP tools
Code audit/refactor  RP agent_run (explore + engineer)
Maximum power        flowctl orchestration + RP agent execution
```

### Simple Tasks — RP Only

For single-file fixes, small changes, or quick questions:

```
get_code_structure → read_file → apply_edits → git diff
```

No need for flow_drive/flow_submit — just use RP tools directly.

### Complex Tasks — flowctl + RP

For multi-step goals with dependencies:

```
flow_drive("add auth system: register, login, JWT, password reset")
  → Engine creates 4 nodes (3 parallel + 1 dependent)
  → For each node:
      1. get_code_structure  ← understand current code
      2. file_search         ← find related files
      3. read_file           ← read specific sections
      4. apply_edits         ← make changes
      5. git diff            ← review changes
      6. flow_submit         ← engine checks quality, gives next node
  → Engine compounds knowledge on completion
```

### Code Audits — RP Multi-Agent

For scanning and bulk optimization, use RP's agent_run:

```
agent_run(explore, "scan for dead code, duplication, long functions")
  → Returns findings with file:line references

agent_run(engineer, "refactor X based on findings")
  → Makes changes directly, runs tests

Main agent verifies + commits
```

RP agents have **full MCP tool access** (unlike Claude Code sub-agents which don't inherit MCP — known bug #37785).

### Maximum Power — flowctl + RP Agents

For large refactors or feature builds:

```
flow_drive("refactor storage layer")
  → n-1: Analyze
      → agent_run(explore, "scan storage/*.rs patterns")
  → n-2: Implement
      → agent_run(engineer, "extract JsonStore trait")
  → n-3: Test
      → agent_run(engineer, "write integration tests")
  → flow_submit each result
```

flowctl manages the goal lifecycle. RP agents do the heavy lifting.

### Tool Priority (always in this order)

1. **get_code_structure** — FIRST. Always understand structure before anything else
2. **file_search** — Find related files by content/path
3. **get_file_tree** — Project structure overview
4. **read_file** — Read specific line ranges
5. **context_builder** — AI-powered deep analysis for complex tasks
6. **apply_edits** — Make changes (multi-edit, rewrite, auto-repair)
7. **git** — Review diffs, blame, log
8. **manage_selection** — Pre-select files for context
9. **agent_run** — Delegate to specialized agents (explore/engineer/pair)

### What NOT to Do

- Do NOT use `flow_query('read file ...')` when RP `read_file` is available
- Do NOT spawn Claude Code sub-agents (Agent tool) for code work — they can't access MCP tools
- Do NOT use Bash for grep/find/cat — use RP `file_search` / `read_file`
- Do NOT use flow_drive for trivial single-file changes

## Engine Internals

| Component | Purpose |
|---|---|
| **Orchestrator** | Central brain — drive/submit/query entry points |
| **GoalEngine** | Goal lifecycle, PlanningMode × SuccessModel classification |
| **Planner** | DAG plan generation with dependency detection |
| **Scheduler** | Parallel node scheduling, status transitions |
| **EscalationEngine** | Three-level escalation (retry → change strategy → replan) |
| **ContextAssembler** | Assembles file slices + patterns + constraints per action |
| **GuardRunner** | Quality gates, auto-detects project type and subdirs |
| **Learner** | Three-layer knowledge (Learning → Pattern → Methodology) |
| **CodeGraph** | Symbol-level code graph with refs/impact queries |

## Storage

```
.flow/
├── goals/{id}/          Goal-scoped storage
│   ├── goal.json        Goal definition
│   ├── plans/           Immutable plan versions
│   ├── attempts/        Per-node attempt history
│   └── events.jsonl     Append-only event log
├── knowledge/
│   ├── learnings/       Raw learnings from completed work
│   ├── patterns/        Distilled patterns (auto-compounded)
│   └── rules/           Methodology rules
├── .state/
│   └── locks.json       File locks for concurrent nodes
└── graph.bin            Code graph (symbols + deps)
```

## Code Quality

```bash
cd flowctl && cargo build --release && cargo test --all
```

115 tests. Rust only — no TypeScript, no npm, no Python.

## Key Design Decisions

- **Engine drives, LLM executes**: Rust engine owns state, scheduling, quality, knowledge. LLM focuses on coding.
- **3 MCP tools**: Token cost ~500 per goal for orchestration.
- **LLM decides tools autonomously**: Engine provides objectives, not tool prescriptions. With `ccx` (built-in tools disabled), LLM naturally uses RP tools.
- **Parallel planning**: Independent criteria auto-detected, dependent tasks (test/deploy/docs) wait for impl.
- **Structured errors (SERF)**: `{category, message, retry_safe, recovery}` for deterministic self-correction.
- **Progressive disclosure**: ActionSpec has summaries; use RP `read_file` for full content.
- **Three-level escalation**: WorkerRetry (1-2 fails) → StrategyChange (3-4) → Replan (5+).
- **Knowledge pyramid**: Learnings (raw) → Patterns (distilled, with decay) → Methodology (rules).

## Files to Never Commit

- `ref/` — reference/backup repos
- `.flow/` — per-project runtime state
- `*.upstream` — upstream backup files
