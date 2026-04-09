# Step 4: Approach Generation & Scoring

## Generate 3 Distinct Approaches

Each approach MUST be genuinely different — not just "same thing with more/less scope". One must be minimal, one must be ideal.

### Required Approaches:

**Approach A: Narrowest Wedge** — The smallest change that unblocks the core use case.
- Fewest files touched, smallest diff
- Ships in hours, not days
- Deliberately leaves things out

**Approach B: Balanced** — Best tradeoff of completeness vs effort.
- Covers main use cases + key edge cases
- Reasonable timeline
- Reuses existing patterns

**Approach C: Ideal Architecture** — Best long-term solution if time were unlimited.
- Complete coverage
- Proper abstractions
- Full test coverage
- May require larger refactoring

### Table Format (all 3 approaches):

| Field | Approach A (Narrow) | Approach B (Balanced) | Approach C (Ideal) |
|-------|--------------------|-----------------------|-------------------|
| **Summary** | One sentence | One sentence | One sentence |
| **Files** | List affected files | List affected files | List affected files |
| **Effort** | S / M / L | S / M / L | S / M / L |
| **Risk** | Low / Med / High | Low / Med / High | Low / Med / High |
| **Reuse** | What existing code used | What existing code used | What existing code used |
| **Pros** | 2-3 bullets | 2-3 bullets | 2-3 bullets |
| **Cons** | 2-3 bullets | 2-3 bullets | 2-3 bullets |

---

## Score Each Approach (1-5 per dimension)

| Dimension | A (Narrow) | B (Balanced) | C (Ideal) | Weight |
|-----------|-----------|-------------|-----------|--------|
| **Completeness** — solves the full problem? | ? | ? | ? | 3x |
| **Code alignment** — fits existing patterns? | ? | ? | ? | 2x |
| **Risk** — what can go wrong? (5=low risk) | ? | ? | ? | 2x |
| **Effort** — how much work? (5=least effort) | ? | ? | ? | 1x |
| **Maintainability** — easy to extend/change later? | ? | ? | ? | 2x |
| **Testability** — can it be verified? | ? | ? | ? | 1x |
| **Weighted Total** | ?/55 | ?/55 | ?/55 | |

---

## Select Best Approach

### Auto Mode (AUTO_MODE=true)

Pick the approach with the **highest weighted score**.

If scores are within 5 points:
- Prefer higher **Completeness** score (most comprehensive wins)
- If still tied, prefer higher **Maintainability** (long-term wins)

Output:
```
**Selected: Approach X — score Y/55**
Reason: [one line citing the scoring advantage]
Runner-up: Approach Z (score W/55) — [one line on what it trades off]
```

### Interactive Mode (AUTO_MODE=false)

Show the scoring table, then ask via `AskUserQuestion`:
> "Approach B scored highest (42/55). Do you want to go with B, or prefer a different approach? (A/B/C/combine)"

---

## Validate Selection Against Self-Interview

After selection, cross-check:
1. Does the selected approach address ALL failure causes from Q6 (Pre-mortem)?
2. Does it respect the Narrowest Wedge from Q3? (Or is there a reason to go bigger?)
3. Does the Temporal Walk-Through (Q7) flag anything this approach doesn't handle?

If validation reveals gaps, add them as "## Known Gaps" in the requirements doc.

## Next Step

Read `steps/step-05-requirements.md` and execute.
