# Flow Plan Steps

**IMPORTANT**: Steps 1-3 (research, gap analysis, depth) ALWAYS run regardless of input type.

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

**Anchor examples** (calibrate against these):
- **S**: Fix a bug, add config, simple UI tweak → combine if sequential
- **M**: New API endpoint with tests, new component with state → ideal
- **L**: New subsystem, architectural change → split into M tasks

**Combine rule**: Sequential S tasks touching related code → combine into one M task.

**If too large, split it:**
- ❌ Bad: "Implement Google OAuth" (L — new subsystem)
- ✅ Good:
  - "Google OAuth backend (config + passport + routes)" (M)
  - "Add Google sign-in button" (S)

**If too granular (7+ tasks), combine:**
- ❌ Over-split: 4 sequential S tasks for backend setup
- ✅ Better: 1 M task covering the sequential work

**Minimize file overlap for parallel work:**

When splitting tasks, design for minimal file overlap. Tasks touching disjoint files can be worked in parallel without merge conflicts.

- ❌ Bad: Task A and B both modify `src/auth.ts`
- ✅ Good: Task A modifies `src/auth.ts`, Task B modifies `src/routes.ts`

List expected files in each task's `**Files:**` field. If multiple tasks must touch the same file, mark dependencies explicitly with `flowctl dep add`.

## Step 0: Initialize .flow

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:

```bash
# Get flowctl path
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl.py"

# Ensure .flow exists
$FLOWCTL init --json
```

## Step 0.5: Clarity Check (auto — no human input)

Before research, check if the request is clear enough to plan directly.

**Clear request indicators** (skip to Step 1):
- Specific feature with defined behavior ("add /api/health/ endpoint returning JSON status")
- Bug fix with reproduction steps
- References existing patterns to follow
- Has acceptance criteria or spec file

**Ambiguous request indicators** (run mini brainstorm):
- Vague goal ("improve UX", "make it faster", "refactor auth")
- Multiple valid approaches with no obvious best choice
- Missing who/what/why ("add payment support" — subscription? one-time? both?)
- Scope unclear ("redesign the dashboard" — full rewrite? incremental?)

**If ambiguous → Mini brainstorm (AI decides, no human input):**

1. **Pressure test** (3 questions, answer them yourself from context):
   - What specific user problem does this solve?
   - What happens if we do nothing?
   - Is there a simpler framing that delivers 80% of the value?

2. **Generate 2-3 approaches** (one sentence each):
   - Approach A: [minimal/safe]
   - Approach B: [balanced]
   - Approach C: [comprehensive]

3. **Pick the best approach** based on:
   - Blast radius (smaller is safer)
   - Value delivered per effort
   - Alignment with existing codebase patterns (from repo-scout later, but estimate now)
   
4. **Output one line** and continue to Step 1:
   ```
   Clarified: "<original request>" → "<specific plan target>"
   Approach: <A|B|C> — <one sentence why>
   ```

**Total cost**: ~10 seconds for the clarity check. ~30 seconds if mini brainstorm runs. Zero for clear requests.

## Step 1: Fast research (parallel)

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

**Scout selection: AI decides per-request.** For each scout, a specific question tells you if it's worth running.

### Scout decision guide

| Scout | ~Cost | Include when... | Skip when... |
|-------|-------|----------------|-------------|
| `repo-scout` | 10s | **Always** — unless using context-scout instead | Using context-scout |
| `context-scout` | 30s | Change touches multiple modules or unfamiliar code. Requires rp-cli | Single-file change in well-known area. rp-cli unavailable |
| `practice-scout` | 20s | Involves security, auth, payments, concurrency, new patterns, or anything you're not 100% sure about best practices | Straightforward CRUD, config change, or well-trodden pattern you've done many times in this codebase |
| `docs-scout` | 20s | Uses an API/library you haven't used recently, or a framework feature that may have changed | Uses only project-internal code, no external APIs |
| `github-scout` | 20s | Novel pattern with no existing example in this codebase. Requires scouts.github | Pattern already exists in this repo (repo-scout will find it) |
| `memory-scout` | 5s | **Always if memory.enabled** — near-zero cost, catches known pitfalls | memory.enabled is false |
| `epic-scout` | 10s | Project has 2+ open epics that might overlap | This is the only active epic |
| `docs-gap-scout` | 10s | Change adds/changes user-facing behavior, public APIs, or CLI commands | Internal refactor, backend-only change invisible to users |

