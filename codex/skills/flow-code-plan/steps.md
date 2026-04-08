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

Use **T-shirt sizes** based on observable metrics — not token estimates (models can't reliably estimate tokens).

| Size | Files | Acceptance Criteria | Pattern | Action |
|------|-------|---------------------|---------|--------|
| **S** | 1-2 | 1-3 | Follows existing | Combine with related work |
| **M** | 3-5 | 3-5 | Adapts existing | ✅ **Sweet spot** |
| **L** | 5+ | 5+ | New/novel | ⚠️ Split into M tasks |

**M is the target size** — fits one context window (~80-100k tokens), makes meaningful progress.

**Rules**: Combine sequential S tasks into one M. Split L tasks into M tasks. If 7+ tasks, look for over-splitting. Minimize file overlap between tasks for parallel work — list expected files in `**Files:**`, use `flowctl dep add` when tasks must share files.

## Step 1: Initialize .flow

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
# Get flowctl path
FLOWCTL="$HOME/.flow/bin/flowctl"

# Ensure .flow exists
$FLOWCTL init --json
```

> **Note — opt-in interactive refinement:** If the user passed `--interactive`, BEFORE running Step 1 (Context Analysis in SKILL.md), invoke `/flow-code:interview` with the raw request text. The interview returns refined-spec markdown with Problem / Scope / Acceptance / Open Questions sections; use that refined text as the effective request for Context Analysis and all subsequent steps. Without the flag, skip this entirely — Step 2 below remains an automated internal brainstorm and is **not** interactive. Do not add any auto-trigger heuristic (length, punctuation, verb detection); interview must be opt-in only to preserve the zero-interaction contract (AGENTS.md:99).

## Step 1.5: Check for prior brainstorm output

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

**If a brainstorm requirements doc exists**, read it and use as enriched context:
- Extract `## Problem`, `## Requirements`, `## Constraints`, `## Non-Goals` sections
- Use `## Chosen Approach` to guide scout research direction
- Use `## Evidence` file references as starting points for repo-scout
- Use `## Open Questions` as specific research targets for scouts
- Pass `## Self-Interview Trace` (if present) as additional context for deep RP analysis

This means `/flow-code:brainstorm` → `/flow-code:plan` flows seamlessly: brainstorm output directly enriches plan research instead of being orphaned.

**If no brainstorm doc exists**, proceed normally — Step 2 does its own mini clarity check.

## Step 2: Clarity Check (auto — no human input)

**Clear?** (specific behavior, bug with repro, existing pattern, has acceptance criteria) → skip to Step 4.

**Ambiguous?** (vague goal, multiple valid approaches, missing who/what/why, unclear scope) → mini brainstorm:

1. Pressure test: What user problem? What if we do nothing? Simpler 80% framing?
2. Generate 2-3 approaches (minimal / balanced / comprehensive)
3. Pick best by: blast radius, value/effort, codebase alignment
4. Output: `Clarified: "<original>" → "<specific target>" | Approach: <A|B|C> — <why>`

## Step 4: Fast research (parallel)

**If input is a Flow ID** (fn-N-slug or fn-N-slug.M, including legacy fn-N/fn-N-xxx): First fetch it with `$FLOWCTL show <id> --json` and `$FLOWCTL cat <id>` to get the request context.

**Check config flags and stack profile:**
```bash
$FLOWCTL config get memory.enabled --json
$FLOWCTL config get scouts.github --json
$FLOWCTL stack show --json
```

**Check architecture invariants:**
```bash
$FLOWCTL invariants show --json
```
If invariants exist, ensure all planned tasks respect them. If a task would violate an invariant, note the conflict in the task spec and flag it.

Stack is auto-detected on `init`. If present, use it throughout planning:
- Include framework/language in scout prompts (e.g., "Django DRF patterns", "Next.js App Router")
- Use `stack.*.conventions` to guide task spec writing
- Put `$FLOWCTL guard` in epic's Quick commands section (replaces manual test/lint commands)
- Tag task specs with which stack layer they belong to (backend/frontend/infra) in the Files field

**Scout selection: AI decides per-request.**

### Scout decision guide

- **Always**: `repo-scout` (fast grep-based research). `memory-scout` if memory.enabled. `capability-scout` unless `--no-capability-scan` passed (non-blocking; fails open — planning continues if it errors).
- **Deep context** (replaces `context-scout` in this guide — exactly one runs per plan, not multiple):
  - **Tier 1** (MCP available): direct `context_builder(response_type:"plan")` call — best quality, automatic workspace binding
  - **Tier 2** (rp-cli available, no MCP): `rp-cli -e 'builder "<request + repo-scout findings>" --response-type plan'` (timeout: 300s)
  - **Tier 3** (neither available): `context-scout` subagent (existing behavior, unchanged)
- **Add when needed**: `practice-scout` for security/auth/payments/concurrency. `docs-scout` for external APIs/libraries. `github-scout` for novel patterns (requires scouts.github). `epic-scout` if 2+ open epics. `docs-gap-scout` if user-facing changes. `flow-gap-analyst` — maps user flows, edge cases, and missing requirements from the spec.
- **Constraints**: min 1 (repo-scout required), max 7. Run ALL selected scouts in ONE parallel Agent/Task call. Deep context (Tier 1/2/3) runs AFTER repo-scout returns — it uses repo-scout findings as input.

Must capture:
- File paths + line refs
- Existing centralized code to reuse
- Similar patterns / prior work
- External docs links
- Project conventions (AGENTS.md, CONTRIBUTING, etc)
- Architecture patterns and data flow
- Epic dependencies (from epic-scout)
- Doc updates needed (from docs-gap-scout) - add to task acceptance criteria
- Capability gaps (from capability-scout) - persist in Step 10 (see below)

### Scout output parsing

Each scout returns Markdown with a `json:scout-summary` block at the end. Parse this block to extract structured data:

```
references[]     → populate task Investigation targets (Required files)
reusable_code[]  → add to task Key context ("Reuse: path/export — usage")
conventions[]    → apply to epic spec Project Conventions section
gaps[]           → feed to gap analyst, add to Open Questions
```

If a scout returns no `json:scout-summary` block (legacy format), fall back to parsing Markdown sections manually (References, Reusable Code, Gaps).

### Step 5: Deep context via RP (after repo-scout)

After repo-scout returns, gather deep codebase context using the best available RP tier. **Exactly one RP-powered call per plan run** — do not call both context_builder and context-scout.

**Tier 1 — RP MCP (preferred):**
```
context_builder(
  instructions: "<request summary> + <repo-scout key findings>",
  response_type: "plan"
)
```

**Tier 2 — rp-cli (fallback when MCP unavailable):**
```bash
rp-cli -e 'builder "<request summary> + <repo-scout key findings>" --response-type plan'
# Timeout: 300s (builder can take minutes)
```

**Tier 3 — context-scout subagent (fallback when neither MCP nor CLI available):**
Run `context-scout` as a subagent (existing behavior, unchanged). This is the pre-existing path.

**Skip condition:** If the request is trivial (clear bug fix, single-file change, S-size task), skip deep context — repo-scout alone is sufficient.

Feed RP/context-scout findings into the epic spec alongside repo-scout findings.

## Step 6: Apply memory lessons (if memory.enabled)

**Skip if memory.enabled is false.**

After scouts complete, check if memory-scout found relevant entries. If so, directly inject them:

```bash
# Quick scan — L1 index (~50 tokens/entry)
$FLOWCTL memory inject --json
```

Scan the L1 index for entries relevant to this plan's domain. If relevant entries exist, fetch full content:

```bash
# Fetch details for relevant entries
$FLOWCTL memory search "<keyword matching this plan's domain>"
```

> **Deduplication note:** Memory is injected here at plan time for research context. Workers also inject memory in Phase 2, but scoped to their specific task domain via `--tags`. Plan-phase injection is broad (full domain scan); worker injection is narrow (task-specific tags). This is intentional — plan needs wide context, workers need focused context. No deduplication is needed because the scopes differ.

**Apply lessons to plan design:**
- **Pitfalls** → add as explicit warnings in task specs or acceptance criteria ("Verify X does not regress Y")
- **Conventions** → ensure tasks follow discovered patterns, reference them in spec
- **Decisions** → respect past architectural choices unless the plan explicitly supersedes them

**Rules:**
- Don't bloat tasks with every memory entry — only apply entries clearly relevant to this plan
- If a past decision conflicts with the current plan, note it as an explicit "supersedes decision #N" in the epic spec
- 0-3 applied entries per plan is normal

## Step 7: Stakeholder & scope check

Before diving into gaps, identify who's affected:
- **End users** — What changes for them? New UI, changed behavior?
- **Developers** — New APIs, changed interfaces, migration needed?
- **Operations** — New config, monitoring, deployment changes?

This shapes what the plan needs to cover.

## Step 8: Flow gap check

Run gap analyst subagent: `flow-code:flow-gap-analyst(<request>, research_findings)`. Fold gaps into the plan.

**After epic is created (Step 10):** Register gaps via `$FLOWCTL gap add --epic <id> --capability "<desc>" --priority required|important|nice-to-have --source flow-gap-analyst --json`. Priority mapping: "MUST answer" → required, high-impact edge cases → important, deferrable → nice-to-have.

## Step 9: Pick depth

Default to standard unless complexity demands more or less.

**SHORT** (bugs, small changes)
- Problem or goal
- Acceptance checks
- Key context

**STANDARD** (most features)
- Overview + scope
- Approach
- Risks / dependencies
- Acceptance checks
- Test notes
- References
- Mermaid diagram if data model changes

**DEEP** (large/critical)
- Detailed phases
- Alternatives considered
- Non-functional targets
- Architecture/data flow diagram (mermaid)
- Rollout/rollback
- Docs + metrics
- Risks + mitigations

## Step 10: Write to .flow

**Efficiency note**: Use stdin (`--file -`) with heredocs to avoid temp files. Use `task spec` to set description + acceptance in one call.

**Route A - Input was an existing Flow ID**:

1. If epic ID (fn-N-slug or legacy fn-N/fn-N-xxx):
   ```bash
   # Use stdin heredoc (no temp file needed)
   $FLOWCTL epic plan <id> --file - --json <<'EOF'
   <plan content here>
   EOF
   ```
   - Create/update child tasks as needed

2. If task ID (fn-N-slug.M or legacy fn-N.M/fn-N-xxx.M):
   ```bash
   # Combined set-spec: description + acceptance in one call
   # Write to temp files only if content has single quotes
   $FLOWCTL task spec <id> --desc /tmp/desc.md --accept /tmp/acc.md --json
   ```

**Route B - Input was text (new idea)**:

1. Create epic:
   ```bash
   $FLOWCTL epic create --title "<Short title>" --json
   ```
   This returns the epic ID (e.g., fn-1-add-oauth).

2. Set epic branch_name (deterministic):
   - Default: use epic ID (e.g., fn-1-add-oauth)
   ```bash
   $FLOWCTL epic branch <epic-id> "<epic-id>" --json
   ```
   - If user specified a branch, use that instead.

3. Write epic spec (use stdin heredoc):
   ```bash
   # Include: Overview, Scope, Approach, Quick commands (REQUIRED), Acceptance, References
   # Add mermaid diagram if data model or architecture changes
   $FLOWCTL epic plan <epic-id> --file - --json <<'EOF'
   # Epic Title

   ## Overview
   ...

   ## Quick commands
   ```bash
   # At least one smoke test command
   ```

   ## Acceptance
   ...
   EOF
   ```

4. Set epic dependencies (from epic-scout findings):

   If epic-scout found dependencies, set them automatically:
   ```bash
   # For each dependency found by epic-scout:
   $FLOWCTL epic add-dep <new-epic-id> <dependency-epic-id> --json
   ```

   Report findings at end of planning (no user prompt needed):
   ```
   Epic dependencies set:
   - fn-N-slug → fn-2-add-auth (Auth): Uses authService from fn-2-add-auth.1
   - fn-N-slug → fn-5-user-model (DB): Extends User model
   ```

5. Create child tasks:
   ```bash
   # Task with no dependencies:
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --json

   # Task with dependencies:
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --deps <dep1>,<dep2> --json

   # Task with domain tag (RECOMMENDED — drives worker skill loading):
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --domain <domain> --json
   # Valid domains: frontend, backend, architecture, testing, docs, ops, general
   # ALWAYS set --domain for tasks that touch UI/components/pages (use "frontend")
   # Workers load domain-specific skills automatically (e.g., frontend → flow-code-frontend-ui)

   # Task with file ownership (recommended for parallel execution):
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --files "src/auth.ts,src/routes.ts" --json
   # Enables flowctl files <id> to detect conflicts before parallel execution
   ```

   **TIP**: Use `--deps` to declare dependencies inline when creating tasks. Tasks must exist before being referenced, so create in dependency order. **Always set `--domain`** when the task clearly belongs to a specific area — this triggers domain-specific skill loading in workers (e.g., `frontend` loads UI engineering best practices). Use `--files` to declare file ownership for teams/parallel conflict prevention.

6. Write task specs (use combined set-spec):
   ```bash
   # For each task - single call sets both sections
   # Write description and acceptance to temp files, then:
   $FLOWCTL task spec <task-id> --desc /tmp/desc.md --accept /tmp/acc.md --json
   ```

   **Task spec content** (remember: NO implementation code):
   ```markdown
   ## Description
   [What to build, not how to build it]

   **Size:** S/M (L tasks should be split)
   **Layer:** backend | frontend | infra | full-stack
   **Files:** list expected files

   ## Approach
   - Follow pattern at `src/example.ts:42`
   - Reuse `existingHelper()` from `lib/utils.ts`

   ## Investigation targets
   **Required** (read before coding):
   - `src/auth/oauth.ts` — existing OAuth flow to extend
   - `src/middleware/session.ts:23-45` — session validation pattern

   **Optional** (reference as needed):
   - `src/auth/*.test.ts` — existing test patterns

   ## Key context
   [Only for recent API changes, surprising patterns, or non-obvious gotchas]
   [If stack config exists, include relevant framework conventions here]

   ## Acceptance
   - [ ] Criterion 1
   - [ ] Criterion 2
   ```

   **Investigation targets rules:**
   - Max 5-7 targets per task — enough to ground the worker, not so many it wastes context
   - Use exact file paths with optional line ranges (e.g., `src/auth.ts:23-45`)
   - **Required** = must read before implementing. **Optional** = helpful reference
   - Auto-populated from repo-scout/context-scout findings in Step 4 research
   - If no relevant files found by scouts, leave the section empty (worker skips Phase 1.5)

   **Layer field**: If stack config is set, tag each task with its primary layer. This helps the worker select the right guard commands (e.g., `pytest` for backend, `pnpm test` for frontend). Full-stack tasks run all guards.

7. Add task dependencies (if not already set via `--deps`):

   **Preferred**: Use `--deps` flag during task creation (step 5). This saves tool calls.

   **Alternative**: Use `dep add` to add dependencies after task creation:
   ```bash
   # Syntax: dep add <dependent-task> <dependency-task>
   # "task B depends on task A" → dep add B A
   $FLOWCTL dep add fn-N.2 fn-N.1 --json
   ```

   Use `dep add` when you need to add dependencies to existing tasks or fix missed dependencies.

8. Output current state:
   ```bash
   $FLOWCTL show <epic-id> --json
   $FLOWCTL cat <epic-id>
   ```

## Step 11: Write capability-gaps.md (if capability-scout ran)

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

## Step 12: Validate

```bash
$FLOWCTL validate --epic <epic-id> --json
```

Fix any errors before proceeding.

### Step 13: Auto-Extract Acceptance Checklist

After validation, generate `.flow/checklists/<epic-id>.json` by parsing `## Acceptance` sections from epic + task specs. Each `- [ ]` line becomes a checklist item with `source` (epic or task ID) and `status: "pending"`. Skip if no acceptance criteria found. Commit with the plan (`git add .flow/checklists/`). Consumed by `/flow-code:epic-review`.

## Step 14: Review (if chosen at start)

If review was decided in Context Analysis:
1. Initialize `PLAN_REVIEW_ITERATIONS=0`
2. Invoke `/flow-code:plan-review` with the epic ID
3. If review returns "Needs Work" or "Major Rethink":
   - Increment `PLAN_REVIEW_ITERATIONS`
   - **If `PLAN_REVIEW_ITERATIONS >= 2`**: stop the loop. Log: "Plan review: 2 iterations completed. Proceeding." Go to Step 15.
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

## Step 15: Execute or Offer next steps

**If `--plan-only`**: print `Plan created: <epic-id> (N tasks) | Next: /flow-code:work <epic-id>` and stop.

**Otherwise (default — auto-execute immediately, no menu):**

```bash
$FLOWCTL epic auto-exec <epic-id> --pending --json
```

Invoke `/flow-code:work <epic-id> --no-review` directly (Teams mode handles parallelism regardless of task count).

> **Flag precedence:** `--no-review` passed here overrides any `review.backend` config setting. This is intentional — when plan auto-executes work, per-task review is skipped because the plan was already reviewed. Epic-level review still runs at completion unless explicitly disabled.
