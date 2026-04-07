# ADR Integration Guide

Architecture Decision Records capture the *why* behind significant technical decisions. This guide covers when to write them, where to store them, and how they integrate with flow-code.

## When to Create an ADR

Write an ADR when making a decision that would be expensive to reverse:

- **Technology choices** — frameworks, libraries, major dependencies
- **Architectural patterns** — data model design, API style, auth strategy
- **Infrastructure decisions** — hosting, build tools, deployment approach
- **Pattern changes** — moving from one approach to another (e.g., REST to GraphQL)

Do NOT write ADRs for routine implementation choices, obvious decisions, or throwaway prototypes.

## Where to Store ADRs

Use `docs/decisions/` with sequential numbering:

```
docs/decisions/
  ADR-001-json-file-based-storage.md
  ADR-002-wave-checkpoint-execution-model.md
  ADR-003-teams-file-locking-protocol.md
```

Use the template at `references/adr-template.md` as your starting point.

## Referencing ADRs in Task Specs

When a task implements or depends on an architectural decision, reference the ADR in the task spec:

```bash
flowctl task create --title "Implement file locking" \
  --spec "Implements ADR-003. See docs/decisions/ADR-003-teams-file-locking-protocol.md"
```

In inline code, link to the ADR near the relevant implementation:

```
// Auth strategy per ADR-002. See docs/decisions/ADR-002-auth-strategy.md
```

## Integration with /flow-code:plan

During planning, ADRs surface naturally at two points:

1. **Plan creation** — When `/flow-code:plan` encounters an architectural decision, create the ADR as a task in the epic. The ADR task should complete before implementation tasks that depend on it.

2. **Plan review** — `/flow-code:plan-review` should verify that significant architectural decisions have corresponding ADRs. Missing ADRs are a review finding.

### Example: ADR as a Plan Task

```
Epic: fn-50-migrate-to-graphql
  Task 1: Write ADR-005 documenting REST-to-GraphQL migration rationale
  Task 2: Implement GraphQL schema (depends on Task 1)
  Task 3: Migrate endpoints (depends on Task 2)
```

## ADR Lifecycle

```
PROPOSED  ->  ACCEPTED  ->  SUPERSEDED by ADR-XXX
                         ->  DEPRECATED
```

Never delete old ADRs. When a decision changes, write a new ADR that supersedes the old one. The historical record is the whole point.
