---
name: flow-code-simplify
description: "Use when reducing code complexity, removing dead code, refactoring for clarity, or when auto-improve targets simplification. Preserves behavior exactly while improving readability."
tier: 2
user-invocable: true
---
<!-- SKILL_TAGS: refactoring,simplification,quality,complexity -->

# Code Simplification

> **Startup:** Follow [Startup Sequence](../_shared/preamble.md) before proceeding.

## flowctl Setup

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Overview

Reduce complexity while preserving exact behavior. The goal is code that is easier to read, modify, and maintain — not code that is clever. Simplification is not optimization — it's a readability and maintenance investment.

## When to Use

- Code review found complexity issues (deep nesting, long functions)
- Auto-improve identified simplification targets
- Preparing code for a major feature (simplify before extending)
- Technical debt cleanup sprints
- Post-merge cleanup of rushed code

**When NOT to use:**
- Code you don't understand yet (understand first, then simplify)
- Performance-critical paths (profile first, simplify second)
- Generated code (regenerate, don't refactor)

## Core Principles

### 1. Chesterton's Fence

**Before removing or changing anything, understand WHY it exists.**

```
"Don't remove a fence until you know why it was built."
```

- Read git blame for the code you want to change
- Check if there's a comment explaining a non-obvious reason
- Look for related tests that might reveal edge cases
- If you can't find the reason, ASK before removing

### 2. Preserve Behavior Exactly

- Tests must pass unchanged after simplification
- If tests don't exist, write them BEFORE simplifying
- Run `flowctl guard` before AND after — diffs should be zero
- No behavior changes hidden inside a "simplification"

### 3. Prefer Clarity Over Cleverness

```typescript
// Clever (bad)
const r = d.reduce((a, x) => (x.s === 'active' ? [...a, x.n] : a), []);

// Clear (good)
const activeNames = data
  .filter(item => item.status === 'active')
  .map(item => item.name);
```

### 4. Scope to What Changed

Only simplify code related to your current task. Don't go on a cleanup spree across unrelated files.

## The Simplification Process

### Step 1: Understand Before Touching

1. Read the code and its tests
2. Check git blame — understand history
3. Identify what the code DOES (not what you think it should do)
4. If no tests exist, write characterization tests first

### Step 2: Identify Opportunities

Scan for these patterns:

**Structural Complexity:**
- Functions >40 lines → extract named functions
- Nesting >3 levels deep → early returns, guard clauses
- Classes >300 lines → split by responsibility
- Files with multiple unrelated exports → split into modules

**Naming:**
- Single-letter variables (except `i`, `j` in short loops)
- Generic names: `data`, `info`, `process`, `handle`, `manager`
- Boolean names that don't read as questions: `flag` → `isActive`

**Redundancy:**
- Dead code (unreachable branches, unused functions)
- Commented-out code (delete it — git has history)
- Duplicate logic (extract shared function)
- Unnecessary abstractions (class with one method → function)

**Control Flow:**
- Nested if/else → guard clauses with early return
- Switch with many cases → lookup table or polymorphism
- Flag parameters → separate functions
- Nested ternaries → if/else or lookup

### Step 3: Apply Changes Incrementally

One simplification at a time. After each:
1. Run tests → must pass
2. Commit if clean
3. Move to next simplification

Never batch multiple simplifications in one commit.

### Step 4: Verify

- All tests pass (unchanged from before)
- `flowctl guard` clean
- Behavior is identical (no hidden changes)
- Code is genuinely simpler (shorter, clearer, fewer branches)

## Simplification Patterns

### Early Return (Flatten Nesting)

```typescript
// Before: 3 levels deep
function processOrder(order) {
  if (order) {
    if (order.items.length > 0) {
      if (order.status === 'pending') {
        // actual logic here
      }
    }
  }
}

// After: flat
function processOrder(order) {
  if (!order) return;
  if (order.items.length === 0) return;
  if (order.status !== 'pending') return;
  // actual logic here
}
```

### Extract Named Function

```typescript
// Before: inline complex logic
const eligible = users.filter(u =>
  u.age >= 18 && u.verified && !u.banned && u.subscription !== 'expired'
);

// After: named predicate
function isEligible(user: User): boolean {
  return user.age >= 18 && user.verified && !user.banned && user.subscription !== 'expired';
}
const eligible = users.filter(isEligible);
```

### Lookup Table (Replace Switch/If Chain)

```typescript
// Before: long switch
function getStatusColor(status: string): string {
  switch (status) {
    case 'active': return 'green';
    case 'pending': return 'yellow';
    case 'error': return 'red';
    case 'disabled': return 'gray';
    default: return 'gray';
  }
}

// After: lookup
const STATUS_COLORS: Record<string, string> = {
  active: 'green', pending: 'yellow', error: 'red', disabled: 'gray',
};
function getStatusColor(status: string): string {
  return STATUS_COLORS[status] ?? 'gray';
}
```

### Remove Dead Code

- Unused imports → delete
- Unreachable branches → delete
- Commented-out code → delete (git has history)
- Functions with zero callers → delete
- Feature flags for shipped features → delete flag, keep the code

### Collapse Unnecessary Abstraction

```typescript
// Before: class with one method
class EmailValidator {
  validate(email: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
  }
}

// After: plain function
function isValidEmail(email: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email);
}
```

## Common Rationalizations

| Rationalization | Reality |
|---|---|
| "I don't know why this code exists, but I'll remove it" | Chesterton's Fence. Understand before removing. |
| "I'll simplify the whole module while I'm here" | Scope creep. Simplify what's related to your task. |
| "This refactor is too small to commit separately" | Small commits are revertable. Batch commits are not. |
| "The tests are passing so the behavior is preserved" | Tests only cover what they cover. Check edge cases manually. |
| "This clever one-liner is more concise" | Concise is not simple. Readable code > short code. |

## Red Flags

- Simplification that changes test assertions (behavior change, not simplification)
- Removing code without checking git blame
- Batching multiple simplifications in one commit
- "Simplifying" by adding abstractions (more indirection ≠ simpler)
- Renaming public APIs without updating all callers
- Deleting "unused" code that's actually used via reflection or dynamic imports

## Verification

After simplification:

- [ ] All tests pass unchanged (same assertions, same expectations)
- [ ] `flowctl guard` clean (lint + type + test)
- [ ] Git blame checked for removed/changed code
- [ ] Each simplification in its own commit
- [ ] No behavior changes (only structural/readability improvements)
- [ ] File is genuinely simpler (fewer lines, fewer branches, clearer names)
- [ ] No unnecessary abstractions introduced
