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

## Pre-Scout Quick Context

This is the mandatory first pass before scouts or RP depth. Use the cached graph/index first, then escalate only if needed.

Before spawning scouts, gather initial context using intent-level commands:

```bash
# If this repo was initialized before auto-bootstrap existed, backfill artifacts once:
# $FLOWCTL graph build --json

# Project structure overview (instant from cached graph)
$FLOWCTL graph map --json

# Find related code
$FLOWCTL find "<key terms from request>" --json

# Check what would be impacted
$FLOWCTL graph impact <likely-changed-file> --json
```

For exact regex with context, use native `Grep`. For file patterns, use native `Glob`.

Feed results into scout prompts for targeted exploration. On small/trivial requests, these commands may be sufficient without invoking deeper RP context.

## Scout Selection: AI Decides Per-Request

### Scout Decision Guide

- **Always**: `repo-scout` (fast grep-based research). `memory-scout` if memory.enabled. `capability-scout` unless `--no-capability-scan` passed (non-blocking; fails open — planning continues if it errors).
- **Deep context** (replaces `context-scout` in this guide — exactly one runs per plan, not multiple):
  - **Tier 1** (MCP available): direct `context_builder(response_type:"plan")` call — best quality, automatic workspace binding
  - **Tier 2** (rp-cli available, no MCP): `rp-cli -e 'builder "<request + repo-scout findings>" --response-type plan'` (timeout: 300s)
  - **Tier 3** (neither available): `context-scout` subagent (existing behavior, unchanged)
- **Add when needed**: `practice-scout` for security/auth/payments/concurrency. `docs-scout` for external APIs/libraries. `github-scout` for novel patterns (requires scouts.github). `epic-scout` if 2+ open epics. `docs-gap-scout` if user-facing changes. `flow-gap-analyst` — maps user flows, edge cases, and missing requirements from the spec.
- **Constraints**: min 1 (repo-scout required), max 7. Run ALL selected scouts via RP agent_run in parallel. Deep context (Tier 1/2/3) runs AFTER repo-scout returns — it uses repo-scout findings as input.

### Scout Spawning via RP agent_run

All scouts are spawned via RP `agent_run` with `explore` role:

```
# For EACH selected scout:
mcp__RepoPrompt__agent_run({
  op: "start",
  model_id: "explore",
  session_name: "scout-<scout-name>-<epic-id>",
  message: "<scout prompt from agents/<scout-name>.md with request context, file references, and output format>",
  detach: true
})
```

Save returned `session_id` for each scout. After all scouts are started, wait for completion:

```
mcp__RepoPrompt__agent_run({
  op: "wait",
  session_ids: [<all scout session IDs>],
  timeout: 180
})
```

Parse output as scout Markdown with `json:scout-summary` block.

Save scout session IDs into `ALL_SESSION_IDS` for batch cleanup at Close phase (Step 5.5).

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

## Deep Context via RP (PARALLEL with scouts)

Launch `context_builder` **at the same time** as scouts — don't wait for scouts first. Both run concurrently:

```bash
RP_TIER=$($FLOWCTL rp tier)  # or: $FLOWCTL rp tier --mcp-hint
```

**If RP_TIER is `mcp`** — start context_builder concurrently with scouts:
```
# This runs WHILE scouts are still executing
context_builder(
  instructions: "<request summary + initial flowctl find results>",
  response_type: "plan"
)
→ save chat_id for follow-up via oracle_send
```

**If RP_TIER is `cli`**: Run `rp-cli -e 'builder "..." --response-type plan'` (timeout 300s)
**If RP_TIER is `none`**: Run `context-scout` as a subagent

**After scouts complete**, if context_builder already returned → merge both results. If context_builder is still running → wait for it, or use `oracle_send` to feed scout findings into the ongoing context:

```
oracle_send(
  chat_id: "<from context_builder>",
  message: "Scout findings to incorporate: <repo-scout key references and gaps>"
)
```

**Skip condition:** If the request is trivial (clear bug fix, single-file change, S-size task), skip deep context — scouts alone are sufficient.

Feed context_builder + scout findings into the epic spec together.

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

> **Deduplication note:** Memory is injected here at plan time for full research context. Workers also inject memory in Phase 2 via `flowctl memory inject --json`. Both use the same full memory store — the difference is that plan uses memory for broad research, while workers focus on their specific task. No deduplication needed because the usage context differs.

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
