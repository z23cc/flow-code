---
name: flow-code-deprecation
description: "Use when removing, replacing, or sunsetting features, APIs, modules, or dependencies in any codebase"
tier: 3
user-invocable: true
---

# Deprecation and Migration

## Overview

Code is a liability, not an asset — every line carries ongoing maintenance cost. This skill enforces a disciplined process for deprecating and removing code that no longer earns its keep, ensuring consumers are migrated safely before anything is removed.

## When to Use

- Replacing an old system, API, library, or module with a new one
- Sunsetting a feature, CLI command, or config option that's no longer needed
- Consolidating duplicate implementations into a single path
- Removing dead code that nobody owns but everybody depends on
- Planning the lifecycle of a new system (deprecation planning starts at design time)
- Deciding whether to maintain a legacy system or invest in migration

**When NOT to use:**
- Routine refactoring that doesn't remove public interfaces — use standard refactoring
- Adding new features alongside old ones without removing anything
- Version bumps that don't change or remove behavior
- Bug fixes in legacy code you intend to keep

## The Deprecation Decision

Before deprecating anything, answer these questions:

```
1. Does this system still provide unique value?
   → If yes, maintain it. If no, proceed.

2. How many consumers depend on it?
   → Quantify: grep for imports, check API call logs, review dependency graphs.

3. Does a replacement exist?
   → If no, BUILD THE REPLACEMENT FIRST. Never deprecate without an alternative.

4. What's the migration cost per consumer?
   → If automatable (codemod, sed, script), do it yourself (the Churn Rule).
   → If manual and high-effort, weigh against ongoing maintenance cost.

5. What's the cost of NOT deprecating?
   → Security risk, engineer time, dependency rot, onboarding friction.
```

If answers 1=no, 3=yes, and 5 > 4, proceed with deprecation.

## Advisory vs Compulsory Deprecation

| Type | When to Use | Mechanism |
|------|-------------|-----------|
| **Advisory** | Old system is stable, migration is optional | Warnings, documentation, nudges. Consumers migrate on their own timeline. |
| **Compulsory** | Security risk, blocks progress, or maintenance cost is unsustainable | Hard deadline. Old system removed by date X. Provide migration tooling. |

**Default to advisory.** Use compulsory only when risk or cost justifies forcing migration. Compulsory deprecation requires providing migration tooling, documentation, and support — you cannot just announce a deadline.

## Core Process

### Phase 1: Assess Impact

1. **Inventory all consumers** — grep for imports, API calls, config references, CLI invocations. Miss nothing.
2. **Map the dependency graph** — direct consumers and transitive dependents. Hyrum's Law: with enough users, every observable behavior becomes depended on, including bugs and timing quirks.
3. **Quantify maintenance cost** — security vulnerabilities, test failures, onboarding friction, dependency update burden.
4. **Document the assessment:**
   ```
   Deprecated: <component name>
   Consumers: <count and list>
   Maintenance cost: <specific burden>
   Replacement: <name or "to be built">
   Migration type: advisory | compulsory
   ```

### Phase 2: Build the Replacement

**Do NOT announce deprecation until the replacement is production-proven.**

1. **Cover all critical use cases** of the old system
2. **Write a migration guide** with concrete steps and examples
3. **Prove it in production** — not just "theoretically better"
4. **Verify behavioral parity** on edge cases consumers depend on

### Phase 3: Announce and Document

Create a deprecation notice:

```
## Deprecation Notice: <ComponentName>

Status: Deprecated as of <date>
Replacement: <NewComponent> (see migration guide)
Removal date: Advisory — no hard deadline | Compulsory — <date>
Reason: <specific maintenance burden or risk>

### Migration Steps
1. Replace <old import/call> with <new import/call>
2. Update configuration (see examples)
3. Run migration verification: <command>
```

Place notices where consumers will see them: inline warnings, changelogs, documentation headers.

### Phase 4: Migrate Consumers

