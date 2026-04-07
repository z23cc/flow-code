---
name: flow-code-api-design
description: "Use when designing or modifying APIs, module boundaries, CLI interfaces, library public surfaces, RPC contracts, or event schemas"
tier: 3
---

# API and Interface Design

## Overview

Contract-first interface design: define the interface before implementing it, then enforce it at boundaries. Applicable to REST APIs, CLI tools, library APIs, RPC services, event schemas, and module boundaries. Good interfaces make the right thing easy and the wrong thing hard.

## When to Use

- Designing a new API, CLI command, library public surface, or RPC contract
- Changing an existing public interface (adding fields, modifying behavior)
- Defining cross-team or cross-module contracts
- Establishing event schemas or message formats
- Creating module boundaries within a codebase

**When NOT to use:**
- Internal implementation details behind a stable interface
- Private functions within a module
- Refactoring internals without changing the public surface
- If debugging an API issue, use the `flow-code-debug` skill instead

## Core Process

### Phase 1: Define the Contract First

Design the interface before writing any implementation. The contract is the spec.

1. **Identify consumers** -- who calls this interface? Other services, CLI users, library consumers, event subscribers?
2. **Define input/output shapes** -- what goes in, what comes out. Be explicit about types, optionality, and defaults.
3. **Specify error semantics** -- every interface has failure modes. Define them upfront.

```
Example contract (stack-agnostic pseudocode):

  interface TaskStore:
    create(input: CreateTaskInput) -> Task | ValidationError
    get(id: string) -> Task | NotFoundError
    list(filter: Filter, page: Page) -> PaginatedResult<Task>
    update(id: string, patch: Partial<Task>) -> Task | NotFoundError | ValidationError
    delete(id: string) -> void  // idempotent: succeeds even if already deleted
```

For CLI tools, the contract is the command signature:
```bash
# Contract: flowctl task create --title <str> [--domain <str>] [--files <paths>]
# Output: JSON with { id, title, status } on success
# Exit code: 0 success, 1 validation error, 2 not found
```

For event schemas:
```
Event: task.completed
  payload: { task_id: string, completed_at: timestamp, duration_seconds: int }
  guarantees: at-least-once delivery, idempotent consumers expected
```

### Phase 2: Apply Hyrum's Law Awareness

> With a sufficient number of users, all observable behaviors become de facto contracts.

Every public behavior -- including undocumented quirks, error message text, ordering, and timing -- becomes a commitment once consumers depend on it.

Design implications:
- Be intentional about what you expose. If consumers can observe it, they will depend on it.
- Do not leak implementation details through the interface (internal IDs, database structure, stack traces).
- Treat error message formats as part of the contract -- consumers parse them.
- For CLI tools: exit codes, output format (JSON vs text), and flag names are all contract surface.

### Phase 3: Design Error Semantics

Pick one error strategy and use it consistently across the entire interface:

**Structured errors (recommended):**
```
Error shape (any transport):
  code: string      // Machine-readable: "VALIDATION_ERROR", "NOT_FOUND"
  message: string   // Human-readable: "Title is required"
  details?: any     // Additional context when helpful

Transport mapping:
  REST:  code -> HTTP status (400, 404, 409, 422, 500)
  CLI:   code -> exit code + JSON stderr
  RPC:   code -> status enum + metadata
  Event: code -> dead-letter reason
```

Do not mix patterns. If some operations throw, others return null, and others return `{ error }` -- consumers cannot predict behavior.

### Phase 4: Validate at Boundaries

Trust internal code. Validate where external input enters the system:

**Where validation belongs:**
- API route handlers (user input)
- CLI argument parsing (user input)
- External service response parsing (third-party data -- always untrusted)
- Event/message deserialization (cross-service data)
- Environment variable loading (configuration)

**Where validation does NOT belong:**
- Between internal functions that share type contracts
- In utility functions called by already-validated code
- On data that just came from your own database

### Phase 5: Plan for Evolution

Design interfaces that can grow without breaking consumers:

1. **Prefer addition over modification** -- new optional fields, new endpoints/commands, new event types.
2. **Never remove or change existing fields** -- every consumer becomes a constraint.
3. **Version when breaking changes are unavoidable** -- but prefer extension first.
4. **Use the One-Version Rule** -- avoid forcing consumers to choose between multiple versions.

For CLI tools: new flags default to off, new subcommands are additive, output format changes require `--format` flags.

### Phase 6: Design for Idempotency

Idempotency applies beyond HTTP -- CLI operations, event handlers, and RPC calls all benefit:

- **DELETE/remove operations**: succeed even if resource already gone.
- **CLI operations**: `flowctl lock --task T1 --files f.py` is safe to call twice.
- **Event handlers**: processing the same event twice produces the same result.
- **Create with client ID**: client provides an idempotency key; server deduplicates.

```
Idempotency decision:
  Is the operation naturally idempotent (GET, PUT, DELETE)? -> No extra work.
  Is it a create/mutation? -> Require idempotency key or make it upsert.
  Is it an event handler? -> Track processed event IDs or make handler pure.
```

### Phase 7: Document the Contract

The contract must be committed alongside the implementation:

- Type definitions or schema files (protobuf, JSON Schema, OpenAPI, CLI --help)
- Error code catalog (all possible error codes with descriptions)
- Examples of success and failure responses
- Migration notes for any changes to existing interfaces

Reference `references/code-review-checklist.md` for review patterns -- particularly the Architecture and Correctness sections when reviewing API changes.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "I'll design the API after implementation" | API shapes emerge from implementation but don't match consumer needs. Contract-first catches mismatches before code exists. |
| "Internal APIs don't need contracts" | Hyrum's Law applies to internal APIs too. Internal consumers are still consumers -- contracts prevent coupling and enable parallel work. |
| "We can always change it later" | Every consumer becomes a constraint. The cost of change grows exponentially with adoption. |
| "The code is the documentation" | Consumers shouldn't need to read implementation to use your API. The contract is the interface, not the code behind it. |
| "Edge cases can wait" | Edge cases in APIs become permanent undefined behavior. Consumers will discover them and depend on whatever happens. |
| "We don't need error codes yet" | Without structured errors from day one, consumers parse message strings. Changing error format later breaks every consumer. |
| "This CLI is just for internal use" | Internal CLI tools accumulate scripts that depend on exact output format. Same contract discipline applies. |
| "Versioning is overkill" | Breaking changes without versioning break consumers silently. Design for extension from the start. |

## Red Flags

- Interface returns different shapes depending on conditions (inconsistent contract)
- Error formats differ across operations in the same API
- Validation scattered throughout internal code instead of at boundaries
- Breaking changes to existing fields (type changes, removals, renamed flags)
- List/search operations without pagination or size limits
- CLI commands with unparseable free-text output instead of structured (JSON) output
- No idempotency story for mutating operations
- Implementation details leaking through the interface (database IDs, internal error traces)
- Contract defined only in code comments, not in types or schema files

## Verification

After designing or modifying an interface:

- [ ] Contract defined before implementation (types, schema, or CLI signature committed first)
- [ ] Every operation has explicit input and output shapes with typed errors
- [ ] Error responses follow a single consistent format across all operations
- [ ] Validation happens at system boundaries only (not scattered internally)
- [ ] New fields/flags are additive and optional (backward compatible)
- [ ] Mutating operations have an idempotency strategy
- [ ] Contract documentation committed alongside implementation
- [ ] Reviewed against `references/code-review-checklist.md` Architecture and Correctness sections
