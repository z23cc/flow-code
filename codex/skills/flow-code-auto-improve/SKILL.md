---
name: flow-code-auto-improve
description: "Use when user wants to autonomously optimize code quality, performance, security, or test coverage. Triggers on /flow-code:auto-improve, 'auto improve', or 'run experiments on'."
user-invocable: false
context: fork
---

# Auto-Improve

One command to start autonomous code improvement. Auto-detects everything, starts immediately.

```
/flow-code:auto-improve "优化 API 性能" --scope src/api/
```

## Input

Full request: $ARGUMENTS

**Format:** `/flow-code:auto-improve "<goal>" [--scope <dirs>] [--max <n>] [--watch]`

| Param | Required | Default | Description |
|-------|----------|---------|-------------|
| goal | YES | — | What to improve (natural language) |
| --scope | no | `.` (whole project) | Directories agent may modify (space-separated) |
| --max | no | 50 | Max experiments before stopping |
| --watch | no | off | Show Claude tool calls in real-time |

**Examples:**
```
/flow-code:auto-improve "fix N+1 queries and add missing tests" --scope src/
/flow-code:auto-improve "reduce bundle size" --scope src/components/ --max 20
/flow-code:auto-improve "improve security" --scope src/api/ src/auth/
/flow-code:auto-improve "提升测试覆盖率到 80%"
```

## Execution (all automatic)

### Step 1: Setup + Analysis-Driven Program Generation

```bash
PLUGIN_ROOT="$HOME/.codex"
TEMPLATES="$PLUGIN_ROOT/skills/flow-code-auto-improve/templates"
FLOWCTL="$HOME/.flow/bin/flowctl"

mkdir -p scripts/auto-improve/runs

# Detect project type
PROJECT_TYPE=$(python3 "$TEMPLATES/detect-project.py" .)
# Returns: django, nextjs, react, or generic

# Copy/update core files (always refresh from plugin)
cp "$TEMPLATES/auto-improve.sh" scripts/auto-improve/
chmod +x scripts/auto-improve/auto-improve.sh
cp "$TEMPLATES/prompt_experiment.md" scripts/auto-improve/

# .gitignore
cat > scripts/auto-improve/.gitignore <<'GITIGNORE'
config.env
runs/
*.log
GITIGNORE
```

**Program.md: if already exists, preserve it (user edits respected). Otherwise, GENERATE from analysis — not copy template.**

If `scripts/auto-improve/program.md` already exists, skip to Step 2.

If it does NOT exist, generate a custom program.md by analyzing the codebase:

#### Step 1a: Collect codebase signals

Run these commands and capture output (all safe, read-only):

```bash
# 1. High-churn files (most modified recently — best improvement targets)
HOTSPOTS=$(git log --since="3 months ago" --diff-filter=M --name-only --pretty=format: \
  | grep -E '\.(py|ts|tsx|js|jsx)$' | sort | uniq -c | sort -rn | head -15)

# 2. Lint errors by file (Python)
LINT_ERRORS=""
if command -v ruff >/dev/null 2>&1; then
  LINT_ERRORS=$(ruff check . --output-format grouped 2>/dev/null | head -50)
fi

# 3. Type errors (JS/TS)
TYPE_ERRORS=""
if [[ -f "frontend/tsconfig.json" ]] || [[ -f "tsconfig.json" ]]; then
  TYPE_ERRORS=$(npx tsc --noEmit 2>&1 | grep "error TS" | head -20)
fi

# 4. Test coverage gaps (which modules have tests, which don't)
TEST_MAP=""
if command -v pytest >/dev/null 2>&1; then
  TEST_MAP=$(python -m pytest --co -q 2>/dev/null | head -30)
fi

# 5. Memory pitfalls (if flow-code memory enabled)
MEMORY_PITFALLS=""
if [[ -x "$FLOWCTL" ]]; then
  MEMORY_PITFALLS=$($FLOWCTL memory inject --json 2>/dev/null || echo "")
fi
```

