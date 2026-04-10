---
name: flow-code-adr
description: "Use when you want to capture, refine, or update an Architecture Decision Record. Triggers on /flow-code:adr."
tier: 2
user-invocable: true
---

# ADR Mode

Capture or update an Architecture Decision Record (ADR) as a first-class workflow.

Use this when the decision is:
- hard to reverse;
- non-obvious or frequently questioned;
- likely to constrain future planning or major changes;
- something `/flow-code:plan` and future agents should be able to cite explicitly.

Canonical RP/MCP orchestration guidance lives in `skills/_shared/rp-mcp-orchestration.md`.

## Use this instead of...

- **`flow-code-documentation`** when you want a dedicated ADR workflow, not generic docs guidance
- **`/flow-code:spec`** when you need requirements for a change, not a durable architectural decision record
- **free-form docs editing** when the decision deserves history, trade-offs, and consequences

## Output target

Choose the smallest sensible target:

- **Existing ADR path** → refine in place
- **Otherwise** → create `docs/decisions/ADR-NNN-short-title.md`

When creating a new ADR:
1. scan existing `docs/decisions/ADR-*.md`;
2. increment the highest number;
3. keep zero-padded numbering.

## Workflow

1. **Extract the real decision** from the request.
2. **Check whether an ADR is warranted**. If not, say so and redirect to the better surface.
3. **Orient briefly**: read related ADRs, `CLAUDE.md`, `.flow/project-context.md`, and do 1-3 targeted searches in the affected area.
4. **Use `context_builder(response_type="question")` by default** to ground the decision in current architecture, constraints, and alternatives.
5. **Write or update the ADR directly** using the repo template in `references/adr-template.md`.
6. **Call out supersession explicitly** if this decision replaces an older ADR.

## Required output shape

```markdown
# ADR-NNN: <Short title>

## Status
Proposed | Accepted | Superseded by ADR-XXX | Deprecated

## Date
YYYY-MM-DD

## Context
<the forces, constraints, and why this decision exists now>

## Decision
<the chosen direction and why>

## Alternatives Considered

### <Alternative A>
- **Pros:** ...
- **Cons:** ...
- **Rejected because:** ...

### <Alternative B>
- **Pros:** ...
- **Cons:** ...
- **Rejected because:** ...

## Consequences
- <what becomes easier>
- <what becomes harder>
- <follow-up obligations, downstream changes, or replacement/removal effects>
```

## Rules

- ADRs record **why this decision exists**, not task-level implementation plans.
- Prefer **Proposed** when the team has not fully committed yet.
- Never delete historical ADRs; supersede them.
- If the decision has notable downstream replacement/removal impact, capture that in **Consequences** and reference the follow-up spec or plan if one exists.
- Keep the ADR grounded in actual repo constraints and existing patterns.

## Builder prompt shape

```json
{"tool":"context_builder","args":{
  "instructions":"<task>Analyze the architecture decision around: <actual decision>.</task>\n<context>Focus on current constraints, existing patterns, realistic alternatives, and consequences. I need an ADR, not an implementation plan.</context>",
  "response_type":"question"
}}
```

## Anti-patterns

- Writing a task plan instead of a decision record
- Creating an ADR for something trivial or easily reversible
- Rewriting history instead of superseding an older ADR
- Omitting alternatives or consequences
- Capturing a high-impact decision without noting downstream consequences
