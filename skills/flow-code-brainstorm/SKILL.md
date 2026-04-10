---
name: flow-code-brainstorm
description: "Use when exploring requirements before planning. Pressure-tests ideas, generates approaches, and outputs a requirements doc for /flow-code:plan. Supports --auto mode for AI self-interview (no human input needed)."
tier: 3
user-invocable: false
---

# Flow brainstorm

Explore and pressure-test an idea before committing to a plan. Outputs a requirements doc that feeds directly into `/flow-code:plan`.

If you already know you want a durable requirements artifact and do not need the broader pressure-test flow, prefer `/flow-code:spec`.

`/flow-code:brainstorm` intentionally produces a broader requirements doc with extra evidence / self-interview trace, while `/flow-code:spec` is the leaner artifact-first path.

**Two modes:**
- **Interactive** (default when invoked standalone via `/flow-code:brainstorm`): asks user questions via `AskUserQuestion`
- **Auto** (`--auto`, or when invoked from `/flow-code:go` pipeline): AI self-interview — analyzes codebase, asks itself questions, answers from code evidence, produces refined spec with zero human input

**Pipeline auto-detection**: If this skill is invoked as part of the `/flow-code:go` pipeline (detected by: epic already exists, or `flow-code-run` is the caller), ALWAYS use Auto mode regardless of flags. The go pipeline has a zero-interaction contract.

**IMPORTANT**: This plugin uses `.flow/` for ALL task tracking. Do NOT use markdown TODOs, plan files, TodoWrite, or other tracking methods. All task state must be read and written via `flowctl`.

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** `which flowctl` will fail (expected). Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
$FLOWCTL <command>
```

## Pre-check: Local setup version

If `.flow/meta.json` exists and has `setup_version`, compare to plugin version:
```bash
SETUP_VER=$(jq -r '.setup_version // empty' .flow/meta.json 2>/dev/null)
# Portable: Claude Code uses .claude-plugin, Factory Droid uses .factory-plugin
PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.claude-plugin/plugin.json"
[[ -f "$PLUGIN_JSON" ]] || PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.factory-plugin/plugin.json"
PLUGIN_VER=$(jq -r '.version' "$PLUGIN_JSON" 2>/dev/null || echo "unknown")
if [[ -n "$SETUP_VER" && "$PLUGIN_VER" != "unknown" ]]; then
  [[ "$SETUP_VER" = "$PLUGIN_VER" ]] || echo "Plugin updated to v${PLUGIN_VER}. Run /flow-code:setup to refresh local scripts (current: v${SETUP_VER})."
fi
```
Continue regardless (non-blocking).

**Role**: product strategist, requirements explorer
**Goal**: pressure-test ideas before planning to avoid wasted implementation effort

## Input

Full request: $ARGUMENTS

Accepts:
- Feature/bug description in natural language
- `--auto` flag: enable AI self-interview mode (no human questions)
- Empty: ask "What idea or problem should we brainstorm? Describe it in 1-5 sentences."

Examples:
- `/flow-code:brainstorm Add real-time collaboration to the editor`
- `/flow-code:brainstorm --auto migrate from REST to GraphQL`
- `/flow-code:brainstorm --auto We keep getting auth token expiry bugs`
- `/flow-code:brainstorm We keep getting auth token expiry bugs`

## Workflow

Execute steps from `steps/` directory one at a time (JIT loading — only read the current step):
1. Read `steps/step-01-mode-detect.md` and execute
2. Read `steps/step-02-context-gather.md` and execute
3. Read `steps/step-03-self-interview.md` and execute
4. Read `steps/step-04-approaches.md` and execute
5. Read `steps/step-05-requirements.md` and execute

---

## Mode Detection

Parse `$ARGUMENTS` for `--auto` flag:
- If `--auto` present: remove flag from arguments, set AUTO_MODE=true
- Otherwise: AUTO_MODE=false (interactive, original behavior)

## Phase 0: Codebase Context Gathering

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

**Always run (both modes):** Read relevant code, git log, and project structure to understand scope.

**In auto mode, gather deep context:**
1. Search for files related to the request (`file_search` via RP MCP, or Grep/Glob as fallback)
2. Read git log for recent changes in relevant areas
3. Check existing `.flow/` specs/epics for related work
4. Read key config files, README, CLAUDE.md for project constraints
5. Identify affected modules, dependencies, and integration points

Classify complexity:

### Trivial (1-2 files, clear fix, well-understood change)
- **Interactive**: Skip brainstorm, suggest `/flow-code:plan` directly.
- **Auto**: Skip brainstorm, suggest `/flow-code:plan` directly.

### Medium (clear feature, moderate scope)
- **Interactive**: quick brainstorm (3 pressure-test questions + 2 approaches)
- **Auto**: self-interview with 6 Q&A pairs + 2 approaches

### Large (cross-cutting, vague requirements, multiple systems affected)
- **Interactive**: full brainstorm (all phases, 3 approaches)
- **Auto**: deep self-interview with 10+ Q&A pairs + 3 approaches + risk matrix

Tell the user which tier and mode. One sentence.

---

## Interactive Mode (AUTO_MODE=false)

Original behavior — ask user questions via `AskUserQuestion`.

### Phase 1: Pressure Test

Ask exactly 3 questions, **one at a time**, using `AskUserQuestion` for each.

**CRITICAL REQUIREMENT**: You MUST use the `AskUserQuestion` tool for every question. Do NOT output questions as plain text — they will be ignored.

Wait for each answer before asking the next question.

#### Question 1: Who and why?
> Who uses this? What's the specific pain point or motivation?

#### Question 2: Cost of inaction?
> What happens if we do nothing? What's the actual cost or risk?

#### Question 3: Simpler framing?
> Is there a simpler version that delivers 80% of the value? What's the minimum viable version?

After all 3 answers, summarize the key insights in 2-3 bullets before proceeding.

### Phase 2: Approach Generation

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

### Phase 3: Requirements Output

→ Jump to [Write Requirements Doc](#write-requirements-doc)

---

## Auto Mode (AUTO_MODE=true)

AI self-interview — no `AskUserQuestion` calls. All answers derived from codebase analysis, best practices, and reasoning.

**Output contract (auto mode):**
1. Print Q&A pairs to **stdout** so the user sees the reasoning in conversation
2. Embed Q&A pairs in the requirements doc under a `## Self-Interview Trace` section
3. Requirements doc written to `.flow/specs/${SLUG}-requirements.md` (same as interactive)

