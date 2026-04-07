# Flow Plan Steps

**IMPORTANT**: Steps 4-9 (research, gap analysis, depth) ALWAYS run regardless of input type.

**CRITICAL**: If you are about to create:
- a markdown TODO list,
- a task list outside `.flow/`,
- or any plan files outside `.flow/`,

**STOP** and instead:
- create/update tasks in `.flow/` using `flowctl`,
- record details in the epic/task spec markdown.

## Success criteria

- Plan references existing files/patterns with line refs
- Reuse points are explicit (centralized code called out)
- Acceptance checks are testable
- Tasks are small enough for one `/flow-code:work` iteration (split if not)
- **No implementation code** — specs describe WHAT, not HOW (see SKILL.md Golden Rule)
- Open questions are listed

## Task Sizing Rule

| Size | Files | Acceptance Criteria | Pattern | Action |
|------|-------|---------------------|---------|--------|
| **S** | 1-2 | 1-3 | Follows existing | Combine with related work |
| **M** | 3-5 | 3-5 | Adapts existing | **Sweet spot** |
| **L** | 5+ | 5+ | New/novel | Split into M tasks |

**M is the target size.** Combine sequential S tasks. Split L tasks. If 7+ tasks, look for over-splitting. Minimize file overlap — list expected files in `**Files (write/read):**`, use `flowctl dep add` when tasks must share files.

## Step 1: Initialize .flow

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
$FLOWCTL init --json
```

### Adaptive depth

Classify the request: `$FLOWCTL plan-depth --request "<text>" --json`

- `"quick"`: execute only Steps 1, 4, 10, 12. Skip all others.
- `"standard"`: skip Steps 2, 6, 7, 9, 11. Execute rest.
- `"deep"`: execute all steps.

> **Opt-in interactive refinement:** If `--interactive`, invoke `/flow-code:interview` first. Without the flag, skip entirely.

## Step 2: Clarity Check (auto — no human input)

**Skip if brainstorm already ran** (check `.flow/specs/` for `*-requirements.md`).

**Clear?** (specific behavior, bug, existing pattern, has acceptance criteria) → skip to Step 4.

**Ambiguous?** → mini brainstorm:
1. Pressure test: What user problem? Simpler 80% framing?
2. Generate 2-3 approaches (minimal / balanced / comprehensive)
3. Pick best by: blast radius, value/effort, codebase alignment
4. Output: `Clarified: "<original>" → "<specific target>" | Approach: <choice> — <why>`

## Step 3: Skill routing (auto — non-blocking)

Match request against engineering discipline skills: `$FLOWCTL skill match "<keywords>" --threshold 0.70 --limit 3 --json`. Save matches for Step 10 task spec writing (max 3 skill references per task).

## Step 4: Fast research (parallel)

**If input is a Flow ID**: fetch with `$FLOWCTL show/cat` first.

**Check config and stack:** `$FLOWCTL config get memory.enabled`, `scouts.github`, `stack show`, `invariants show`.

**Scout cache:** Check cache at current commit before spawning. Only spawn scouts with cache misses. Cache results after completion.

### Scout profiles

| Profile | Scouts | When |
|---------|--------|------|
| **quick** | `repo-scout` only | S-size, clear bugs |
| **standard** | `repo-scout` + `capability-scout` + deep context + `memory-scout` | Default |
| **deep** | All standard + `practice-scout` + `docs-scout` + `github-scout` + `epic-scout` + `docs-gap-scout` | Architecture, security |

Auto-selected from depth. Override: `--research=quick|standard|deep`. Min 1, max 7 scouts. Run ALL in one parallel call.

Must capture: file paths + line refs, reusable code, patterns, conventions, architecture, epic dependencies, doc updates needed, capability gaps.

### Step 5: Deep context via RP (after repo-scout)

Exactly one RP-powered call per plan:
- **Tier 1 (MCP):** `context_builder(response_type: "plan")`
- **Tier 2 (CLI):** `rp-cli -e 'builder ... --response-type plan'` (timeout 300s)
- **Tier 3:** `context-scout` subagent (fallback)

Skip for trivial requests (S-size, single-file).

## Step 6: Apply memory lessons (if memory.enabled)

Scan L1 index for relevant entries. Apply to plan:
- **Pitfalls** → warnings in task specs
- **Conventions** → ensure tasks follow patterns
- **Decisions** → respect past choices (note if superseding)

0-3 applied entries per plan is normal.

## Step 7: Stakeholder & scope check

Identify who's affected: end users, developers, operations. This shapes what the plan covers.

## Step 8: Flow gap check

Run `flow-code:flow-gap-analyst`. After epic creation, register gaps via `$FLOWCTL gap add`. Priority mapping: "MUST answer" → required, high-impact → important, deferrable → nice-to-have.

## Step 9: Pick depth

- **SHORT** (bugs, small): Problem, acceptance, key context
- **STANDARD** (most features): Overview, scope, approach, risks, acceptance, tests, references, diagram if data model changes
- **DEEP** (large/critical): Detailed phases, alternatives, non-functional targets, architecture diagram, rollout/rollback, docs, metrics, risks + mitigations

## Step 10: Write to .flow

**Route A — Existing Flow ID**: Update epic plan or task spec via `$FLOWCTL epic plan` / `$FLOWCTL task spec`.

**Route B — New idea**:
1. Create epic: `$FLOWCTL epic create --title "<title>" --json`
2. Set branch: `$FLOWCTL epic branch <id> "<id>" --json`
3. Write epic spec via stdin heredoc (`$FLOWCTL epic plan <id> --file -`). Include: Overview, Scope, Approach, Quick commands (REQUIRED), Acceptance, References.
4. Set epic dependencies from epic-scout findings
5. Create tasks with `--deps`, `--domain`, `--files` flags as appropriate. Create in dependency order.
6. Write task specs via `$FLOWCTL task spec <id> --desc --accept`

**Task spec template** (NO implementation code): Description (what, not how) + Size (S/M) + Layer + Files (write/read) + Approach (patterns to follow, code to reuse) + Investigation targets (Required: read before coding; Optional: reference. Max 5-7, exact paths with line ranges, from scouts) + Key context (gotchas only) + Acceptance (`- [ ]` criteria).

## Step 11: Capability gaps (if capability-scout ran)

Persist to `.flow/epics/<epic-id>/capability-gaps.md`. Register `required` gaps via `$FLOWCTL gap add`.

## Step 12: Validate

`$FLOWCTL validate --epic <id> --json` — fix any errors.

### Step 13: Auto-Extract Acceptance Checklist

Parse `## Acceptance` sections from specs into `.flow/checklists/<epic-id>.json`. Each `- [ ]` becomes a checklist item. Consumed by `/flow-code:epic-review`.

## Step 14: Review (if chosen at start)

1. Initialize `PLAN_REVIEW_ITERATIONS=0`
2. Invoke `/flow-code:plan-review`
3. If "Needs Work": increment iterations, re-anchor (`$FLOWCTL show/cat`), fix, re-run review
4. Max 3 iterations (MAX_REVIEW_ITERATIONS). Fully automated — no human gates.

## Step 15: Execute or Offer next steps

- **`--plan-only`**: print plan summary and stop.
- **Default**: mark auto-exec pending, invoke `/flow-code:work <epic-id> --no-review` directly.
