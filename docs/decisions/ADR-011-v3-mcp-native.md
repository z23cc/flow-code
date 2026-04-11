# ADR-011: V3 MCP-Native Goal-Driven Architecture

**Status**: Accepted
**Date**: 2026-04-11
**Deciders**: z23cc
**Supersedes**: V1 pipeline state machine

## Context

flowctl has grown to 90+ CLI commands, 52 skills, 24 agents, and 8 hook points. Control logic is scattered across multiple protocol layers. The 6-phase linear pipeline forces all tasks through the same steps regardless of complexity.

## Decision

Transform flowctl from a CLI-only pipeline tool into an MCP-native goal-driven adaptive engine:

1. **MCP Server**: `flowctl serve` starts an rmcp-based MCP server (stdio transport) exposing 16 structured tools
2. **Goal Model**: Replace Epic + 6-phase pipeline with Goal + orthogonal PlanningMode (Direct/Graph) × SuccessModel (Criteria/Numeric/Mixed)
3. **Async Boundary**: tokio + rmcp are confined to `flowctl-mcp` crate. `flowctl-core` remains fully synchronous
4. **3-Crate Architecture**: flowctl-core (domain+storage+engine), flowctl-mcp (MCP server), flowctl-cli (thin CLI facade)
5. **Pipeline Freeze**: `pipeline.rs` and `pipeline_phase.rs` receive bug fixes only, no new features

## Consequences

- flowctl binary grows to include MCP server capability
- First introduction of async runtime (tokio) — strictly confined to flowctl-mcp
- Existing CLI commands preserved via compat shim layer
- Old .flow/ data migrated via `flowctl migrate v3`, originals archived to `.flow/.archive/v1/`

## Alternatives Considered

- **Incremental evolution**: Continue adding to pipeline model. Rejected: complexity ceiling already reached.
- **V3 Draft (merged ExecutionMode)**: Single enum mixing planning and success. Rejected: leads to condition sprawl.
- **5-crate split**: Premature for ~27K LOC codebase. Defer until core exceeds 20K LOC.
