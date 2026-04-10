---
name: flow-code-deprecation
description: "Use when removing, replacing, or sunsetting features, APIs, modules, or dependencies in any codebase"
tier: 3
user-invocable: true
---

# Deprecation, Replacement, and Removal

## Overview

Code is a liability, not an asset — every line carries maintenance cost, support cost, and future drag. This skill helps you decide whether an old path should be replaced or deleted, assess the blast radius, and remove it responsibly.

**Default stance:** prefer direct replacement or direct removal when the old thing is no longer worth keeping. Use a short temporary bridge only when real consumers or runtime risk make one-pass removal unsafe.

## When to Use

- Replacing an old system, API, library, or module with a better one
- Removing a feature, CLI command, flag, config option, or code path that no longer earns its keep
- Consolidating duplicate implementations into a single supported path
- Cleaning up zombie code that still has consumers but no clear owner
- Deciding whether a legacy path should be kept, replaced, or removed

## Command Entry Points

- This skill is the front door when the dominant question is how to replace or remove an old surface safely.
- Use `/flow-code:spec` when you need a reusable requirements artifact before planning.
- Use `/flow-code:adr` when the change requires explicit architectural decisions.
- Use `/flow-code:plan` after the scope, consumers, and success criteria are clear.

**When NOT to use:**
- Routine refactoring that doesn't remove public interfaces
- Adding new features while keeping the old path indefinitely
- Version bumps that don't change or remove behavior
- Bug fixes in legacy code you still intend to maintain

## The Removal Decision

Before you deprecate anything, answer these questions:

```
1. Does this thing still provide unique value?
   → If yes, keep or improve it. If no, continue.

2. Who consumes it today?
   → Quantify with search, dependency graphs, logs, metrics, or config references.

3. Is a replacement actually needed?
   → If no, plan direct removal.
   → If yes, make sure the replacement is real enough to move remaining consumers.

4. Can the consumers be updated directly?
   → If yes, do that instead of inventing extra temporary machinery.

5. What breaks if we remove it now?
   → List concrete consumers, workflows, contracts, and edge behaviors.

6. What is the cost of keeping it?
   → Maintenance burden, security risk, dependency rot, cognitive load, slower delivery.
```

Proceed when the old path no longer earns its keep, the consumer impact is understood, and you have a concrete way to update or remove the remaining usage.

## If You Need a Short Bridge

Use a temporary bridge only when at least one of these is true:

- external consumers cannot all be changed in one pass;
- runtime risk is high enough that a brief staged rollout materially reduces danger;
- the old interface must remain briefly while you land an automated consumer update.

If none of those apply, remove or replace the old path directly.

When you do need a bridge, keep it narrow:
- define exactly what stays working temporarily;
- set an explicit owner and removal trigger/date;
- keep the instructions short and concrete;
- do not let the temporary path become a second supported surface.

## Core Process

### Phase 1: Assess Blast Radius

1. **Inventory consumers** — imports, API calls, config keys, CLI usage, docs, automation, and tests.
2. **Map critical behaviors** — not just the happy path; note observable quirks consumers may rely on.
3. **Classify consumers** — owned/internal, adjacent teams, or external/public.
4. **Document the assessment:**
   ```
   Component: <name>
   Decision: replace | remove
   Consumers: <count and list>
   Replacement: <name or "none; feature removed">
   Temporary bridge: none | brief | required
   Main risk: <specific blast-radius concern>
   Verification: <tests, logs, usage check>
   ```

### Phase 2: Prepare the Target State

1. **Build or validate the replacement** if one is needed.
2. **Cover the critical use cases** consumers actually rely on.
3. **Choose the update shape** — direct edit, codemod, brief shim, or staged rollout.
4. **Write only the docs you need** — usually a short removal/replacement note plus concrete update steps if humans must act.

**Do not turn this into extra program management.** The goal is to make the old path disappear safely with the least temporary machinery needed.

### Phase 3: Update Consumers

For each active consumer:

1. **Change the call sites or integrations** to the replacement, or remove the dependency entirely.
2. **Automate churn when possible** — codemods, scripts, mechanical edits.
3. **Verify behavior** with tests, integration checks, or focused manual validation.
4. **Remove the old references immediately** once the consumer is updated.

