# ADR-003: Rust Single Binary for flowctl

## Status
ACCEPTED

## Date
2026-04-07

## Context
The original flow-next used shell scripts and Node.js for task management. This created dependency issues (Node version, npm install), slow startup times, and fragile string parsing. Claude Code plugins need to be fast and self-contained.

## Decision
Rewrite the core engine in Rust as a single binary (`flowctl`) with two crates: `flowctl-core` (library) and `flowctl-cli` (binary). All state operations, DAG management, and file locking are native Rust. Skills and agents remain Markdown files (no build step needed).

## Alternatives Considered

| Option | Pros | Cons | Why not? |
|--------|------|------|----------|
| Shell scripts | Zero build step, familiar | Fragile, slow, no type safety | Unmaintainable at scale |
| Node.js CLI | Ecosystem, easy JSON | Dependency hell, startup time | Claude Code users shouldn't need Node |
| Go binary | Fast, single binary | Less expressive error handling | Team familiarity with Rust |
| Rust binary (chosen) | Fast, safe, single binary, no runtime | Build complexity | Worth it for reliability |

## Consequences
- **Easier:** `flowctl` starts in <50ms, no runtime dependencies, type-safe state handling
- **Harder:** Contributors need Rust toolchain, compile times are longer
- **Risk:** Smaller contributor pool. Mitigated by keeping skills/agents as Markdown (most contributions don't touch Rust).
