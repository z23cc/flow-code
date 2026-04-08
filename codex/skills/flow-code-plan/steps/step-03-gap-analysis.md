# Step 3: Gap Analysis & Scope

## Stakeholder & Scope Check

Before diving into gaps, identify who's affected:
- **End users** — What changes for them? New UI, changed behavior?
- **Developers** — New APIs, changed interfaces, migration needed?
- **Operations** — New config, monitoring, deployment changes?

This shapes what the plan needs to cover.

## Flow Gap Check

Run gap analyst subagent: `flow-code:flow-gap-analyst(<request>, research_findings)`. Fold gaps into the plan.

**After epic is created (Step 10):** Register gaps via `$FLOWCTL gap add --epic <id> --capability "<desc>" --priority required|important|nice-to-have --source flow-gap-analyst --json`. Priority mapping: "MUST answer" -> required, high-impact edge cases -> important, deferrable -> nice-to-have.

## Pick Depth

Default to standard unless complexity demands more or less.

### SHORT (bugs, small changes)
- Problem or goal
- Acceptance checks
- Key context

### STANDARD (most features)
- Overview + scope
- Approach
- Risks / dependencies
- Acceptance checks
- Test notes
- References
- Mermaid diagram if data model changes

### DEEP (large/critical)
- Detailed phases
- Alternatives considered
- Non-functional targets
- Architecture/data flow diagram (mermaid)
- Rollout/rollback
- Docs + metrics
- Risks + mitigations

## Next Step

Read `steps/step-04-task-breakdown.md` and execute.
