---
name: flow-code-context-eng
description: "Use when context window is filling up, agent output quality drops, or starting a new complex task that needs careful context loading"
tier: 2
user-invocable: true
---

# Context Engineering

## Overview

Context engineering is about loading the RIGHT information at the RIGHT time — not "load everything." The context window is finite, attention degrades with noise, and wrong context is worse than no context. This skill teaches strategic context management: what to load, when to load it, and when to prune.

For RepoPrompt / MCP-specific workflow ownership, use `skills/_shared/rp-mcp-orchestration.md` as the canonical guide. This skill focuses on *context strategy* and should not be treated as a competing builder/oracle protocol.

## When to Use

- Starting a new session or complex task that spans multiple files
- Agent output quality is declining (wrong patterns, hallucinated APIs, ignored conventions)
- Switching between different parts of a codebase
- Agent ignores project conventions despite them being documented
- Context window pressure — responses are getting slower or less accurate
- Planning a review that needs cross-file understanding

**When NOT to use:**
- Simple single-file edits where the file is already loaded
- Tasks where the relevant context is already in the conversation
- Quick fixes where the error message contains all needed information

## Core Process

### Phase 1: Audit Current Context

Before loading anything new, assess what you already have.

**Ask these questions:**
1. What files/specs are currently loaded in this conversation?
2. What information is missing for the current task?
3. What loaded context is stale or irrelevant (from a previous task)?
4. How deep into the conversation are we? (stale context risk increases with depth)

```
Current context audit:
  Loaded:  [list what's in the conversation]
  Missing: [what the task needs but isn't loaded]
  Stale:   [what was loaded for a previous task]
  Action:  [load X, prune Y, refresh Z]
```

### Phase 2: Apply the 5-Tier Hierarchy

Structure context from most persistent to most transient. Higher tiers have higher priority — when the window is tight, cut from the bottom.

```
┌─────────────────────────────────────────┐
│  Tier 1: Rules files                    │ ← Persistent, highest priority
│  (CLAUDE.md, .cursorrules)              │
├─────────────────────────────────────────┤
│  Tier 2: Specs & architecture           │ ← Per-feature
│  (SPEC.md, ADRs, epic specs)            │
├─────────────────────────────────────────┤
│  Tier 3: Source files                   │ ← Per-task
│  (relevant code, types, tests)          │
├─────────────────────────────────────────┤
│  Tier 4: Error output                   │ ← Per-iteration
│  (test failures, build errors, logs)    │
├─────────────────────────────────────────┤
│  Tier 5: Conversation history           │ ← Accumulating, lowest priority
│  (previous messages, prior attempts)    │
└─────────────────────────────────────────┘
```

**Tier 1 — Rules files:** Always loaded, never pruned. This is persistent context that survives across sessions. If it's not in the rules file, it doesn't exist for the agent.

**Tier 2 — Specs & architecture:** Load the relevant section only. "Here's the auth section of our spec" beats "here's our entire 5000-word spec" when working on auth.

**Tier 3 — Source files:** Load before editing. Read the file, its tests, one example of the pattern, and relevant type definitions. Use `structure` commands for signatures before committing to full file reads.

**Tier 4 — Error output:** Feed specific errors, not entire logs. One failing test's traceback, not 500 lines of test output.

**Tier 5 — Conversation history:** Compresses and degrades over time. Start fresh sessions when switching major features. Summarize progress when context is getting long.

### Phase 3: Optimize CLAUDE.md

CLAUDE.md is the highest-leverage context in any project. 10 lines there outweigh 1000 lines in conversation.

**A good CLAUDE.md covers:**
- Tech stack and versions
- Build/test/lint commands (exact invocations)
- Code conventions (with one example)
- Boundaries (what NOT to do)
- Project-specific patterns

**Audit checklist:**
- [ ] Does the CLAUDE.md exist?
- [ ] Does it cover the commands an agent needs to run?
- [ ] Does it show the conventions the agent keeps violating?
- [ ] Is it under 200 lines? (longer = lower compliance)

If the agent keeps making the same mistake, the fix is almost always adding a line to CLAUDE.md — not correcting the agent in conversation.

### Phase 4: Use Existing Tools Strategically

Do not reinvent context gathering. flow-code has purpose-built tools for each context need.

