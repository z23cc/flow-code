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

**Full autopilot (idea to PR):** `/flow-code:go "your idea"`

**Individual phases:**
- Brainstorm: `/flow-code:brainstorm "idea"`
- Plan only: `/flow-code:go "idea" --plan-only`
- Work on existing epic: `/flow-code:go fn-1-add-oauth`

## Skill Selection Flowchart

```
What are you trying to do?
│
├─ Build a feature from scratch?
│  └─ /flow-code:go "description"
│     (runs: brainstorm → plan → work → review → close)
│
├─ Plan without implementing?
│  └─ /flow-code:go "description" --plan-only
│
├─ Resume existing work?
│  └─ /flow-code:go fn-N-slug
│
├─ Explore an idea before committing?
│  └─ /flow-code:brainstorm "idea"
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
| `flow-code-incremental` | All tasks | Vertical slicing, Implement→Test→Verify→Commit |
| `flow-code-code-review` | All tasks (Phase 6) | Five-axis review, severity labels |
| `flow-code-frontend-ui` | domain=frontend | Component architecture, WCAG 2.1 AA |
| `flow-code-security` | domain=backend/architecture/ops | OWASP Top 10, three-tier boundaries |
| `flow-code-tdd` | --tdd flag or domain=testing | Red-Green-Refactor, Prove-It Pattern |
| `flow-code-api-design` | domain=backend/architecture | Contract-first, Hyrum's Law, error semantics |
| `flow-code-simplify` | /flow-code:simplify or auto-improve | Chesterton's Fence, 18 patterns |
| `flow-code-debug` | domain=testing or bug tasks | Reproduce→Localize→Reduce→Fix→Guard |
| `flow-code-performance` | Performance tasks | Measure→Identify→Fix→Verify→Guard |

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
