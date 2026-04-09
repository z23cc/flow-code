# Step 2: Codebase Context Gathering

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

**Always run (both modes):** Read relevant code, git log, and project structure to understand scope.

**In auto mode, gather deep context:**
1. Search for files related to the request:
   - `$FLOWCTL find "<key terms>" --json` — auto-routes to best search backend
   - `$FLOWCTL graph refs "<symbol>" --json` — if investigating a specific function/type
   - `$FLOWCTL graph map --json` — project overview (instant, cached)
   - `file_search` (RP MCP) or Grep/Glob (fallback) for exact regex patterns
2. Read git log for recent changes in relevant areas
3. Check existing `.flow/` specs/epics for related work
4. Read key config files, README, CLAUDE.md for project constraints
5. Identify affected modules, dependencies, and integration points
6. Read `.flow/project-context.md` Non-Goals and Architecture Decisions. Do NOT propose approaches that conflict with Non-Goals. Reference Architecture Decisions to avoid re-debating settled choices.
7. Read pitfalls specifically to avoid known failure patterns:
   ```bash
   $FLOWCTL memory list --type pitfall --json
   ```

## Classify Complexity

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

## Next Step

- If AUTO_MODE=true: Read `steps/step-03-self-interview.md` and execute.
- If AUTO_MODE=false (interactive): Read `steps/step-03-self-interview.md` and execute (it handles both modes).
