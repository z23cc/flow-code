# Step 4: Approach Generation

## Interactive Mode (AUTO_MODE=false)

Generate 2-3 concrete approaches based on Phase 1 answers and codebase analysis.

For each approach:

| Field | Format |
|-------|--------|
| **Name** | Short descriptive label |
| **Summary** | One sentence — what this approach does |
| **Effort** | S / M / L |
| **Risk** | Low / Med / High |
| **Pros** | 2-3 bullets |
| **Cons** | 2-3 bullets |

Ask (via `AskUserQuestion`):
> Which approach do you prefer? (1/2/3, or "combine" to mix elements)

---

## Auto Mode (AUTO_MODE=true)

Same table format as interactive (Name/Summary/Effort/Risk/Pros/Cons), but **AI picks the best approach** instead of asking user.

Generate 2-3 approaches with the same table format.

**Auto-select logic** — pick the approach that:
1. Aligns best with existing codebase patterns (don't fight the codebase)
2. Has lowest risk for the effort level
3. Maximizes reuse of existing code

Output: "**Selected: Approach N** — <one-line reason based on code evidence>"

If approaches are genuinely close (risk/effort within one level), flag it:
> "Approaches N and M are close calls. Defaulting to N (<reason>). Override by re-running without --auto."

## Next Step

Read `steps/step-05-requirements.md` and execute.
