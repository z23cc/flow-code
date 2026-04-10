---
name: flow-code-documentation
description: "Use when writing ADRs, API docs, READMEs, changelogs, or any technical documentation. Covers the 'document the why' principle, ADR templates, and doc-as-code workflow."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: documentation,adr,readme,changelog,docs -->

# Technical Documentation

## Overview

Document the WHY, not the what. Code shows what it does — documentation explains why it exists, how to use it, and what decisions shaped it. Every document has an audience and a purpose. Write for the reader, not the author.

## When to Use

- Recording architectural decisions (ADR)
- Writing or updating README
- Documenting APIs (REST, GraphQL, CLI)
- Maintaining changelogs
- Onboarding documentation
- Post-incident write-ups

## Command Front Doors (Discovery)

Use these first when you want command-led documentation workflows:

- `/flow-code:spec "idea / change / refactor"` for artifact-first requirements docs feeding planning.
- `/flow-code:adr "decision"` for durable architecture decision records.
- `flow-code-deprecation` when the main documentation problem is replacement/removal guidance.

## Architecture Decision Records (ADR)

Use ADRs for decisions that are:
- Hard to reverse (database choice, auth strategy, framework)
- Non-obvious (why X instead of the "standard" Y?)
- Frequently questioned ("why did we do it this way?")

For routing, see the Command Front Doors section above. Replacement/removal notes, deprecation notices, and changelog entries can still be written directly within this documentation workflow.

### ADR Template

```markdown
# ADR-NNN: Short Title

## Status
PROPOSED | ACCEPTED | SUPERSEDED by ADR-MMM | DEPRECATED

## Date
YYYY-MM-DD

## Context
What is the situation? What forces are at play?
(Technical constraints, business requirements, team capabilities)

## Decision
What did we decide and why?

## Alternatives Considered
| Option | Pros | Cons | Why not? |
|--------|------|------|----------|
| Option A | ... | ... | ... |
| Option B (chosen) | ... | ... | N/A |

## Consequences
What becomes easier? What becomes harder?
What are the risks we're accepting?
```

**Storage:** `docs/decisions/ADR-NNN-short-title.md`
**Lifecycle:** PROPOSED → ACCEPTED → SUPERSEDED/DEPRECATED

**See also:** [ADR Template](../../references/adr-template.md) for the full template.

## README Structure

```markdown
# Project Name
One-line description of what this does.

## Quick Start
3-5 commands to go from clone to running.

## Architecture
Brief overview (link to detailed docs if complex).

## Development
How to set up the dev environment, run tests, contribute.

## API Reference
Link to generated docs or inline reference.

## Deployment
How to deploy, environment variables needed.

## License
```

**Rules:**
- Quick Start must work in < 5 minutes on a fresh machine
- Keep README under 300 lines (link to detailed docs)
- Update README when adding new setup steps or changing architecture
- Test Quick Start instructions on a fresh clone periodically

## API Documentation

### REST API

```markdown
## POST /api/orders

Create a new order.

**Request:**
```json
{
  "items": [{ "productId": "p1", "quantity": 2 }],
  "shippingAddress": { "street": "123 Main", "city": "..." }
}
```

**Response (201):**
```json
{
  "id": "order_abc123",
  "status": "pending",
  "total": 59.98,
  "createdAt": "2026-04-08T12:00:00Z"
}
```

**Errors:**
| Status | Code | When |
|--------|------|------|
| 400 | VALIDATION_ERROR | Missing required fields |
| 401 | UNAUTHORIZED | No auth token |
| 422 | OUT_OF_STOCK | Item unavailable |
```

**Rules:**
- Show request AND response examples (not just schema)
- Document ALL error responses (not just 200)
- Include authentication requirements
- Keep examples realistic (not `"foo"`, `"bar"`, `"test"`)

## Changelog

### Format (Keep a Changelog)

```markdown
# Changelog

## [1.2.0] - 2026-04-08

### Added
- OAuth login with Google (#123)
- Rate limiting on public endpoints

### Changed
- Order total calculation now includes tax

### Fixed
- Login form not showing error on invalid credentials (#456)

### Removed
- Deprecated v1 API endpoints (see replacement/removal notes)
```

**Rules:**
- Group by: Added, Changed, Fixed, Removed, Security, Deprecated
- Link to issues/PRs
- Write for users (what changed for THEM), not developers
- Update changelog in the same PR as the change

## Inline Documentation

```typescript
// Good: explains WHY, not WHAT
// We use a 30-second cache here because the recommendation service
// has a 100 req/min rate limit and we get ~50 unique requests/min
const CACHE_TTL = 30_000;

// Bad: restates the code
// Set cache TTL to 30000 milliseconds
const CACHE_TTL = 30_000;
```

**Rules:**
- Comment the WHY (business reason, constraint, non-obvious decision)
- Don't comment the WHAT (code should be self-explanatory)
- Don't comment the HOW (unless the algorithm is genuinely complex)
- Link to tickets, ADRs, or external docs for context
- Delete outdated comments (worse than no comment)

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "The code is self-documenting" | Code shows WHAT, not WHY. Decisions, trade-offs, and context need docs. |
| "Docs get stale" | Stale docs are a process problem. Update docs in the same PR as the change. |
| "Nobody reads docs" | Bad docs don't get read. Good docs save hours of questions and onboarding. |
| "We'll document it later" | Later never comes. Write the ADR when the decision is fresh, not 6 months later. |
| "README is just for open source" | Internal READMEs are even more important — they're the onboarding document for every new team member. |

## Red Flags

- README Quick Start doesn't work on a fresh machine
- No ADRs for major architectural decisions
- API docs missing error responses
- Changelog updated months after the change
- Comments explaining WHAT the code does (not WHY)
- Docs in a wiki that nobody maintains (prefer docs-as-code in repo)
- No mention of environment variables or configuration

## Verification

- [ ] README Quick Start works in < 5 minutes on fresh clone
- [ ] Major decisions have ADRs (database, auth, framework choices)
- [ ] API endpoints have request/response examples + error docs
- [ ] Changelog updated in same PR as the change
- [ ] Inline comments explain WHY, not WHAT
- [ ] Environment variables documented (`.env.example` or README)
- [ ] No outdated comments or docs that contradict current code
