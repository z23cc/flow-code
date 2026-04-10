---
name: flow-code-spec
description: "Use when you want a reusable requirements-first spec before planning or implementation. Triggers on /flow-code:spec."
tier: 2
user-invocable: true
---

# Spec Mode

Create or refine a reusable requirements-first spec that feeds cleanly into `/flow-code:plan`.

This is the explicit entrypoint for:
- new feature specs;
- replacement / removal requirements when a written contract helps;
- refactor scopes that need a written contract before planning;
- turning a rough idea into a durable `.md` artifact without creating an epic yet.

Canonical RP/MCP orchestration guidance lives in `skills/_shared/rp-mcp-orchestration.md`.

## Use this instead of...

- **`/flow-code:brainstorm`** when you want a written spec artifact more than a broad pressure-test conversation
- **`/flow-code:interview`** when you are not refining an existing epic/task/file through a long interactive interview
- **`/flow-code:plan`** when the requirements are still too vague to plan confidently

## Output target

Choose the smallest sensible target:

- **Existing markdown file path** → refine in place
- **`--output <path>` provided** → write there
- **Otherwise** → write to `.flow/specs/<slug>-requirements.md`

Defaulting to `.flow/specs/` keeps the output first-class for `/flow-code:plan`, which already consumes requirements docs from that location.

## Workflow

1. **Extract the actual problem** from the request. Do not keep meta-framing like “write a spec for...”.
2. **Orient briefly**: read `CLAUDE.md`, `.flow/project-context.md` if present, relevant ADRs if they exist, and do 1-3 targeted searches.
3. **Use `context_builder(response_type="question")` by default** for multi-file, architectural, or otherwise non-local scope.
4. **Write the spec directly** as markdown. Use Oracle follow-up only if a requirements trade-off remains unresolved inside the selected context.
5. **End with the next step**: usually `/flow-code:plan <spec-path>`.

## Required output shape

When writing a new requirements doc, use this structure so planning can consume it cleanly:

```markdown
# Requirements: <Title>

## Problem
<what is changing and why now>

## Users / Actors
<who is affected>

## Chosen Approach
<the recommended direction at a product/architecture level, not implementation steps>

## Requirements
- [ ] <testable requirement 1>
- [ ] <testable requirement 2>

## Non-Goals
- <explicitly out of scope>

## Constraints
- <technical, business, integration, or rollout constraints>

## Transition / Removal Notes
- <affected interfaces, consumers, or data concerns if relevant>
- <interface transition, replacement/removal, or cleanup requirements if relevant>

## Evidence
- `path/to/file` — <existing pattern, dependency, or constraint this spec should respect>

## Open Questions
- <unresolved item for planning research, or `None`>
```

## Rules

- Specs describe **what / why / scope**, not implementation code.
- Include explicit **non-goals** so planning does not gold-plate.
- Turn replacement/removal work into explicit requirements, scope boundaries, or open questions.
- Prefer a repo-local markdown artifact over an ephemeral conversation-only answer.
- If the request is already well served by an existing spec file, refine that file instead of spawning a parallel document.

## Builder prompt shape

```json
{"tool":"context_builder","args":{
  "instructions":"<task>Draft a requirements-first spec for: <actual task>.</task>\n<context>Focus on goals, scope, constraints, affected consumers, and acceptance-style requirements. Avoid implementation-level task breakdown.</context>",
  "response_type":"question"
}}
```

## Anti-patterns

- Writing implementation steps instead of requirements
- Creating a second spec when an existing file should be refined in place
- Skipping affected-consumer or transition requirements for replacement/removal changes
- Going straight to `/flow-code:plan` when scope is still ambiguous
- Treating this as a full project plan rather than a requirements artifact
