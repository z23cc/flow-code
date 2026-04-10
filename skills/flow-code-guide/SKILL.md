---
name: flow-code-guide
description: "Skill discovery guide. Use at session start or when unsure which skill to apply. Shows the flowchart for selecting the right flow-code skill."
tier: 1
user-invocable: true
---

# Flow-Code Skill Guide

## When to Use

- Session start (recommended: skim this to orient)
- Unsure which skill or command to use
- New to flow-code and want the overview

## Quick Start

**Full autopilot / resume:** `/flow-code:go "your idea"` or `/flow-code:go fn-1`

**Other common front doors:**
- Plan only: `/flow-code:plan "idea"`
- Brainstorm first: `/flow-code:brainstorm "idea"`
- Spec first: `/flow-code:spec "idea, change, or refactor"`
- ADR capture: `/flow-code:adr "decision or architectural change"`
- Replacement/removal guidance: `flow-code-deprecation` (skill, no slash command)

## Front-Door Routing

- `go` = full execution path or resume path.
- `plan` = planning-only when you do **not** want execution yet.
- `brainstorm` = open-ended exploration / pressure-testing.
- `spec` = artifact-first requirements capture.
- `adr` = durable architecture decision capture.
- `flow-code-deprecation` = replacement/removal guidance surface (skill, not a slash command).

## Skill Selection Flowchart

```
What are you trying to do?
│
├─ Build a feature from scratch?
│  └─ /flow-code:go "description"
│     (runs: brainstorm → plan → work → review → close)
│
├─ Plan without implementing?
│  └─ /flow-code:plan "description"
│     (use go --plan-only only if you're already on the go path)
│
├─ Resume existing work?
│  └─ /flow-code:go fn-N-slug
│
├─ Explore an idea before committing?
│  └─ /flow-code:brainstorm "idea"
│
├─ Write a reusable requirements doc first?
│  └─ /flow-code:spec "idea / change / refactor"
│     (writes a planning-ready requirements spec)
│
├─ Record an architectural decision?
│  └─ /flow-code:adr "decision"
│     (writes or updates an ADR in docs/decisions/)
│
├─ Replace or remove an old surface?
│  └─ flow-code-deprecation
│     (skill, no slash command; guidance for replacement, deprecation, and clean removal)
│
├─ Build/modify UI components?
│  └─ /flow-code:frontend-ui "component description"
│     (design system, accessibility, AI aesthetic avoidance)
│
├─ Simplify complex code?
│  └─ /flow-code:simplify "file or directory"
│     (Chesterton's Fence, behavior preservation)
│
├─ Debug a bug or test failure?
│  └─ Use flow-code-debug methodology
│     (reproduce → localize → reduce → fix → guard)
│
├─ Audit codebase readiness?
│  └─ /flow-code:prime
│
├─ Map codebase architecture?
│  └─ /flow-code:map
│
├─ QA test a web app?
│  └─ /flow-code:qa "url"
│
├─ Visual design audit?
│  └─ /flow-code:design-review "url"
│
└─ Improve code autonomously?
   └─ /flow-code:auto-improve "goal" --scope dir/
```

## Engineering Skills (loaded by workers automatically)

These skills are loaded during `/flow-code:go` based on task domain. You don't invoke them directly — they guide worker behavior:

| Skill | Loaded When | Core Methodology |
|-------|-------------|-----------------|
| **Core (all tasks)** | | |
| `flow-code-incremental` | All tasks | Vertical slicing, Implement→Test→Verify→Commit |
| `flow-code-code-review` | All tasks (Phase 6) | Five-axis review, severity labels |
| `flow-code-error-handling` | All tasks with error paths | Retry, circuit breaker, graceful degradation |
| **Frontend** | | |
| `flow-code-frontend-ui` | domain=frontend | Component architecture, WCAG 2.1 AA |
| `flow-code-state-management` | domain=frontend | State classification, decision ladder, server state |
| `flow-code-i18n` | i18n/l10n tasks | ICU messages, Intl APIs, RTL support |
| **Backend** | | |
| `flow-code-api-design` | domain=backend/architecture | Contract-first, Hyrum's Law, error semantics |
| `flow-code-security` | domain=backend/architecture/ops | OWASP Top 10, three-tier boundaries |
| `flow-code-database` | DB/ORM tasks | Migration safety, N+1 detection, indexing |
| `flow-code-auth` | Auth tasks | OAuth, JWT lifecycle, RBAC, resource-level auth |
| `flow-code-caching` | Caching tasks | Cache-aside, TTL, invalidation strategies |
| **Infrastructure** | | |
| `flow-code-observability` | domain=backend/ops | Structured logging, tracing, metrics (RED/USE) |
| `flow-code-monitoring` | domain=ops | SLO/SLI, dashboards, alerting, runbooks |
| `flow-code-containerization` | Docker/K8s tasks | Multi-stage builds, health probes, security |
| `flow-code-microservices` | Service boundary tasks | Saga, events, data ownership, decomposition |
| **Quality** | | |
| `flow-code-tdd` | --tdd flag or domain=testing | Red-Green-Refactor, Prove-It Pattern |
| `flow-code-debug` | domain=testing or bug tasks | Reproduce→Localize→Reduce→Fix→Guard |
| `flow-code-performance` | Performance tasks | Measure→Identify→Fix→Verify→Guard |
| `flow-code-simplify` | /flow-code:simplify or auto-improve | Chesterton's Fence, 18 patterns |
| `flow-code-documentation` | docs domain or releases | ADRs, README, API docs, changelogs |

## Reference Checklists

Quick-lookup docs for reviews and verification:

- [Accessibility Checklist](../../references/accessibility-checklist.md) — WCAG 2.1 AA
- [Performance Checklist](../../references/performance-checklist.md) — Core Web Vitals, backend, frontend
- [Security Checklist](../../references/security-checklist.md) — OWASP Top 10, headers, secrets
- [Testing Patterns](../../references/testing-patterns.md) — AAA, pyramid, anti-patterns, Prove-It

## Pipeline Overview

```
/flow-code:go "idea"
  ├── Brainstorm (AI self-interview)
  ├── Plan (scouts + task creation)
  ├── Plan Review (RP/Codex)
  ├── Work (parallel workers in worktrees)
  │   └── Worker phases: investigate → implement → verify → commit → review
  ├── Impl Review (adversarial)
  └── Close (pre-launch checklist → PR)
```
