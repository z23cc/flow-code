# flowctl Rust Platform Design Spec

> **Date**: 2026-04-04
> **Status**: Draft
> **Scope**: Rewrite flowctl from Python to Rust as a platform-grade development orchestration engine

## 1. Overview

### Problem

The current Python-based flowctl CLI has functional limitations that prevent it from becoming a platform-grade tool:

- **Cold start overhead**: Every invocation spawns a Python interpreter (~100ms)
- **Concurrency safety**: File locking via fcntl is fragile, JSON file scatter creates race conditions
- **No daemon mode**: Cannot auto-schedule, watch files, or push events
- **No real-time visibility**: No TUI dashboard, no metrics, no analytics
- **Limited queryability**: JSON files cannot be aggregated, filtered, or trended
- **Platform lock**: fcntl is Unix-only, no native Windows support

### Solution

Rewrite flowctl as a Rust binary with:

1. **SQLite-backed state** (derived from Markdown source of truth)
2. **DAG scheduler** with bounded parallelism and failure propagation
3. **Daemon mode** with event bus, file watching, and auto-scheduling
4. **TUI dashboard** with real-time task/DAG/log/metrics views
5. **HTTP API** over Unix socket for external integration
6. **Comprehensive observability** with event sourcing, metrics, and analytics

### Non-Goals

- Multi-AI-agent support (remains Claude Code focused)
- Remote/distributed execution
- Web UI (HTTP API enables future development, but not in scope)
- Rewriting skills/agents Markdown files (they stay as-is)

---

## 2. Data Architecture

### Core Principle: Markdown is Canonical, SQLite is Cache

```
.flow/
├── epics/                     ← Git-tracked, CANONICAL source of truth
│   └── fn-1-add-auth.md       ← YAML frontmatter + Markdown body
├── tasks/                     ← Git-tracked, CANONICAL source of truth
│   ├── fn-1-add-auth.1.md
│   └── fn-1-add-auth.2.md
├── reviews/                   ← Git-tracked
│   └── impl-fn-1.1-rp.json
├── .state/                    ← .gitignore, runtime only
│   ├── flowctl.db             ← SQLite WAL (index + runtime + metrics)
│   └── flowctl.sock           ← Daemon Unix socket
└── .archive/                  ← Git-tracked, closed epics
```

**Key invariant**: `flowctl reindex` can fully rebuild the SQLite database from Markdown files. If `.flow/.state/flowctl.db` is deleted, all indexed data is recoverable. Runtime-only data (locks, heartbeats, events, metrics) is not recoverable — this is by design.