#### Step 1a-rp: RP Refactor Analysis (optional, three-tier fallback)

After collecting statistical signals, attempt RP-powered refactor analysis for deeper insight into code quality issues (redundancies, complexity hotspots, dead code). This complements the statistical signals above with AI-driven structural analysis.

**Tier 1 (MCP) -- context_builder available:**

If the `mcp__RepoPrompt__context_builder` tool is available in the current session, invoke it directly:

```
mcp__RepoPrompt__context_builder(
  instructions: "Analyze the codebase under ${SCOPE} for improvement opportunities aligned with the goal: ${GOAL}. Focus on: (1) redundant or duplicated logic that could be consolidated, (2) overly complex functions/modules that need simplification, (3) dead code or unused exports, (4) missing error handling patterns, (5) performance anti-patterns. Return a ranked list of specific, actionable findings with file paths and line references.",
  response_type: "review"
)
```

Store the result as `RP_REFACTOR_FINDINGS`. Proceed to Step 1b.

**Tier 2 (CLI) -- rp-cli available, MCP not:**

```bash
RP_REFACTOR_FINDINGS=""
if command -v rp-cli >/dev/null 2>&1; then
  RP_REFACTOR_FINDINGS=$(timeout 120 rp-cli -e 'builder "Analyze the codebase under '"${SCOPE}"' for improvement opportunities aligned with the goal: '"${GOAL}"'. Focus on: (1) redundant or duplicated logic, (2) overly complex functions/modules, (3) dead code or unused exports, (4) missing error handling, (5) performance anti-patterns. Return a ranked list of specific findings with file paths." --response-type review' 2>/dev/null || echo "")
fi
```

**Tier 3 (none) -- neither available:**

Skip RP analysis entirely. `RP_REFACTOR_FINDINGS` remains empty. Step 1b uses only the statistical signals collected above (hotspots, lint, coverage, memory). Zero regression from current behavior.

#### Step 1b: Generate Action Catalog

Using the collected signals + user's GOAL, generate `scripts/auto-improve/program.md` with this structure:

```markdown
# Auto-Improve Program

## Goal
${GOAL}

## Scope
You may ONLY modify files in: `${SCOPE}`

## Fitness Function
Guard: ${GUARD_CMD}
Direction: lint errors ↓, test count ↑, type errors ↓

## Action Catalog (ranked by estimated impact)

| # | Action | Impact | File | Source | How |
|---|--------|--------|------|--------|-----|
```

**Populate the Action Catalog by combining:**

1. **From user's GOAL**: Parse the goal and generate 3-5 specific actions targeting it
   - Goal "修复 N+1 查询" → scan for missing `select_related`/`prefetch_related` in scope
   - Goal "提升测试覆盖率" → identify modules with 0 test files
   - Goal "优化性能" → check for obvious bottlenecks (no pagination, no caching)

2. **From hotspot analysis**: For each of the top 5 high-churn files in scope, suggest one specific improvement

3. **From lint errors**: Group ruff errors by type, suggest top 3 fixable categories

4. **From RP refactor analysis** (if `RP_REFACTOR_FINDINGS` is non-empty): Merge RP-identified issues into the catalog. Each RP-sourced action gets a `source: rp-refactor` tag in the table:
   ```markdown
   | # | Action | Impact | File | Source | How |
   |---|--------|--------|------|--------|-----|
   | 1 | Consolidate duplicate validation logic | High | src/api/views.py | rp-refactor | Extract shared validator... |
   | 2 | Remove dead export in utils | Low | src/utils.py | rp-refactor | Delete unused `format_legacy()` |
   ```
   - De-duplicate: if RP findings overlap with hotspot or lint signals, keep the more specific one
   - RP findings that don't overlap with statistical signals are especially valuable (they catch structural issues that metrics miss)