### Phase A1: Deep Code Analysis

Before self-interview, gather evidence:

1. **Affected surface**: `file_search` (RP MCP) or Grep/Glob (fallback) for all files related to the request. List them.
2. **Current patterns**: How does the codebase currently handle similar functionality? Read 3-5 key files.
3. **Dependencies**: What modules/packages/APIs are involved? Check imports, configs.
4. **Test coverage**: Do tests exist for the affected area? What kind?
5. **Recent history**: `git log --oneline -20` on affected files — who changed what, why?
6. **Existing specs**: Check `.flow/specs/` and `.flow/epics/` for related prior work.

### Phase A2: Self-Interview

Ask and answer questions in structured Q&A format. Output each as a visible block:

```
### Q: <question>
**A:** <answer with code evidence>
```

**Core questions (always ask all):**

#### 1. Problem & Users
> Q: Who uses this and what specific pain point does it solve?
> A: Derive from codebase context — who calls the affected code, what user-facing behavior it impacts.

#### 2. Cost of Inaction
> Q: What happens if we do nothing? What breaks or degrades?
> A: Check for open issues, error patterns, performance trends, tech debt signals in the code.

#### 3. Simpler Framing
> Q: Is there a simpler version that delivers 80% of the value?
> A: Analyze the request — what's the minimum change that solves the core problem? What can be deferred?

#### 4. Existing Patterns
> Q: How does the codebase currently handle similar problems?
> A: Cite specific files, functions, patterns found in Phase A1. Quote code if relevant.

#### 5. Integration Points
> Q: What other systems/modules will this touch? What contracts must be preserved?
> A: List APIs, shared types, database schemas, config files that are affected.

#### 6. Edge Cases & Failure Modes
> Q: What can go wrong? What are the boundary conditions?
> A: Analyze error handling in current code, identify missing cases, concurrency risks.

**Extended questions (Large tier only):**

#### 7. Performance Impact
> Q: Will this change affect latency, memory, or throughput?
> A: Analyze hot paths, data volume, caching layers in affected code.

#### 8. Security Surface
> Q: Does this introduce or modify authentication, authorization, or data handling?
> A: Check for auth middleware, input validation, sensitive data flows.

#### 9. Migration & Compatibility
> Q: Are there breaking changes? Do we need data migration or feature flags?
> A: Check API contracts, database schemas, config formats for backwards compatibility.

#### 10. Testing Strategy
> Q: What test types are needed and what's the current coverage gap?
> A: Analyze existing test files for the affected area, identify missing test categories.

**Adaptive follow-ups**: If any answer reveals unexpected complexity (e.g., a shared module with 10+ consumers, no test coverage, concurrency issues), add 1-2 follow-up Q&A pairs to drill into that specific area. Cap at 15 total Q&A pairs.

## Structured Deepening

After self-interview completes, apply 1-2 named reasoning methods to pressure-test the output:

### Method Selection Guide
| Method | Best for | Prompt |
|--------|----------|--------|
| **Pre-mortem Analysis** | Specs, plans, new features | "Assume this shipped and failed 6 months later. What are the 3 most likely causes?" |
| **First Principles** | Architecture, major refactors | "Strip all assumptions. What's the simplest possible solution from ground truth?" |
| **Inversion** | Risk assessment, refactoring | "How would you guarantee this fails? Now avoid those things." |
| **Red Team** | Security, APIs, public surfaces | "You're an attacker. How do you break this?" |
| **Constraint Removal** | Innovation, scope decisions | "Remove all constraints (time, tech, team). What changes? What stays the same?" |
| **Stakeholder Mapping** | Multi-user features | "Re-evaluate from each stakeholder's perspective. Who loses?" |

