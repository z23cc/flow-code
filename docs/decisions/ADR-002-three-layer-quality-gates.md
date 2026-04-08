# ADR-002: Three-Layer Quality Gate System

## Status
ACCEPTED

## Date
2026-04-08

## Context
A single review pass (lint or AI review) catches some issues but misses entire categories. Lint catches syntax but not spec drift. AI review catches design issues but has blind spots from the same model that wrote the code. We needed independent, complementary layers.

## Decision
Implement three non-overlapping review layers that every epic passes through:

1. **Guard** (`flowctl guard`) — Deterministic: lint, type-check, test. Runs at worker Phase 6, wave checkpoint, and close.
2. **RP Plan-Review** — Code-aware spec validation via RepoPrompt context_builder. Catches spec-code misalignment.
3. **Codex Adversarial** (`flowctl codex adversarial`) — Cross-model (GPT) adversarial review. Different model family catches blind spots Claude might share.

## Alternatives Considered

| Option | Pros | Cons | Why not? |
|--------|------|------|----------|
| Single AI review | Simple | Same model, same blind spots | Insufficient coverage |
| Lint + one AI review | Better | Still same-model bias | Missing adversarial perspective |
| Three layers (chosen) | Complementary coverage, cross-model | Slower, requires Codex API | Worth it for quality |

## Consequences
- **Easier:** High confidence in shipped code, catches issues at the right layer
- **Harder:** Pipeline takes longer, requires both RP and Codex backends configured
- **Risk:** Review fatigue from false positives. Mitigated by circuit breaker (max 2-3 iterations per layer)