### Constraints

- **Minimum 1**: repo-scout or context-scout (must understand existing code)
- **Maximum 7**: coordination overhead dominates beyond this
- **Parallel only**: run ALL selected scouts in ONE Agent/Task call, never sequentially
- **When in doubt, include**: a wasted 10s scout is cheaper than a plan missing context

Must capture:
- File paths + line refs
- Existing centralized code to reuse
- Similar patterns / prior work
- External docs links
- Project conventions (CLAUDE.md, CONTRIBUTING, etc)
- Architecture patterns and data flow (especially with context-scout)
- Epic dependencies (from epic-scout)
- Doc updates needed (from docs-gap-scout) - add to task acceptance criteria

## Step 1b: Apply memory lessons (if memory.enabled)

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

**Apply lessons to plan design:**
- **Pitfalls** → add as explicit warnings in task specs or acceptance criteria ("Verify X does not regress Y")
- **Conventions** → ensure tasks follow discovered patterns, reference them in spec
- **Decisions** → respect past architectural choices unless the plan explicitly supersedes them

**Rules:**
- Don't bloat tasks with every memory entry — only apply entries clearly relevant to this plan
- If a past decision conflicts with the current plan, note it as an explicit "supersedes decision #N" in the epic spec
- 0-3 applied entries per plan is normal

## Step 2: Stakeholder & scope check

Before diving into gaps, identify who's affected:
- **End users** — What changes for them? New UI, changed behavior?
- **Developers** — New APIs, changed interfaces, migration needed?
- **Operations** — New config, monitoring, deployment changes?

This shapes what the plan needs to cover. A pure backend refactor needs different detail than a user-facing feature.

## Step 3: Flow gap check

Run the gap analyst subagent:
- Task flow-code:flow-gap-analyst(<request>, research_findings)

Fold gaps + questions into the plan.

**After epic is created (Step 5):** Register each gap found by the analyst into the gap registry:

```bash
# For each gap identified by flow-gap-analyst:
$FLOWCTL gap add --epic <epic-id> --capability "<gap description>" \
  --priority required|important|nice-to-have \
  --source flow-gap-analyst --json
```

Map analyst output to priority:
- "Priority Questions (MUST answer before coding)" → `required`
- "Edge Cases" with high impact → `important`
- "Nice-to-Clarify (can defer)" → `nice-to-have`

This makes gaps machine-trackable. `epic-review` and `epic close` will verify all required/important gaps are resolved.

## Step 4: Pick depth

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

## Step 5: Write to .flow

**Efficiency note**: Use stdin (`--file -`) with heredocs to avoid temp files. Use `task set-spec` to set description + acceptance in one call.

**Route A - Input was an existing Flow ID**:

1. If epic ID (fn-N-slug or legacy fn-N/fn-N-xxx):
   ```bash
   # Use stdin heredoc (no temp file needed)
   $FLOWCTL epic set-plan <id> --file - --json <<'EOF'
   <plan content here>
   EOF
   ```
   - Create/update child tasks as needed

2. If task ID (fn-N-slug.M or legacy fn-N.M/fn-N-xxx.M):
   ```bash
   # Combined set-spec: description + acceptance in one call
   # Write to temp files only if content has single quotes
   $FLOWCTL task set-spec <id> --description /tmp/desc.md --acceptance /tmp/acc.md --json
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
   $FLOWCTL epic set-branch <epic-id> --branch "<epic-id>" --json
   ```
   - If user specified a branch, use that instead.

3. Write epic spec (use stdin heredoc):
   ```bash
   # Include: Overview, Scope, Approach, Quick commands (REQUIRED), Acceptance, References
   # Add mermaid diagram if data model or architecture changes
   $FLOWCTL epic set-plan <epic-id> --file - --json <<'EOF'
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

   # Task with domain tag (optional — helps worker adjust strategy):
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --domain <domain> --json
   # Valid domains: frontend, backend, architecture, testing, docs, ops, general

   # Task with file ownership (recommended for --teams mode):
   $FLOWCTL task create --epic <epic-id> --title "<Task title>" --files "src/auth.ts,src/routes.ts" --json
   # Enables flowctl files --epic <id> to detect conflicts before parallel execution
   ```

   **TIP**: Use `--deps` to declare dependencies inline when creating tasks. Tasks must exist before being referenced, so create in dependency order. Use `--domain` when the task clearly belongs to a specific area. Use `--files` to declare file ownership for teams/parallel conflict prevention.