If you own the deprecated surface and the consumer updates are mechanical, do the updates yourself instead of pushing that burden downstream.

### Phase 4: Remove the Old Path

Once active usage is handled:

1. **Delete the implementation** — code, exports, types, config, flags.
2. **Delete transitional scaffolding** — warnings, adapters, temporary shims, extra branches.
3. **Delete stale docs and examples** that teach the removed path.
4. **Run verification:**
   ```bash
   $FLOWCTL guard
   ```

### Phase 5: Verify Clean Removal

1. **Re-run consumer detection** — use the same search, dependency, log, or metric checks from Phase 1 to confirm the old path is truly unused.
2. **Search for orphaned references** — stale imports, docs, scripts, config keys, and links.
3. **Run tests/build checks** relevant to the removed path.
4. **Confirm user-facing docs are coherent** — changelog, README, command docs, upgrade notes if needed.
5. **Confirm no accidental long-term support commitment remains** — no permanent deprecation banners for code that should now be gone.

For multi-task work, start with `/flow-code:spec` when you need a durable scope-and-requirements artifact; then use `/flow-code:plan` when you need DAG-level breakdown and execution orchestration.

## Removal Patterns

Use the smallest pattern that gets you safely to removal.

### Direct Replacement or Removal
Update consumers and delete the old path in the same change or release window.

**Default choice** when consumers are owned, mechanical, or limited.

### Brief Shim
Keep a thin shim around the new implementation for a short, explicit window.

**Use when** consumers cannot all move at once, but the end state is still removal.

### Parallel Rollout
Run old and new in parallel while traffic or workloads move incrementally.

**Use when** runtime risk is high enough to justify the extra complexity. Remove the old path as soon as traffic reaches zero.

## Zombie Code

Code that nobody wants to own but somebody still depends on.

Signs:
- no meaningful maintenance for months;
- no clear owner;
- failing or flaky tests nobody fixes;
- vulnerable or stale dependencies nobody updates;
- consumers still exist, often undocumented.

**Response:** assess consumers, choose replacement or removal, and drive it to completion. Do not leave zombie code in permanent limbo just because cleanup is inconvenient.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "It still works, why remove it?" | Working but unwanted code still costs maintenance, security attention, and cognitive load. |
| "Someone might need it later" | "Maybe" is not a reason to keep paying for it now. Rebuild later if it truly matters. |
| "We should keep both paths just in case" | A temporary bridge is a tool, not a virtue. Keep it only when concrete consumers require it. |
| "Let's deprecate it first and decide later" | Deprecation without a removal or replacement path becomes bureaucracy and drift. |
| "Consumers will update on their own" | If the change is important, drive the update with tooling, direct edits, or explicit instructions. |
| "The cleanup is too expensive" | Compare that cost with years of carrying duplicate paths, docs, tests, and support burden. |
| "We can support both forever" | Two paths usually mean duplicated maintenance and slower change everywhere. |
| "We need a large coordination effort" | Most cleanup work needs impact analysis and execution, not extra ceremony. |

## Red Flags

- No inventory of actual consumers before starting removal
- Long-lived temporary bridge with no concrete removal owner or trigger
- Keeping the old path because removal feels socially harder than deciding
- Building a replacement that never becomes the default
- Removing code without checking logs, search results, or dependency edges first
- Leaving temporary adapters, flags, or warnings in place indefinitely
- Updating docs to say "deprecated" but never actually reducing the old surface

## Verification

After completing a deprecation/removal cycle, confirm:

- [ ] Decision documented: replace or remove, with consumer count and main risk
- [ ] Replacement exists and is validated when one is needed
- [ ] Active consumers were updated, removed, or explicitly accounted for
- [ ] Consumer inventory was re-checked with search/logs/metrics as appropriate
- [ ] Any temporary shim or bridge is narrow, temporary, and tracked for deletion
- [ ] Old code, exports, tests, docs, and config were fully removed when intended
- [ ] No orphaned references to the deprecated path remain
- [ ] `$FLOWCTL guard` passes after removal
- [ ] No leftover deprecation bureaucracy remains for code that should be gone
