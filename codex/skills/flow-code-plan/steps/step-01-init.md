# Step 1: Initialize & Parse Input

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
# Get flowctl path
FLOWCTL="$HOME/.flow/bin/flowctl"

# Ensure .flow exists
$FLOWCTL init --json
```

## Success Criteria

- Plan references existing files/patterns with line refs
- Reuse points are explicit (centralized code called out)
- Acceptance checks are testable
- Tasks are small enough for one `/flow-code:work` iteration (split if not)
- **No implementation code** — specs describe WHAT, not HOW (see SKILL.md Golden Rule)
- Open questions are listed

## Task Sizing Rule

Use **T-shirt sizes** based on observable metrics — not token estimates (models can't reliably estimate tokens).

| Size | Files | Acceptance Criteria | Pattern | Action |
|------|-------|---------------------|---------|--------|
| **S** | 1-2 | 1-3 | Follows existing | Combine with related work |
| **M** | 3-5 | 3-5 | Adapts existing | Sweet spot |
| **L** | 5+ | 5+ | New/novel | Split into M tasks |

**M is the target size** — fits one context window (~80-100k tokens), makes meaningful progress.

**Rules**: Combine sequential S tasks into one M. Split L tasks into M tasks. If 7+ tasks, look for over-splitting. Minimize file overlap between tasks for parallel work — list expected files in `**Files:**`, use `flowctl dep add` when tasks must share files.

## Step 1: Initialize .flow

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
$FLOWCTL init --json
```

> **Note — opt-in interactive refinement:** If the user passed `--interactive`, BEFORE running Step 1 (Context Analysis in SKILL.md), invoke `/flow-code:interview` with the raw request text. The interview returns refined-spec markdown with Problem / Scope / Acceptance / Open Questions sections; use that refined text as the effective request for Context Analysis and all subsequent steps. Without the flag, skip this entirely — Step 2 below remains an automated internal brainstorm and is **not** interactive. Do not add any auto-trigger heuristic (length, punctuation, verb detection); interview must be opt-in only to preserve the zero-interaction contract (AGENTS.md:99).

## Step 1.5: Check for Prior Brainstorm Output

Before doing any research, check if `/flow-code:brainstorm` (or `/flow-code:go` Phase 0) already produced a requirements doc:

```bash
# If input is a file path to a requirements doc, use it directly
if [[ "$INPUT" == *.md ]] && [[ -f "$INPUT" ]]; then
  BRAINSTORM_DOC="$INPUT"
# Otherwise check .flow/specs/ for a matching requirements doc
elif ls .flow/specs/*-requirements.md 1>/dev/null 2>&1; then
  # Find the most recent requirements doc
  BRAINSTORM_DOC=$(ls -t .flow/specs/*-requirements.md 2>/dev/null | head -1)
fi
```

**If a brainstorm requirements doc exists**, you MUST read it and use as enriched context (this is NOT optional — brainstorm output is a first-class input to planning):
- Extract `## Problem`, `## Requirements`, `## Constraints`, `## Non-Goals` sections — these override any guesses
- Use `## Chosen Approach` to guide scout research direction and as the primary approach (do not re-derive from scratch)
- Use `## Evidence` file references as starting points for repo-scout
- Use `## Open Questions` as specific research targets for scouts — each MUST be resolved or explicitly carried as a gap
- Pass `## Self-Interview Trace` (if present) as additional context for deep RP analysis
- Reference the requirements doc in the epic spec: `**Source:** .flow/specs/<slug>-requirements.md`

This means `/flow-code:brainstorm` -> `/flow-code:plan` flows seamlessly: brainstorm output directly enriches plan research instead of being orphaned. The plan MUST be traceable back to the requirements doc.

**If no brainstorm doc exists**, proceed normally — Step 2 does its own mini clarity check.

## Step 2: Clarity Check (auto — no human input)

**Clear?** (specific behavior, bug with repro, existing pattern, has acceptance criteria) -> skip to Step 4 (research).

**Ambiguous?** (vague goal, multiple valid approaches, missing who/what/why, unclear scope) -> mini brainstorm:

1. Pressure test: What user problem? What if we do nothing? Simpler 80% framing?
2. Generate 2-3 approaches (minimal / balanced / comprehensive)
3. Pick best by: blast radius, value/effort, codebase alignment
4. Output: `Clarified: "<original>" -> "<specific target>" | Approach: <A|B|C> — <why>`

## Next Step

Read `steps/step-02-research.md` and execute.
