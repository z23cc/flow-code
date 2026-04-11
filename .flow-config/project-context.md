# Project Context

> Shared technical standards for all agents. Auto-loaded by workers during re-anchoring.
> Focus on what's **unobvious** — things agents can't infer from code alone.

## Technology Stack
- Language: Rust (edition 2024, MSRV 1.85)
- Runtime: tokio (async, flowctl-mcp crate ONLY)
- MCP SDK: rmcp 0.16 (official Rust MCP SDK)
- DAG: petgraph 0.7
- Search: NgramIndex (bincode), nucleo-matcher (fuzzy)
- Storage: JSON/JSONL files in .flow/ (zero external deps, no SQLite)
- Testing: cargo test + trycmd snapshots
- Linting: cargo clippy (unsafe_code = forbid)

## Guard Commands
```yaml
test: "cd flowctl && cargo test --all"
lint: "cd flowctl && cargo clippy --all -- -D warnings"
typecheck: "cd flowctl && cargo build --all"
format_check: ""
```

## Critical Implementation Rules
- CI: GitHub Actions
- Async boundary: tokio + rmcp are allowed ONLY in `flowctl-mcp` crate. `flowctl-core` MUST remain fully synchronous. MCP crate calls core via `spawn_blocking`.
- Storage: all state is JSON/JSONL files in `.flow/`. No SQLite, no external database.
- Architecture: V3 MCP-native goal-driven engine (see docs/v3-final-architecture.md)
- Pipeline freeze: `pipeline.rs` and `pipeline_phase.rs` are FROZEN — bug fixes only, no new features

## File Conventions
<!-- Maps domains to file patterns for auto domain assignment -->
```yaml
frontend: []
backend: ["flowctl/crates/"]
testing: ["scripts/"]
docs: []
```

## Architecture Decisions
- ADR-011: flowctl is MCP-native runtime (see docs/decisions/ADR-011-v3-mcp-native.md)
- Goal-driven, not pipeline-driven: PlanningMode (Direct/Graph) × SuccessModel (Criteria/Numeric/Mixed)
- 3-crate workspace: flowctl-core (sync domain+storage+engine), flowctl-mcp (async MCP server), flowctl-cli (thin CLI)
- PolicyEngine with 2 adapters (MCP internal + PreToolUse hook) for physical enforcement
- ProviderRegistry with trait abstractions for review/planning backends

## Non-Goals
- Do not add SQLite or any external database dependency
- Do not add async/tokio to flowctl-core (only flowctl-mcp)
- Do not break existing flowctl CLI commands (provide compat shim)
- Do not modify pipeline.rs or pipeline_phase.rs (FROZEN — bug fixes only)
- Do not hardcode RP or Codex logic into core engine (use ProviderRegistry traits)
