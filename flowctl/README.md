# flowctl

A fast, native task and workflow engine for structured, plan-first development. Manages epics, tasks, dependencies, and state machines in a local `.flow/` directory backed by SQLite.

flowctl is the Rust rewrite of the Python `flowctl` CLI from [flow-code](https://github.com/anthropics/flow-code), designed for speed, cross-platform support, and zero-dependency deployment.

## Installation

### From source

```sh
cargo install --path crates/flowctl-cli
```

### From GitHub releases

```sh
curl -fsSL https://raw.githubusercontent.com/anthropics/flow-code/main/flowctl/install.sh | sh
```

Set `FLOWCTL_INSTALL_DIR` to change the install location (default: `/usr/local/bin`).
Set `FLOWCTL_VERSION` to pin a specific version (e.g., `v0.1.0`).

### From crates.io (coming soon)

```sh
cargo install flowctl-cli
```

## Quick start

```sh
# Initialize a new .flow directory
flowctl init

# Create an epic
flowctl epic create "Build auth system"

# Add tasks
flowctl task create -e ep-1 "Design token schema" --domain backend
flowctl task create -e ep-1 "Implement JWT middleware" --domain backend
flowctl dep add tsk-2 tsk-1

# Start work
flowctl start tsk-1

# Complete with evidence
flowctl done tsk-1 --summary-file summary.md --evidence-json evidence.json

# Check status
flowctl status
flowctl tasks -e ep-1
```

## Feature flags

| Flag | Crate | Effect |
|------|-------|--------|
| `tui` | flowctl-cli | Enables the TUI dashboard (`flowctl tui`) |
| `daemon` | flowctl-daemon | Enables the background daemon with HTTP API |
| `daemon` | flowctl-scheduler | Enables file-watcher integration |

Build with all features:

```sh
cargo build --release --all-features
```

## Architecture

flowctl is split into six crates for modularity:

```
flowctl-core        Core types, ID parsing, state machine, YAML/JSON I/O
flowctl-db          SQLite storage layer (rusqlite + migrations)
flowctl-scheduler   DAG-based task scheduler and event bus
flowctl-cli         CLI entry point (clap) — the `flowctl` binary
flowctl-daemon      Background daemon: scheduler, file watcher, HTTP/WS API
flowctl-tui         Terminal UI dashboard (ratatui)
```

**Data flow**: CLI parses commands via `clap`, calls into `flowctl-db` for storage, which uses `flowctl-core` types. The scheduler builds a DAG from task dependencies to determine execution order. The daemon wraps the scheduler with file watching and an HTTP API. The TUI provides a live dashboard.

## Release profile

Release builds are size-optimized:

- `opt-level = "z"` (minimize size)
- `lto = "fat"` (full link-time optimization)
- `codegen-units = 1` (single codegen unit)
- `panic = "abort"` (no unwinding overhead)
- `strip = true` (strip debug symbols)

## License

MIT
