# Step 5: Validate, Review & Output

## Write Capability Gaps (if capability-scout ran)

**Skip if `--no-capability-scan` was passed, or capability-scout was not run, or scout errored (fails open).**

After epic creation, persist capability-scout findings to `.flow/epics/<epic-id>/capability-gaps.md` (human-readable markdown, NOT JSON — plan-review scans this file).

```bash
mkdir -p .flow/epics/<epic-id>
cat > .flow/epics/<epic-id>/capability-gaps.md <<'EOF'
# Capability Gaps — <epic-id>

Source: capability-scout (plan-time)

<human summary table + references from capability-scout output>
EOF
```

For each `priority: required` gap in the scout's JSON output, persist in the gap registry:

```bash
$FLOWCTL gap add --epic <epic-id> \
  --capability "<capability>: <details>" \
  --priority required \
  --source capability-scout --json
```

`important` and `nice-to-have` gaps are recorded in the markdown file only — not in the gap registry (don't over-fill with noise).

## Validate

```bash
$FLOWCTL validate --epic <epic-id> --json
```

Fix any errors before proceeding.

## Auto-Extract Acceptance Checklist

After validation, generate `.flow/checklists/<epic-id>.json` by parsing `## Acceptance` sections from epic + task specs. Each `- [ ]` line becomes a checklist item with `source` (epic or task ID) and `status: "pending"`. Skip if no acceptance criteria found. Commit with the plan (`git add .flow/checklists/`). Consumed by `/flow-code:epic-review`.

## Review (if chosen at start)

If review was decided in Context Analysis:
1. Initialize `PLAN_REVIEW_ITERATIONS=0`
2. Invoke `/flow-code:plan-review` with the epic ID
3. If review returns "Needs Work" or "Major Rethink":
   - Increment `PLAN_REVIEW_ITERATIONS`
   - **If `PLAN_REVIEW_ITERATIONS >= 2`**: stop the loop. Log: "Plan review: 2 iterations completed. Proceeding." Go to Execute.
   - **Re-anchor EVERY iteration** (do not skip):
     ```bash
     $FLOWCTL show <epic-id> --json
     $FLOWCTL cat <epic-id>
     ```
   - **Immediately fix the issues** (do NOT ask for confirmation — user already consented)
   - Re-run `/flow-code:plan-review`
4. Repeat until review returns "Ship" or iteration limit reached.

**No human gates here** — the review-fix-review loop is fully automated. Max 5 iterations prevents infinite loops.

**Why re-anchor every iteration?** Per Anthropic's long-running agent guidance: context compresses, you forget details. Re-read before each fix pass.

## Offer Next Steps

`/flow-code:plan` stops after planning. Print a concise summary and the most relevant next command.

Default output shape:

- `Plan created: <epic-id> (N tasks)`
- `Next: /flow-code:work <epic-id>`
- Optional alternatives:
  - `/flow-code:go <epic-id>` to continue on the full execution pipeline
  - `/flow-code:plan-review <epic-id>` if the user wants an explicit follow-up review pass

If `--plan-only` was passed, treat it as a no-op compatibility flag and stop here the same way.
