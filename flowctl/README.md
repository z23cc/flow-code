# flowctl

A fast, native task and workflow engine for structured, plan-first development. Manages epics, tasks, dependencies, and state machines in a local `.flow/` directory backed by libSQL (async, native vector search).

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

## Architecture

flowctl is split into four crates:

```
flowctl-core        Core types, ID parsing, state machine, DAG, JSON I/O
flowctl-db          libSQL storage layer (async, native vector search)
flowctl-service     Business logic service layer — unifies CLI, daemon, and MCP execution paths
flowctl-cli         CLI entry point (clap) — the `flowctl` binary
```

**Data flow**: CLI parses commands via `clap`, calls into `flowctl-service` for business logic, which uses `flowctl-db` for storage and `flowctl-core` types. The DAG module computes task dependencies and execution order.

## Release profile

Release builds are size-optimized:

- `opt-level = "z"` (minimize size)
- `lto = "fat"` (full link-time optimization)
- `codegen-units = 1` (single codegen unit)
- `panic = "abort"` (no unwinding overhead)
- `strip = true` (strip debug symbols)

## License

MIT
