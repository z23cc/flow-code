---
name: flow-code-rp-explorer
description: "Use when user says 'use rp to...' or 'use repoprompt to...' followed by explore, find, understand, search, or similar actions."
tier: 1
user-invocable: true
---

# RP-Explorer

Use when the user explicitly wants RepoPrompt-driven exploration.

Canonical orchestration guidance lives in `skills/_shared/rp-mcp-orchestration.md`.

## Default Mode

Prefer in-session RepoPrompt MCP tools when available. Use `rp-cli` only when:
- the user explicitly asks for CLI commands; or
- MCP RepoPrompt tools are unavailable in the current host.

## Trigger Phrases

Activates when user combines "use rp" or "use repoprompt" with an action:
- "use rp to explore how auth works"
- "use repoprompt to find similar patterns"
- "use rp to understand the data flow"
- "use repoprompt to search for API endpoints"

## Default Workflow

1. **Orient briefly** with `get_file_tree`, `file_search`, or `get_code_structure`.
2. **Use `context_builder` for broad discovery** when the request is cross-file, architectural, or otherwise hard to answer from a couple of direct reads.
3. **Use `ask_oracle` only after selection exists** and only for synthesis over the selected context.
4. **Use `manage_selection` only for targeted add/remove/slice adjustments**. If the builder missed the area entirely, rerun it with better instructions instead of hand-curating a large replacement.
5. **Use CLI fallback only when needed**; see `cli-reference.md` for CLI-specific commands.

## Recommended Patterns

- **“Use RP to understand a flow”** → `context_builder(response_type="question")`
- **“Use RP to find similar patterns”** → `file_search` first, then `context_builder` if the pattern spans subsystems
- **“Use RP to explore this feature before implementing”** → brief orientation, then `context_builder(response_type="plan")`
- **“Use RP to search for endpoints / handlers / symbols”** → `file_search` or `get_code_structure` first, builder only if the answer is broader than a direct lookup

## Token Efficiency

- Use `get_code_structure` / `structure` before full file reads when shape is enough
- Let `context_builder` own the first broad selection
- Do not ask Oracle questions about files that are not in the current selection
- Avoid defaulting to `rp-cli` when MCP tools are already present in-session

## CLI Reference

If the user specifically wants CLI usage or MCP is unavailable, read `cli-reference.md` for the `rp-cli` command surface.