### Auto-Selection Rules
- For spec/plan tasks → Pre-mortem (default)
- For architecture tasks → First Principles
- For refactoring tasks → Inversion
- For security-sensitive → Red Team
- For scope decisions → Constraint Removal

### Execution
1. Auto-select the most relevant method based on task type
2. Apply the method's prompt to the current brainstorm output
3. Append insights to requirements doc under "## Deepening Insights" section
4. If insights reveal significant gaps, re-run self-interview for those specific areas

### Phase A3: Approach Generation

Same as interactive Phase 2, but **AI picks the best approach** instead of asking user:

Generate 2-3 approaches with the same table format (Name/Summary/Effort/Risk/Pros/Cons).

**Auto-select logic** — pick the approach that:
1. Aligns best with existing codebase patterns (don't fight the codebase)
2. Has lowest risk for the effort level
3. Maximizes reuse of existing code

Output: "**Selected: Approach N** — <one-line reason based on code evidence>"

If approaches are genuinely close (risk/effort within one level), flag it:
> "Approaches N and M are close calls. Defaulting to N (<reason>). Override by re-running without --auto."

### Phase A4: Requirements Output

→ Jump to [Write Requirements Doc](#write-requirements-doc)

---

## Write Requirements Doc

Based on the chosen/selected approach, write a requirements document:

```bash
# Generate slug from the idea
SLUG=$(echo "$IDEA" | tr '[:upper:]' '[:lower:]' | tr ' ' '-' | tr -cd 'a-z0-9-' | head -c 40)

# Ensure .flow/specs/ exists
mkdir -p .flow/specs

# Write requirements doc
```

Write to `.flow/specs/${SLUG}-requirements.md`:

```markdown
# Requirements: <Title>

## Problem
<1-2 sentences — from user answers (interactive) or code-derived analysis (auto)>

## Users
<Who uses this — from Q1 answer>

## Chosen Approach
<Name and summary of selected approach>

## Requirements
- [ ] <Requirement 1>
- [ ] <Requirement 2>
- [ ] <Requirement 3>
...

## Non-Goals
- <What this explicitly does NOT include>

## Constraints
- <Technical or business constraints identified during brainstorm>

## Evidence
<Auto mode only — key files and patterns that informed decisions>
- `path/to/file.rs:42` — <what it shows>
- `path/to/other.rs` — <pattern found>

## Self-Interview Trace
<Auto mode only — full Q&A pairs for auditability>

### Q: <question 1>
**A:** <answer with code evidence>

### Q: <question 2>
**A:** <answer with code evidence>

...

## Open Questions
- <Anything unresolved that /flow-code:plan should address>
```

**Interactive mode**: omit Evidence and Self-Interview Trace sections.

After writing the file, tell the user:

```
Requirements written to .flow/specs/<slug>-requirements.md

Next step: Run /flow-code:plan .flow/specs/<slug>-requirements.md
```

**Auto mode additionally**: show a one-paragraph summary of the self-interview findings and the selected approach, so the user can quickly validate without reading the full doc.

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "I already know what to build" | Premature certainty is the most expensive mistake. Brainstorming surfaces assumptions you didn't know you had. |
| "Brainstorming is just talking, not real work" | Brainstorming produces the requirements doc that drives everything downstream. Skip it and you plan against assumptions. |
| "We don't have time to explore alternatives" | Exploring 3 approaches for 15 minutes is cheaper than rebuilding after choosing wrong on day one. |
| "The first idea is usually right" | First ideas are anchored on recent experience. Pressure-testing reveals options that outperform the obvious choice. |
| "Requirements are already in the ticket" | Tickets describe what someone wants, not what should be built. Brainstorming translates desire into actionable constraints. |
| "Let's just start and iterate" | Iteration without direction is wandering. Brainstorming sets the constraints that make iteration productive. |
| "This is too small to brainstorm" | Small scope doesn't mean small risk. A 5-minute pressure test on a "simple" feature often reveals hidden complexity. |
| "Auto mode can't know business context" | True — but it knows code context deeply. Use auto for technical refinement, interactive for business discovery. |

## Red Flags

- Jumping straight to /flow-code:plan without exploring requirements
- Only one approach considered (no alternatives generated or evaluated)
- Requirements doc has zero constraints or non-goals listed
- "Open Questions" section is empty (every problem has unknowns)
- Brainstorm output restates the original request without adding new insight
- User's actual problem never identified (solution proposed without understanding need)
- Auto mode answers not grounded in code evidence (speculation without file references)
