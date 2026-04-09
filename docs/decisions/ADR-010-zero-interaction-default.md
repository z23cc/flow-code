---
id: ADR-010
status: accepted
date: 2026-04-09
tags: [ux, automation]
verify: "grep -q 'ZERO-INTERACTION' skills/flow-code-run/SKILL.md"
scope: "skills/flow-code-run/"
---
# ADR-010: Zero-Interaction as Default Mode

## Status
ACCEPTED

## Date
2026-04-09

## Context
AI coding agents are most valuable when they run autonomously. Every question asked to the user is a context switch that breaks flow. The user said "add OAuth" — they want a PR, not a conversation.

## Decision
`/flow-code:go` runs the entire pipeline (brainstorm → plan → work → review → close) without asking any questions. AI reads git state, `.flow/` config, and codebase to make all decisions (branch, review backend, research depth, task sizing) autonomously.

## Consequences
- **Constraint**: The flow-code-run skill MUST NOT use AskUserQuestion at any point
- **Benefit**: One sentence in → draft PR out, zero friction
- **Benefit**: Enables Ralph (unattended autonomous operation)
- **Trade-off**: AI may make suboptimal decisions. Mitigated by three-layer review gates and project-context.md constraints

## Rejected Alternatives
- Interactive by default (BMAD approach): Better for requirement discovery but breaks autonomous flow
- Hybrid (ask only for critical decisions): Hard to define "critical" consistently
- Config-driven (ask/auto per phase): Complexity for marginal benefit
