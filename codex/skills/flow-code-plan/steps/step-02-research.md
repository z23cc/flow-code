# Step 2: Fast Research (Parallel Scouts)

**IMPORTANT**: Steps 4-9 (research, gap analysis, depth) ALWAYS run regardless of input type.

## Resolve Flow IDs First

**If input is a Flow ID** (fn-N-slug or fn-N-slug.M, including legacy fn-N/fn-N-xxx): First fetch it with `$FLOWCTL show <id> --json` and `$FLOWCTL cat <id>` to get the request context.

## Check Config Flags and Stack Profile

```bash
$FLOWCTL config get memory.enabled --json
$FLOWCTL config get scouts.github --json
$FLOWCTL stack show --json
```

## Read Project Context

Read `.flow/project-context.md` if it exists. Use Non-Goals to scope out excluded approaches. Use File Conventions to auto-assign task domains. Use Architecture Decisions to avoid proposing alternatives to settled choices.

## Read Architecture Decision Records

```bash
ls docs/decisions/ADR-*.md 2>/dev/null
```

If ADRs exist, scan their YAML frontmatter and Decision sections. Key constraints:
- **Rejected Alternatives**: Do NOT propose these approaches again
- **Constraints** (in Consequences): Rules all tasks must follow
- **Scope**: Which files are governed by which ADR
- If a new task would conflict with an accepted ADR, either respect the ADR or propose superseding it with a new ADR

## Check Architecture Invariants

```bash
$FLOWCTL invariants show --json
```

If invariants exist, ensure all planned tasks respect them. If a task would violate an invariant, note the conflict in the task spec and flag it.

Stack is auto-detected on `init`. If present, use it throughout planning:
- Include framework/language in scout prompts (e.g., "Django DRF patterns", "Next.js App Router")
- Use `stack.*.conventions` to guide task spec writing
- Put `$FLOWCTL guard` in epic's Quick commands section (replaces manual test/lint commands)
- Tag task specs with which stack layer they belong to (backend/frontend/infra) in the Files field

## Pre-Scout Search (flowctl tools)

Before spawning scouts, use flowctl's built-in search tools for fast initial discovery:

```bash
# Fuzzy file search with frecency + git status ranking
$FLOWCTL search "<key terms from request>" --limit 20 --json

# Trigram indexed content search (if index exists)
$FLOWCTL index search "<key terms from request>" --limit 20 --json

# Project structure overview
$FLOWCTL repo-map --budget 512 --json

# Symbol extraction for key directories
$FLOWCTL code-structure extract --path <relevant-dir> --json
```

Feed these results into scout prompts for more targeted exploration. Skip if the request is trivial (single-file, clear location).

## Scout Selection: AI Decides Per-Request

### Scout Decision Guide

- **Always**: `repo-scout` (fast grep-based research). `memory-scout` if memory.enabled. `capability-scout` unless `--no-capability-scan` passed (non-blocking; fails open — planning continues if it errors).
- **Deep context** (replaces `context-scout` in this guide — exactly one runs per plan, not multiple):
  - **Tier 1** (MCP available): direct `context_builder(response_type:"plan")` call — best quality, automatic workspace binding
  - **Tier 2** (rp-cli available, no MCP): `rp-cli -e 'builder "<request + repo-scout findings>" --response-type plan'` (timeout: 300s)
  - **Tier 3** (neither available): `context-scout` subagent (existing behavior, unchanged)
- **Add when needed**: `practice-scout` for security/auth/payments/concurrency. `docs-scout` for external APIs/libraries. `github-scout` for novel patterns (requires scouts.github). `epic-scout` if 2+ open epics. `docs-gap-scout` if user-facing changes. `flow-gap-analyst` — maps user flows, edge cases, and missing requirements from the spec.
- **Constraints**: min 1 (repo-scout required), max 7. Run ALL selected scouts in ONE parallel Agent/Task call. Deep context (Tier 1/2/3) runs AFTER repo-scout returns — it uses repo-scout findings as input.

### Must Capture

- File paths + line refs
- Existing centralized code to reuse
- Similar patterns / prior work
- External docs links
- Project conventions (AGENTS.md, CONTRIBUTING, etc)
- Architecture patterns and data flow
- Epic dependencies (from epic-scout)
- Doc updates needed (from docs-gap-scout) - add to task acceptance criteria
- Capability gaps (from capability-scout) - persist in Step 10 (see below)

### Scout Output Parsing

Each scout returns Markdown with a `json:scout-summary` block at the end. Parse this block to extract structured data:

```
references[]     -> populate task Investigation targets (Required files)
reusable_code[]  -> add to task Key context ("Reuse: path/export — usage")
conventions[]    -> apply to epic spec Project Conventions section
gaps[]           -> feed to gap analyst, add to Open Questions
```

If a scout returns no `json:scout-summary` block (legacy format), fall back to parsing Markdown sections manually (References, Reusable Code, Gaps).

## Deep Context via RP (After Repo-Scout)

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

## Apply Memory Lessons (if memory.enabled)

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
- **Pitfalls** -> add as explicit warnings in task specs or acceptance criteria ("Verify X does not regress Y")
- **Conventions** -> ensure tasks follow discovered patterns, reference them in spec
- **Decisions** -> respect past architectural choices unless the plan explicitly supersedes them

**Rules:**
- Don't bloat tasks with every memory entry — only apply entries clearly relevant to this plan
- If a past decision conflicts with the current plan, note it as an explicit "supersedes decision #N" in the epic spec
- 0-3 applied entries per plan is normal

## Next Step

Read `steps/step-03-gap-analysis.md` and execute.
