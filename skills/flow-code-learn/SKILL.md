---
name: flow-code-learn
description: V3 Knowledge management — search, compound, refresh patterns.
user_invocable: true
---

# V3 Learn — Knowledge Management

> Manage the three-layer knowledge pyramid: Learnings → Patterns → Methodology.

## Commands

### Search Knowledge
```bash
flowctl knowledge search "query" --limit 10 --json
```
Searches across all three layers. Returns patterns (highest value), learnings (raw experience), and methodology rules.

### Record a Learning
```bash
flowctl knowledge record <goal-id> "content" --kind discovery --json
```
Kinds: `success`, `failure`, `discovery`, `pitfall`.

### Compound Learnings → Patterns
```bash
flowctl knowledge compound <goal-id> --json
```
Groups learnings by tags. Promotes clusters of 3+ to patterns. Boosts confidence on validated existing patterns.

### Refresh Stale Patterns
```bash
flowctl knowledge refresh --json
```
Decays confidence on patterns not used within their `decay_days` (default 90). Patterns below 0.3 confidence are flagged for review.

## MCP Tools (Alternative)

When MCP server is running, use these tools directly:
- `knowledge_search` — search all layers
- `knowledge_record` — record a learning
- `knowledge_compound` — compound after goal completion
- `knowledge_refresh` — decay stale patterns