6. Write task specs (use combined set-spec):
   ```bash
   # For each task - single call sets both sections
   # Write description and acceptance to temp files, then:
   $FLOWCTL task set-spec <task-id> --description /tmp/desc.md --acceptance /tmp/acc.md --json
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

   ## Key context
   [Only for recent API changes, surprising patterns, or non-obvious gotchas]
   [If stack config exists, include relevant framework conventions here]

   ## Acceptance
   - [ ] Criterion 1
   - [ ] Criterion 2
   ```

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

## Step 6: Validate

```bash
$FLOWCTL validate --epic <epic-id> --json
```

Fix any errors before proceeding.

### Step 6b: Auto-Extract Acceptance Checklist

After validation passes, auto-generate a machine-readable checklist from acceptance criteria:

```bash
# Extract acceptance criteria from epic spec and all task specs into checklist.json
$FLOWCTL cat <epic-id> > /tmp/epic-spec.md
```

Parse the epic spec and all task specs for `## Acceptance` sections. Extract each `- [ ]` bullet point.

Write a structured checklist to `.flow/checklists/<epic-id>.json`:

```bash
mkdir -p .flow/checklists
cat > .flow/checklists/<epic-id>.json <<'EOF'
{
  "epic_id": "<epic-id>",
  "generated_at": "<ISO timestamp>",
  "items": [
    {"id": "epic.1", "source": "<epic-id>", "criterion": "First acceptance criterion from epic spec", "status": "pending"},
    {"id": "epic.2", "source": "<epic-id>", "criterion": "Second acceptance criterion from epic spec", "status": "pending"},
    {"id": "<task-id>.1", "source": "<task-id>", "criterion": "First acceptance criterion from task spec", "status": "pending"},
    {"id": "<task-id>.2", "source": "<task-id>", "criterion": "Second acceptance criterion from task spec", "status": "pending"}
  ]
}
EOF
```

Rules:
- Each `- [ ]` line becomes one checklist item
- `source` tracks which spec (epic or task) it came from
- `status` starts as `pending`, set to `pass`/`fail` during review
- If no acceptance criteria found, skip (don't create empty checklist)
- Commit the checklist with the plan: `git add .flow/checklists/`

This checklist is consumed by `/flow-code:epic-review` for structured verification.

## Step 7: Review (if chosen at start)

If review was decided in Context Analysis:
1. Initialize `PLAN_REVIEW_ITERATIONS=0`
2. Invoke `/flow-code:plan-review` with the epic ID
3. If review returns "Needs Work" or "Major Rethink":
   - Increment `PLAN_REVIEW_ITERATIONS`
   - **If `PLAN_REVIEW_ITERATIONS >= 2`**: stop the loop. Log: "Plan review: 2 iterations completed. Proceeding." Go to Step 8.
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

## Step 8: Execute or Offer next steps

**Default: auto-execute immediately.** Do NOT show a menu or wait for user input unless `--plan-only` was specified.

```bash
TASK_COUNT=$($FLOWCTL tasks --epic <epic-id> --json | python3 -c "import json,sys; print(json.load(sys.stdin)['count'])")
```

**If `--plan-only` was specified:**
```
Plan created: <epic-id> (N tasks)
Next: /flow-code:work <epic-id>
```
Stop here. Do NOT auto-execute.

**Otherwise (default — auto-execute):**

Persist the intent to execute so interrupted handoffs are detected:
```bash
$FLOWCTL epic set-auto-execute <epic-id> --pending --json
```

Show one-line summary, then invoke work immediately:
```
Epic <epic-id>: "<title>" (N tasks) — executing...
```

- **≤ 10 tasks**: Invoke `/flow-code:work <epic-id> --no-review` directly in this session.
- **> 10 tasks**: Print recommendation:
  ```
  Epic has N tasks — recommend using Ralph for fresh context per task:
    /flow-code:ralph-init
  Or: /flow-code:work <epic-id>
  ```

**CRITICAL: Do NOT show an options menu. Do NOT wait for user selection. The default behavior is to proceed to work immediately.**
