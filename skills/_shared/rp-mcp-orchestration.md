# Canonical RP/MCP Orchestration Guide

This file is the repo's default source of truth for RepoPrompt / MCP workflow guidance.

Use it to answer:
- when to call `context_builder` versus direct search/read/git tools;
- what `ask_oracle` / `oracle_send` can and cannot do;
- when `manage_selection` is appropriate;
- how `prompt` / `workspace_context` exports fit;
- when to use `agent_run` for a separate worker or delegated session.

If another skill or doc restates RP/MCP behavior, this file wins unless that surface is defining a clearly narrower workflow-specific rule.

## Tool-name map

Different surfaces use slightly different names for the same roles:

| Role | Canonical name in this repo | Common aliases in older RP-managed docs |
|---|---|---|
| Broad discovery + initial selection | `context_builder` | `builder`, discovery agent |
| Deep reasoning over the current selection | `ask_oracle` | `oracle_send`, `chat`, oracle |
| Selection curation | `manage_selection` | selection / select ops |
| Export or inspect current prompt/context | `prompt`, `workspace_context` | prompt export, context export |
| Separate delegated session | `agent_run`, `agent_manage` | session / agent ops |

## Default ownership rules

1. **Use direct tools for obvious local facts.**  
   If `file_search`, `read_file`, `get_code_structure`, or `git` can answer the question directly, do that yourself.

2. **Use `context_builder` for broad discovery and the initial curated selection.**  
   This is the default when scope is unknown, multi-file, architectural, review-oriented, or likely to benefit from discovery. Keep pre-builder exploration brief.

3. **Use `ask_oracle` only after selection exists.**  
   Oracle follow-ups reason over the current selection and prior chat state; they do not discover missing files on their own.

4. **Continue the same Oracle chat by default.**  
   If you already have a `chat_id`, reuse it for clarification, re-review, or follow-up synthesis unless you intentionally need a fresh thread.

5. **Use `manage_selection` only for small, targeted refinements.**  
   Add or remove a few files or slices when you know exactly what is missing. If coverage is materially wrong, rerun `context_builder` with better instructions instead of hand-curating a large replacement.

6. **Do not use Oracle as the editor.**  
   Once the plan or review is clear, implement directly with editing tools. Oracle helps with reasoning, not with making the final edits for you.

7. **Use `prompt` / `workspace_context` for handoff or explicit inspection, not as mandatory post-builder steps.**  
   For builder-driven exports, trust the generated selection/prompt unless you noticed a concrete issue.

8. **Use `agent_run` only when you truly need another session.**  
   Reach for a delegated worker when you want parallel execution, isolation, or a separate agent lifecycle. Do not substitute `agent_run` for same-session builder/oracle reasoning.

## Workflow matrix

| Goal | Default path |
|---|---|
| Question / investigation | Brief orientation → `context_builder(response_type="question")` or direct tools if clearly local → `ask_oracle` only for synthesis over selected context |
| Build / implementation | Brief orientation → `context_builder(response_type="plan")` → direct edits → same-chat `ask_oracle` only if selected-context reasoning gap remains |
| Review | Confirm review scope first → `context_builder(response_type="review")` with explicit compare target → same-chat Oracle follow-up for clarification or re-review |
| Export for external model | Extract the underlying task → usually `context_builder(response_type="clarify")` → `prompt export` |
| Delegation / parallel work | Prepare/export plan or review context if needed → `agent_run` / `agent_manage` for the separate session |

## Review-specific rules

- Confirm the comparison scope before starting a deep review.
- Put the explicit compare target in the builder instructions.
- After fixes, prefer a same-chat Oracle re-review.
- Rerun `context_builder` only if the review scope changed materially or a new area truly was not covered.

## Export-specific rules

- Strip meta-framing like “write a prompt about…” and pass the underlying task.
- For review exports, explicitly say **code review** and include the compare scope.
- Use a repo-local export path by default.
- After builder-based export, do not reflexively reopen the prompt, selection, or token state unless you noticed a concrete problem.

## Anti-patterns

- Doing deep manual repo exploration before a builder call that should have owned discovery
- Asking Oracle questions about files that are not in the current selection
- Clearing or replacing builder-created selection without a very deliberate reason
- Re-running `context_builder` when a same-chat Oracle follow-up would answer the real question
- Using `manage_selection` as a substitute for better builder instructions
- Using `agent_run` when the task only needs same-session reasoning
- Treating exported prompt text as something to automatically rewrite after a builder-generated export

## Maintainer note

When updating RP/MCP docs, prefer linking here and only document the workflow-specific delta in each skill or protocol file.

Extended by: `skills/_shared/rp-review-protocol.md`, `skills/flow-code-context-eng/SKILL.md`, `skills/flow-code-rp-explorer/SKILL.md`, `skills/flow-code-export-context/SKILL.md`, and the `.claude/skills/rp-*` guidance surfaces.
