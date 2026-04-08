# Step 2: Codebase Context Gathering

```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

**Always run (both modes):** Read relevant code, git log, and project structure to understand scope.

**In auto mode, gather deep context:**
1. Search for files related to the request:
   - `$FLOWCTL search "<key terms>" --limit 20 --json` — fuzzy file search with frecency ranking
   - `$FLOWCTL index search "<key terms>" --limit 20 --json` — trigram content search (if index exists)
   - `$FLOWCTL code-structure extract --path <relevant-dir> --json` — extract symbols for key directories
   - Grep/Glob for exact regex patterns (fallback)
2. Read git log for recent changes in relevant areas
3. Check existing `.flow/` specs/epics for related work
4. Read key config files, README, CLAUDE.md for project constraints
5. Identify affected modules, dependencies, and integration points

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