| Need | Tool | When |
|------|------|------|
| Deep codebase understanding | `context-scout` agent | Before planning or major implementation |
| Broad multi-file discovery | `context_builder` | Default when scope is architectural, review-oriented, or otherwise too broad for direct reads |
| RP-guided exploration on explicit request | `flow-code-rp-explorer` skill | When the user explicitly wants RepoPrompt-style exploration guidance |
| Deep reasoning over selected context | `ask_oracle` / Oracle chat | After a builder run or a deliberate selection update |
| Cross-model review context | `flow-code-export-context` skill | When preparing context for external LLM review or handoff |
| Parallel delegated work | `agent_run` | When the task should move to a separate worker/session |
| Code signatures without full reads | `rp-cli structure` or `get_code_structure` | When you need function/type shapes, not full files |
| Targeted file search | `rp-cli search` or `file_search` | When you know what pattern to find |

**Decision tree:**

```
Need to understand a feature?
  ├─ Know the files → Read them directly (Tier 3)
  ├─ Know the area → structure + targeted search
  └─ Don't know where to start → context-scout agent or builder

Need cross-file analysis?
  ├─ Architecture question → context_builder(response_type="question")
  ├─ Planning / implementation → context_builder(response_type="plan")
  ├─ Review → context_builder(response_type="review")
  └─ Follow-up reasoning on selected context → ask_oracle / same Oracle chat

Need to share context externally?
  └─ flow-code-export-context skill

Need a separate parallel worker or isolated session?
  └─ agent_run
```

### Phase 5: Monitor and Prune

Context management is not set-and-forget. Monitor throughout the session.

**Prune triggers:**
- Switched to a different task → remove files from the old task
- Error was fixed → remove the error output and failed attempts
- Conversation exceeds ~50 messages → consider fresh session
- Agent starts hallucinating APIs → context is stale, reload source files

**Token budgeting principles:**
- Signatures (structure) cost ~500 tokens vs ~5000 for full files (10x savings)
- Slice reads (`--start-line --limit`) cost ~300 tokens for 50 lines
- Prefer structure first, full read only for files you will edit
- Never dump full files for context — use signatures + targeted slices

**Fresh session checklist:**
1. Summarize progress so far (what's done, what's next)
2. Note any decisions or conventions discovered
3. Start new session with the summary + relevant Tier 1-2 context
4. Reload only the Tier 3 files needed for the next task

## Common Rationalizations

| Excuse | Reality |
|--------|---------|
| "I'll just load everything" | Context windows are finite. Irrelevant context dilutes signal and degrades output quality. Precision beats volume. |
| "CLAUDE.md doesn't matter much" | It's the highest-leverage file in any project. 10 lines in CLAUDE.md prevent more mistakes than 100 lines of in-conversation correction. |
| "I can hold it all in my head" | Context compresses and degrades. What you loaded 20 messages ago may be effectively gone. Verify, don't assume. |
| "More context is always better" | Research shows performance degrades with excess instructions. Wrong context is actively worse than no context — it introduces false patterns. |
| "I'll read files when I need them" | Reactive loading causes mid-task quality drops. Proactive context loading at task start prevents the "forgot the convention" class of errors entirely. |
| "The context window is huge, I'll use it all" | Window size is not attention budget. A 200K window with 190K of noise performs worse than a 50K window with 50K of signal. |
| "I don't need to start a fresh session" | Stale context from previous tasks actively misleads. The cost of reloading is minutes; the cost of stale context is hours of wrong-direction work. |

## Red Flags

- Agent output stops following project conventions that are documented in CLAUDE.md
- Agent invents APIs, imports, or utilities that don't exist in the codebase
- Agent re-implements something that already exists (failed to load existing code)
- Quality visibly degrades as conversation gets longer
- Agent gives generic advice instead of project-specific answers
- Same correction given to the agent more than twice in one session
- No rules file exists in the project (context starvation guaranteed)
- Full files dumped into context when only a function signature was needed

## Verification

After applying context engineering, confirm:

- [ ] CLAUDE.md exists, is current, and covers tech stack, commands, conventions, and boundaries
- [ ] Only task-relevant files are loaded (no leftover context from previous tasks)
- [ ] Agent output references actual project files and APIs (not hallucinated ones)
- [ ] Structure/signatures used before full file reads (token efficiency)
- [ ] Context was refreshed when switching between major tasks
- [ ] Error output is specific (single failure, not full logs)
- [ ] Session was restarted if conversation exceeded ~50 messages on different topics