For each consumer:

1. **Identify all touchpoints** with the deprecated system
2. **Update to the replacement** — provide codemods or scripts where possible
3. **Verify behavior matches** — tests, integration checks, manual validation
4. **Remove references** to the old system
5. **Confirm no regressions**

**The Churn Rule:** If you own the infrastructure being deprecated, you are responsible for migrating your users — or providing backward-compatible shims. Do not announce deprecation and leave users to figure it out.

### Phase 5: Remove Old Code

Only after all consumers have migrated:

1. **Verify zero active usage** — metrics, logs, dependency analysis, grep
2. **Remove the code** — implementation, types, exports
3. **Remove associated artifacts** — tests, documentation, configuration, feature flags
4. **Remove deprecation notices** — they served their purpose
5. **Run full verification:**
   ```bash
   $FLOWCTL guard
   ```

### Phase 6: Verify Clean Removal

1. **Search for orphaned references** — stale imports, dead config keys, broken links in docs
2. **Run the full test suite** — no failures from missing code
3. **Check build artifacts** — no phantom exports or dangling symbols
4. **Confirm documentation is updated** — no references to removed components

For planning migration work across multiple tasks, use `/flow-code:plan`.

## Migration Patterns

### Strangler Pattern
Run old and new in parallel. Route traffic incrementally from old to new. Remove old when it handles 0%.

### Adapter Pattern
Wrap old interface around new implementation. Consumers keep using the old interface while the backend migrates underneath.

### Feature Flag Migration
Use feature flags to switch consumers from old to new one at a time. Roll back instantly if issues arise.

## Zombie Code

Code that nobody owns but everybody depends on. Signs:
- No commits in 6+ months but active consumers exist
- No assigned maintainer
- Failing tests nobody fixes
- Dependencies with known vulnerabilities nobody updates

**Response:** Either assign an owner and maintain it, or deprecate it with a concrete migration plan. Zombie code cannot stay in limbo.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "It still works, why remove it?" | Working code without maintenance accumulates security debt and complexity. Cost grows silently until it's a crisis. |
| "Someone might need it later" | If needed later, it can be rebuilt with better design. Keeping unused code "just in case" costs more than rebuilding. |
| "No one uses this anymore" | Did you verify that with grep, metrics, and logs? Undocumented consumers are the norm, not the exception. |
| "We can remove it later" | Later never comes. Every month you delay, more consumers may adopt the deprecated path. |
| "Breaking changes are fine for a major version" | A major version bump does not excuse removing things without migration paths. Semver is a signal, not a license to break users. |
| "The migration is too expensive" | Compare migration cost to 2-3 years of ongoing maintenance. Migration is almost always cheaper long-term. |
| "Users will migrate on their own" | They won't. Provide tooling, documentation, and support — or do the migration yourself (the Churn Rule). |
| "We can maintain both indefinitely" | Two systems doing the same thing means double the maintenance, testing, documentation, and onboarding cost. |

## Red Flags

- Deprecating a system with no replacement available or proven
- Deprecation announcement with no migration guide, tooling, or timeline
- "Soft" deprecation that has been advisory for months or years with no progress
- Removing code without verifying zero active consumers first
- New features added to a system already marked deprecated (invest in the replacement instead)
- Zombie code with no owner and active consumers left in limbo
- Deprecation without quantifying current usage

## Verification

After completing a deprecation cycle, confirm:

- [ ] Deprecation decision documented with consumer count, maintenance cost, and replacement name
- [ ] Replacement is production-proven and covers all critical use cases
- [ ] Migration guide exists with concrete steps, examples, and verification commands
- [ ] All active consumers migrated (verified by grep, metrics, or logs — not assumption)
- [ ] Old code, tests, documentation, and configuration fully removed
- [ ] No orphaned references to the deprecated system remain in the codebase
- [ ] `$FLOWCTL guard` passes after removal
- [ ] Deprecation notices removed (they served their purpose)