**Rationale** (validated by Fossil SCM, MarkdownDB, DVC):
- Markdown files are human/LLM-readable, git-diffable, git-mergeable
- SQLite enables fast queries, aggregations, concurrent access
- Never put SQLite in git (binary, can't diff/merge, bloats repo)

### Markdown Frontmatter Format

**Epic** (`epics/fn-1-add-auth.md`):
```markdown
---
schema_version: 1
id: fn-1-add-auth
title: Add Authentication
status: open
branch: feat/add-auth
plan_review: unknown
depends_on_epics: []
created_at: 2025-01-01T00:00:00Z
---

## Overview
User authentication via OAuth2...
```

**Task** (`tasks/fn-1-add-auth.1.md`):
```markdown
---
schema_version: 1
id: fn-1-add-auth.1
epic: fn-1-add-auth
title: Design Auth Flow
status: todo
priority: 1
domain: backend
depends_on: []
files: [src/auth.ts, src/routes.ts]
---

## Description
Design the authentication flow...

## Acceptance Criteria
- [ ] Flow diagram created
- [ ] API endpoints defined
```

### SQLite Schema

```sql
-- ═══ Indexed from Markdown frontmatter (rebuildable via reindex) ═══

CREATE TABLE epics (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open',
    branch_name TEXT,
    plan_review TEXT DEFAULT 'unknown',
    file_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    epic_id     TEXT NOT NULL REFERENCES epics(id),
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'todo',
    priority    INTEGER DEFAULT 999,
    domain      TEXT DEFAULT 'general',
    file_path   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE task_deps (
    task_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (task_id, depends_on)
);

CREATE TABLE epic_deps (
    epic_id     TEXT NOT NULL,
    depends_on  TEXT NOT NULL,
    PRIMARY KEY (epic_id, depends_on)
);

CREATE TABLE file_ownership (
    file_path   TEXT NOT NULL,
    task_id     TEXT NOT NULL,
    PRIMARY KEY (file_path, task_id)
);

-- ═══ Runtime-only data (not in Markdown, not rebuildable) ═══

CREATE TABLE runtime_state (
    task_id       TEXT PRIMARY KEY,
    assignee      TEXT,
    claimed_at    TEXT,
    completed_at  TEXT,
    duration_secs INTEGER,
    blocked_reason TEXT,
    baseline_rev  TEXT,
    final_rev     TEXT
);

CREATE TABLE file_locks (
    file_path   TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL,
    locked_at   TEXT NOT NULL
);

CREATE TABLE heartbeats (
    task_id     TEXT PRIMARY KEY,
    last_beat   TEXT NOT NULL,
    worker_pid  INTEGER
);

CREATE TABLE phase_progress (
    task_id     TEXT NOT NULL,
    phase       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',
    completed_at TEXT,
    PRIMARY KEY (task_id, phase)
);

CREATE TABLE evidence (
    task_id       TEXT PRIMARY KEY,
    commits       TEXT,
    tests         TEXT,
    files_changed INTEGER,
    insertions    INTEGER,
    deletions     INTEGER,
    review_iters  INTEGER
);

-- ═══ Event log + metrics (append-only, runtime-only) ═══

CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    epic_id     TEXT NOT NULL,
    task_id     TEXT,
    event_type  TEXT NOT NULL,
    actor       TEXT,
    payload     TEXT,
    session_id  TEXT
);

CREATE TABLE token_usage (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    epic_id         TEXT NOT NULL,
    task_id         TEXT,
    phase           TEXT,
    model           TEXT,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    cache_read      INTEGER DEFAULT 0,
    cache_write     INTEGER DEFAULT 0,
    estimated_cost  REAL
);

CREATE TABLE daily_rollup (
    day              TEXT NOT NULL,
    epic_id          TEXT,
    tasks_started    INTEGER DEFAULT 0,
    tasks_completed  INTEGER DEFAULT 0,
    tasks_failed     INTEGER DEFAULT 0,
    total_duration_s INTEGER DEFAULT 0,
    input_tokens     INTEGER DEFAULT 0,
    output_tokens    INTEGER DEFAULT 0,
    PRIMARY KEY (day, epic_id)
);

CREATE TABLE monthly_rollup (
    month            TEXT PRIMARY KEY,
    epics_completed  INTEGER DEFAULT 0,
    tasks_completed  INTEGER DEFAULT 0,
    avg_lead_time_h  REAL DEFAULT 0,
    total_tokens     INTEGER DEFAULT 0,
    total_cost_usd   REAL DEFAULT 0
);

-- ═══ Indexes ═══

CREATE INDEX idx_tasks_epic ON tasks(epic_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_events_entity ON events(epic_id, task_id);
CREATE INDEX idx_events_ts ON events(timestamp);
CREATE INDEX idx_events_type ON events(event_type, timestamp);
CREATE INDEX idx_token_epic ON token_usage(epic_id);

-- ═══ Auto-aggregation triggers ═══

CREATE TRIGGER trg_daily_rollup AFTER INSERT ON events
WHEN NEW.event_type IN ('task_completed', 'task_failed', 'task_started')
BEGIN
    INSERT INTO daily_rollup (day, epic_id, tasks_completed, tasks_failed, tasks_started)
    VALUES (DATE(NEW.timestamp), NEW.epic_id,
            CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
            CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
            CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END)
    ON CONFLICT(day, epic_id) DO UPDATE SET
        tasks_completed = tasks_completed +
            CASE WHEN NEW.event_type = 'task_completed' THEN 1 ELSE 0 END,
        tasks_failed = tasks_failed +
            CASE WHEN NEW.event_type = 'task_failed' THEN 1 ELSE 0 END,
        tasks_started = tasks_started +
            CASE WHEN NEW.event_type = 'task_started' THEN 1 ELSE 0 END;
END;
```

### SQLite Configuration

```sql
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA wal_autocheckpoint = 1000;
```

### Write Flow

Write order: **SQLite first, then Markdown**. If Markdown write fails, SQLite can be rebuilt via `reindex`. The reverse is not recoverable.

1. `flowctl done fn-1.1` → BEGIN SQLite transaction
2. Update `tasks` index table (`status = 'done'`)
3. Write `runtime_state` table (duration, evidence — runtime-only data)
4. Write `events` table (audit log)
5. COMMIT transaction
6. Update `tasks/fn-1-add-auth.1.md` frontmatter `status: done`
7. Trigger fires to update `daily_rollup`

If step 6 fails (filesystem error), SQLite is ahead of Markdown. Running `flowctl reindex` will reconcile by re-reading Markdown (which still shows old status). This is the expected recovery path — Markdown is canonical.

### Rebuild

```bash
flowctl reindex   # Delete SQLite index tables, scan all .md frontmatter, rebuild
                  # runtime_state/events/metrics are NOT rebuilt (expected)
```

---

## 3. Task State Machine

### Extended States (validated by Airflow)

```
                                    ┌──────────────┐
                                    │ up_for_retry │
                                    └──┬───────────┘
                                       │ retry
     ┌──────┐    ┌─────────────┐    ┌──▼───────┐    ┌──────┐
     │ todo │───>│ in_progress │───>│  failed  │    │ done │
     └──────┘    └──────┬──────┘    └──────────┘    └──────┘
                        │                              ▲
                        ├──────────────────────────────┘
                        │
                        │           ┌─────────────────┐
                        │           │ upstream_failed  │
                        │           └─────────────────┘
                        │
                 ┌──────▼──────┐    ┌─────────┐
                 │   blocked   │    │ skipped │
                 └─────────────┘    └─────────┘
```

**New states vs current Python version:**
- `upstream_failed` — dependency failed, auto-marked, not executed
- `up_for_retry` — retriable failure, distinguishes from terminal `failed`
- `failed` — terminal failure state (current Python uses `blocked` ambiguously)

### Valid Transitions

| From | To | Trigger |
|------|----|---------|
| `todo` | `in_progress` | `flowctl start` |
| `todo` | `skipped` | `flowctl task skip` |
| `in_progress` | `done` | `flowctl done` |
| `in_progress` | `failed` | guard failure, timeout |
| `in_progress` | `blocked` | `flowctl block` (external dependency) |
| `failed` | `up_for_retry` | auto (if retries remaining) |
| `up_for_retry` | `in_progress` | scheduler auto-retry |
| `blocked` | `todo` | `flowctl restart` |
| `failed` | `todo` | `flowctl restart` |
| `*` (downstream) | `upstream_failed` | dependency entered `failed` |

---

## 4. Engine Core

### 4.1 DAG Scheduler (Mise + Turborepo pattern)

**Algorithm**: Kahn's algorithm with bounded parallelism via `tokio::sync::Semaphore`.

**Data structure**: `petgraph::DiGraph<TaskId, ()>` with `HashMap<TaskId, NodeIndex>` for O(1) lookup.

**Scheduling loop**:
1. Compute ready set (tasks with all deps satisfied, status = `todo`)
2. Sort by priority, dispatch up to `--jobs N` via Semaphore
3. Each task runs as a `tokio::spawn` with `CancellationToken`
4. On completion: decrement dependent in-degrees, discover new ready tasks
5. On failure: propagate `upstream_failed` to all downstream tasks
6. Circuit breaker: N consecutive failures → cancel all in-flight, halt

**Key APIs**:
```rust
impl TaskDag {
    fn from_tasks(tasks: &[Task]) -> Result<Self>;
    fn ready_tasks(&self, status: &HashMap<TaskId, Status>) -> Vec<TaskId>;
    fn complete(&mut self, id: &TaskId) -> Vec<TaskId>;  // returns newly ready
    fn propagate_failure(&self, id: &TaskId) -> Vec<TaskId>;  // returns downstream
    fn detect_cycles(&self) -> Option<Vec<TaskId>>;
    fn critical_path(&self) -> Vec<TaskId>;
    fn split_task(&mut self, id: &TaskId, new: Vec<Task>) -> Result<()>;
    fn skip_task(&mut self, id: &TaskId) -> Vec<TaskId>;
}
```

### 4.2 Heartbeat Zombie Detection (Airflow + Temporal pattern)

Workers emit heartbeats every 10s. Watchdog checks every 15s. If no heartbeat within 60s:
- If retries remaining → `up_for_retry`
- Else → `failed` + propagate `upstream_failed`

### 4.3 Event Bus

`tokio::sync::broadcast` with enum-based events:

```rust
enum FlowEvent {
    TaskReady { task_id },
    TaskStarted { task_id, worker_pid },
    TaskCompleted { task_id, duration, evidence },
    TaskFailed { task_id, error, will_retry },
    TaskZombie { task_id, last_heartbeat },
    WaveStarted { wave_num, task_count },
    WaveCompleted { wave_num, results },
    EpicCompleted { epic_id, lead_time },
    GuardPassed { command, duration },
    GuardFailed { command, stderr },
    LockConflict { file, holder, requester },
    CircuitOpen { failures },
    DaemonStarted { pid },
    DaemonShutdown { reason },
}
```

Consumers: SQLite logger, TUI renderer, metrics collector, watchdog.

### 4.4 Daemon Mode (Docker pattern)

**Architecture**: CLI → Unix socket → Daemon (axum HTTP API)

**Lifecycle**:
1. `flowctl daemon start` → PID lock, bind socket, start subsystems
2. Subsystems: event logger, file watcher (notify), watchdog, metrics collector, scheduler
3. HTTP API: `/health`, `/status`, `/epics`, `/tasks`, `/start`, `/events` (WebSocket)
4. `flowctl daemon stop` → send shutdown via socket
5. Graceful shutdown: `CancellationToken` + `TaskTracker` drain with 10s timeout

**CLI smart routing**: If daemon running → use socket. If not → direct SQLite read/write. No daemon required for basic operations. **If daemon is detected (PID file exists) but socket is unreachable → error exit, do not fall back to direct write.** Falling back would bypass the event bus, making the operation invisible to TUI and other consumers.

---

## 5. TUI Dashboard

### Architecture (Lazygit component + Bottom data collection)

**Four tabs**: Tasks, DAG, Logs, Stats

**Component pattern**: Each panel implements a `Component` trait with `handle_key_events()`, `update()`, `render()`. Communication via `Action` enum over `mpsc` channel.

**Context-sensitive keybindings**: Lazygit's context stack pattern — each panel defines its own bindings, no conflicts between panels.

**Main loop**: `tokio::select!` multiplexing render timer (30fps), tick timer (250ms), keyboard events, daemon event stream.

### Tab 1: Tasks
- Sortable/filterable table with fuzzy search (nucleo crate)
- Status icons, domain, duration columns
- Progress bar with ETA
- Enter → task detail popup

### Tab 2: DAG
- ASCII dependency graph (ascii-dag or custom Sugiyama)
- Critical path highlighting (longest path in DAG)
- Status-colored nodes (green=done, yellow=running, red=failed, gray=todo)
- Interactive node navigation

### Tab 3: Logs
- Split-pane per-worker log streaming
- Level filtering (ERROR/WARN/INFO/DEBUG)
- Search within logs
- Auto-scroll with manual override

### Tab 4: Stats
- Sparkline: throughput (tasks/hour over time)
- BarChart: task duration histogram
- Gauge: success rate, epic progress
- Phase breakdown: research/implementation/review/verify percentages
- Token usage and estimated cost

### Toast notifications
- Bottom-right corner stack, auto-expire with TTL
- Levels: success (green), error (red), warning (yellow)

---

## 6. Observability

### Three-Tier Metrics

| Tier | Storage | Retention | Purpose |
|------|---------|-----------|---------|
| Raw events | `events` table | 90 days | Audit trail, debugging |
| Daily rollup | `daily_rollup` table | 1 year | Trend analysis |
| Monthly rollup | `monthly_rollup` table | Forever | Long-term tracking |

### Key Metrics

**Scheduling**: queue depth, scheduling latency, concurrency utilization
**Execution**: task duration, success/failure rate, retry rate, review iterations
**Business (DORA-mapped)**: lead time (epic create→close), throughput (epics/week), change failure rate (restart %), time to restore (blocked→running)
**Cost**: input/output tokens, cache hit rate, estimated USD cost per epic/task

### CLI Dashboard

```bash
flowctl stats                  # Summary
flowctl stats --epic fn-42     # Per-epic breakdown
flowctl stats --weekly         # Trend view
flowctl stats --tokens         # Token/cost analysis
flowctl stats --bottlenecks    # Phase bottleneck analysis
flowctl stats --format json    # Machine-readable export
```

### Retention (RTK pattern)

Auto-cleanup on every write (no cron needed):
- Raw events: 90 days
- Daily rollup: 365 days
- Monthly rollup: never deleted
- Incremental vacuum every 100 cleanups

---

## 7. Technology Choices

| Concern | Crate | Rationale |
|---------|-------|-----------|
| **CLI** | `clap 4` (derive) | RTK, mise, cargo all use it |
| **Errors** | `anyhow` + `thiserror` | `anyhow` for app, `thiserror` for library errors |
| **Error display** | `color-eyre` + `miette` | Rich panics + user-facing diagnostics |
| **SQLite** | `rusqlite` + `deadpool-sqlite` | Sync for CLI (no tokio needed), connection pool for daemon. RTK-validated pattern |
| **DAG** | `petgraph` | 328M downloads, toposort + cycle detection built-in |
| **Concurrency** | `tokio` + `Semaphore` | mise-validated scheduler pattern |
| **Shutdown** | `tokio-util` (`CancellationToken` + `TaskTracker`) | Official, composable |
| **HTTP API** | `axum` | First-class Unix socket support |
| **TUI** | `ratatui` + `crossterm` | 19k stars, bottom/gitui proven |
| **File watch** | `notify` | watchexec's foundation |
| **Process mgmt** | `nix` | Unix signals, process groups |
| **PID lock** | `pidlock` | Stale detection built-in |
| **Serialization** | `serde` + `serde_json` + `serde_yaml` | Ecosystem standard |
| **Regex** | `regex` + `lazy_static` | RTK-validated lazy compilation |
| **Logging** | `tracing` | Structured, span-based, SQLite subscriber |
| **Fuzzy search** | `nucleo` | Helix editor's engine |
| **DAG render** | `ascii-dag` or custom | Terminal-native Sugiyama layout |
| **Shell completions** | `clap_complete` | Auto-generated from clap Command |
| **Time** | `chrono` | Serde integration |

### Cargo Workspace Structure

```
flowctl/
├── Cargo.toml                    # workspace
├── crates/
│   ├── flowctl-core/             # Zero framework deps
│   │   └── types, id, state_machine, dag, frontmatter
│   ├── flowctl-db/               # SQLite layer
│   │   └── pool, repo, indexer, migrations/
│   ├── flowctl-scheduler/        # DAG scheduler + event bus
│   │   └── scheduler, watcher, event_bus, watchdog
│   ├── flowctl-cli/              # CLI entry point
│   │   └── main, commands/, output
│   ├── flowctl-daemon/           # Daemon process
│   │   └── server, handlers, lifecycle
│   └── flowctl-tui/              # TUI dashboard
│       └── app, tabs/, widgets/, toast
├── build.rs
└── README.md
```

### Feature Flags

```toml
[features]
default = ["cli"]
cli = ["dep:clap", "dep:clap_complete"]
tui = ["dep:ratatui", "dep:crossterm", "dep:nucleo"]
daemon = ["dep:tokio/full", "dep:axum", "dep:notify"]
api = ["daemon"]
```

### Build Profile

```toml
[profile.release]
opt-level = "z"
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

---

## 8. Phased Delivery Plan

### Phase 0: Foundation (2-3 weeks, LOW risk)

- Cargo workspace: `core` + `cli` + `db`
- Core types: Task, Epic, Phase, Evidence, TaskId
- Frontmatter parser/writer (serde_yaml) with `schema_version: 1`
- State machine with compile-time transition validation
- SQLite schema + migrations (rusqlite_migration)
- JSON → SQLite importer (`flowctl reindex`)
- **CLI interface contract spec**: define every command's args, JSON output schema, and exit codes
- **trycmd test suite**: Markdown-based CLI contract tests (Rust output must match Python output)
- clap skeleton with subcommands registered
- Test harness: `assert_cmd` + `insta` + `trycmd`
- CI: GitHub Actions compile + test

### Phase 1: CLI Parity (3-4 weeks, MEDIUM risk)

- Port commands: `init`, `status`, `show`, `ready`, `next`, `start`, `done`, `block`
- Port commands: `epic create/close`, `task create/skip/split`
- Port commands: `lock/unlock/lock-check`
- Port commands: `dep add/rm`, `restart`, `tasks`, `epics`, `list`, `cat`
- Markdown ↔ SQLite bidirectional sync
- `flowctl reindex` full rebuild
- Integration tests: compare Rust vs Python output
- `--legacy-json` feature flag for fallback
- Ship as opt-in alongside Python version

### Phase 2: Enhanced CLI (2-3 weeks, LOW risk)

- petgraph DAG: cycle detection, critical path, topological sort
- `flowctl stats` metrics dashboard (RTK gain pattern)
- `flowctl stats --tokens` token/cost tracking
- Event logging with auto-cleanup (90-day retention)
- `upstream_failed` propagation on task failure
- Shell completion generation (bash/zsh/fish)
- Remove `--legacy-json`, full SQLite
- `color-eyre` + `miette` error display

### Phase 3: TUI (3-4 weeks, MEDIUM risk)

- `feature = "tui"`, separate crate
- Four tabs: Tasks / DAG / Logs / Stats
- Component architecture with Action channels
- Lazygit-style context keybindings
- ascii-dag dependency graph with critical path
- Sparkline, Gauge, BarChart widgets
- Toast notification system
- No daemon required: direct SQLite + polling
- Snapshot tests with insta

### Phase 4: Daemon (3-4 weeks, HIGH risk)

- `feature = "daemon"`, separate crate
- Kahn's algorithm + Semaphore concurrent scheduler
- File watcher (notify) → auto-reindex
- Heartbeat watchdog (zombie detection)
- Circuit breaker (consecutive failure halt)
- broadcast event bus with multiple consumers
- axum HTTP API over Unix socket
- TUI connects to daemon event stream (WebSocket)
- Graceful shutdown: CancellationToken + TaskTracker
- PID lock with stale detection

### Phase 5: Distribution (2 weeks, LOW risk)

- cargo-dist: automated GitHub releases (Linux/macOS/Windows)
- Homebrew formula
- install.sh with checksum verification
- Binary size optimization
- Complete documentation
- 1.0 release

**Total**: ~15-20 weeks for full platform

---

## 9. Migration Strategy

### Backward Compatibility

1. **Phase 1**: Ship Rust binary alongside Python. Users opt-in via `FLOWCTL_RUST=1`
2. **Phase 1**: Rust reads existing `.flow/` JSON files via `flowctl reindex`
3. **Phase 1**: `--legacy-json` flag falls back to JSON file operations
4. **Phase 2**: Remove JSON fallback. SQLite is the only runtime store. Markdown remains canonical.
5. **Skills/agents**: Zero changes needed. They call `flowctl` CLI which has identical interface.

### Data Migration

```bash
# One-time migration from Python JSON to Rust SQLite
flowctl reindex          # Scans .flow/**/*.md frontmatter → builds SQLite
                         # JSON files in .flow/tasks/*.json become redundant
                         # JSON files in .flow/epics/*.json become redundant
```

### Testing During Migration

- Integration tests run both Python and Rust implementations
- Compare JSON output for identical inputs
- Capnproto/hash comparison for state consistency (Turborepo pattern)

---

## 10. Open Source References

### Architecture Patterns Adopted

| Pattern | Source | Component |
|---------|--------|-----------|
| Markdown canonical, DB cache | Fossil SCM, MarkdownDB | Data architecture |
| Kahn's algorithm + Semaphore | Mise, dag_exec | DAG scheduler |
| Content-addressable caching | Turborepo | Future: skip unchanged tasks |
| upstream_failed propagation | Airflow | Failure handling |
| Heartbeat zombie detection | Airflow, Temporal | Teams mode reliability |
| Circuit breaker | Production SRE | Runaway failure prevention |
| Rescue DAG / checkpoint | HTCondor DAGMan | Crash recovery |
| Dynamic DAG mutation | Prefect | Runtime task split/skip |
| CLI → Unix socket → Daemon | Docker | Daemon communication |
| CancellationToken + TaskTracker | tokio-util | Graceful shutdown |
| Context stack keybindings | Lazygit | TUI navigation |
| Three-thread TUI | Bottom (btm) | TUI data pipeline |
| AsyncSingleJob dedup | Gitui | Stale request prevention |
| Observer + delta detection | K9s | Efficient TUI updates |
| SQLite WAL + busy_timeout | RTK, production consensus | Concurrent access |
| Auto-cleanup on write | RTK tracking.rs | Retention management |
| Weighted CPT cost estimation | RTK cc_economics.rs | Token cost tracking |
| TTY-aware dashboard formatting | RTK gain.rs | CLI output |
| Component architecture | Ratatui official patterns | TUI structure |
| Feature flags for modularity | Nushell, Zellij | Incremental delivery |
| Multi-crate workspace | Zellij, Nushell | Code organization |
| Incremental language rewrite | Turborepo Go→Rust | Migration strategy |

---

## 11. CEO Review Findings (2026-04-04)

### Decisions Made

| # | Finding | Decision |
|---|---------|----------|
| 1 | CLI-Daemon fallback when socket unreachable | Error exit, no fallback to direct write |
| 2 | SQLite library choice | rusqlite (sync) instead of sqlx (async). CLI doesn't need tokio |
| 3 | Write order (Markdown vs SQLite) | SQLite first (transaction), then Markdown. Reindex recovers |
| 4 | Frontmatter schema version | Added `schema_version: 1` to all frontmatter formats |
| 5 | CLI interface contract | Added contract spec + trycmd tests to Phase 0 |

### Open Items (to address during implementation)

1. **Invalid frontmatter values**: reindex should warn + skip, not crash
2. **SQLite write failure (disk full)**: needs explicit error handling and user notification
3. **Concurrent Markdown modification**: need timestamp check or advisory lock before overwriting
4. **Orphan tasks**: task referencing non-existent epic should warn, not crash during reindex
5. **Duplicate IDs**: two .md files with same id — define resolution strategy (newer wins? error?)
6. **Unix socket permissions**: set mode 0600 on daemon socket
7. **Daemon health metrics**: track uptime, memory, WAL size, event bus backlog
8. **TUI empty states**: define what each tab shows when no epics/tasks/daemon exist
9. **TUI performance**: use memory cache + event-driven updates, not per-frame SQLite reads

---

## 12. Eng Review Findings (2026-04-04)

### Decisions Made

| # | Finding | Decision |
|---|---------|----------|
| 1 | SQLite location incompatible with git worktree | Move to git common-dir (matches Python behavior) |
| 2 | 4 error handling crates (redundant) | Drop color-eyre, keep anyhow + thiserror + miette |
| 3 | lazy_static crate (unnecessary) | Use std::sync::LazyLock (Rust 1.80+) |

### Additional Findings (no decision needed)

1. **Feature flags vs crate boundary**: clarify which crate owns the bin target and how features compose
2. **Trigger performance during reindex**: disable triggers during bulk import, rebuild aggregates after
3. **broadcast channel capacity**: use mpsc for critical consumers (SQLite logger), broadcast for non-critical (TUI)
4. **Test coverage**: 30 gaps identified across 42 code paths (29% planned coverage). Full test plan artifact written.
5. **Parallelization**: Phases 3 (TUI) and 4 (Daemon) can run in parallel after Phase 1, compressing 5 sequential phases to 3.

### Critical Gaps (7 total)

1. Invalid frontmatter status value → silent reindex failure
2. Disk full during frontmatter write → silent data loss
3. Concurrent Markdown modification → silent overwrite
4. SQLite transaction failure → silent state corruption
5. broadcast consumer lag → silent event loss
6. Orphan task during reindex → silent skip or crash
7. Duplicate ID during reindex → undefined behavior

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | ISSUES_OPEN | mode: HOLD_SCOPE, 3 critical gaps |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | — | — |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | ISSUES_OPEN | 5 issues, 7 critical gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | — | — |

- **UNRESOLVED:** 0 unresolved decisions across all reviews
- **VERDICT:** CEO + ENG reviewed. 7 critical gaps to resolve during implementation. No blocking issues for starting Phase 0.
