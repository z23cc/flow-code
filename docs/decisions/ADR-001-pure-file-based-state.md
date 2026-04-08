# ADR-001: Pure File-Based State (No Database)

## Status
ACCEPTED

## Date
2026-04-07

## Context
Flow-Code needs persistent state for task DAGs, pipeline progress, file locks, and review receipts. Early versions used libSQL (embedded SQLite) with fastembed for vector search. This added significant binary size, async runtime complexity, and cross-platform build issues.

The plugin runs as a CLI tool invoked by Claude Code — it doesn't need concurrent connections, complex queries, or full-text search. State access patterns are simple: read-modify-write on small JSON files.

## Decision
Remove libSQL and fastembed entirely. Use plain JSON/JSONL files in `.flow/` for all state, with advisory file locks (`fs2`) for concurrency safety and atomic rename for crash safety.

## Alternatives Considered

| Option | Pros | Cons | Why not? |
|--------|------|------|----------|
| libSQL (SQLite) | Familiar SQL, ACID | Binary bloat (~15MB), async runtime, cross-compile pain | Overkill for simple key-value state |
| JSON files (chosen) | Zero deps, readable by any tool, trivial debugging | No complex queries | Queries not needed — state is small |
| RocksDB | Fast, embedded | C++ dependency, cross-compile, opaque format | Even more bloat than SQLite |

## Consequences
- **Easier:** Single binary with no native deps, state inspectable via `cat`/`jq`, simpler CI
- **Harder:** No SQL queries — must load full file to filter (acceptable given file sizes < 100KB)
- **Risk:** File locking is advisory-only — malicious or buggy tools could corrupt state. Mitigated by atomic rename writes.