5. **From memory pitfalls**: Include any relevant pitfalls as "Gotchas" section:
   ```markdown
   ## Gotchas (from project memory)
   - [pitfall content from memory #N]
   ```

6. **Impact estimation**:
   - High: Fixes a bug, adds tests for untested code, removes N+1 queries
   - Medium: Reduces lint errors, improves types, simplifies complex code
   - Low: Style fixes, dead code removal, documentation

**Rank actions**: High impact first, Low impact last. Agent works top-to-bottom.

#### Step 1c: Add standard sections

After the Action Catalog, append these sections from the template (keep/discard criteria, experiment process, output format):

```bash
# Read the keep/discard and process sections from template (reuse, don't duplicate)
tail -n +$( grep -n "## Experiment Process" "$TEMPLATES/programs/${PROJECT_TYPE}.md" | head -1 | cut -d: -f1 ) \
  "$TEMPLATES/programs/${PROJECT_TYPE}.md" >> scripts/auto-improve/program.md
```

#### Step 1d: Fallback

If analysis fails (no git, no ruff, no pytest — e.g., fresh clone), fall back to template:

```bash
if [[ ! -f scripts/auto-improve/program.md ]]; then
  # Analysis failed — use static template as fallback
  cp "$TEMPLATES/programs/${PROJECT_TYPE}.md" scripts/auto-improve/program.md
fi
```

### Step 2: Auto-detect guard command

Scan project and build the best guard command automatically:

```bash
GUARD_PARTS=()

# Python: ruff/flake8 + pytest
if [[ -f "pyproject.toml" ]] || [[ -f "setup.py" ]] || [[ -f "manage.py" ]]; then
  command -v ruff >/dev/null && GUARD_PARTS+=("ruff check .")
  if grep -q "pytest" pyproject.toml 2>/dev/null || [[ -f "pytest.ini" ]] || [[ -f "conftest.py" ]]; then
    GUARD_PARTS+=("python -m pytest -x -q")
  fi
fi

# Node: lint + test
if [[ -f "package.json" ]]; then
  grep -q '"lint"' package.json && GUARD_PARTS+=("npm run lint")
  grep -q '"test"' package.json && GUARD_PARTS+=("npm test")
fi

# Fallback
if [[ ${#GUARD_PARTS[@]} -eq 0 ]]; then
  GUARD_CMD="echo 'WARNING: no guard detected — set GUARD_CMD in scripts/auto-improve/config.env'"
else
  GUARD_CMD=$(IFS=' && '; echo "${GUARD_PARTS[*]}")
fi
```

### Step 3: Write config.env (merge user params + detected values)

```bash
TAG=$(date -u +%Y%m%d)
cat > scripts/auto-improve/config.env <<CONF
GOAL=${GOAL}
SCOPE=${SCOPE}
GUARD_CMD=${GUARD_CMD}
EXPERIMENT_TAG=${TAG}
MAX_EXPERIMENTS=${MAX}
YOLO=1
CONF
```

Where `GOAL`, `SCOPE`, `MAX` come from parsed arguments.

### Step 4: Show config and start

```
Auto-Improve starting!

  Goal:    ${GOAL}
  Scope:   ${SCOPE}
  Guard:   ${GUARD_CMD}
  Project: ${PROJECT_TYPE}
  Max:     ${MAX} experiments

  Logs:    scripts/auto-improve/runs/latest/
  Program: scripts/auto-improve/program.md (edit to customize)

Starting experiment loop...
```

Then immediately run:

```bash
scripts/auto-improve/auto-improve.sh
```

If `--watch` was passed, add `--watch` flag.

## Notes

- First run auto-scaffolds `scripts/auto-improve/`. Subsequent runs reuse existing program.md (preserves user edits).
- User can edit `scripts/auto-improve/program.md` between runs to adjust improvement focus.
- `config.env` is regenerated each run from command args (goal/scope/max override previous).
- Guard command is auto-detected but can be overridden: add `--guard "custom command"` or edit config.env.
