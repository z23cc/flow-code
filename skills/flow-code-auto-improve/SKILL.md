---
name: flow-code-auto-improve
description: "Use when user wants to autonomously optimize code quality, performance, security, or test coverage. Triggers on /flow-code:auto-improve, 'auto improve', or 'run experiments on'."
user-invocable: false
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

### Step 1: Setup (auto, first-time creates files, subsequent runs reuse)

```bash
PLUGIN_ROOT="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}"
TEMPLATES="$PLUGIN_ROOT/skills/flow-code-auto-improve/templates"

mkdir -p scripts/auto-improve/runs

# Detect project type
PROJECT_TYPE=$(python3 "$TEMPLATES/detect-project.py" .)
# Returns: django, nextjs, react, or generic

# Copy/update core files (always refresh from plugin)
cp "$TEMPLATES/auto-improve.sh" scripts/auto-improve/
chmod +x scripts/auto-improve/auto-improve.sh
cp "$TEMPLATES/prompt_experiment.md" scripts/auto-improve/

# Program.md: copy if missing, preserve if user edited
if [[ ! -f scripts/auto-improve/program.md ]]; then
  cp "$TEMPLATES/programs/${PROJECT_TYPE}.md" scripts/auto-improve/program.md
fi

# .gitignore
cat > scripts/auto-improve/.gitignore <<'GITIGNORE'
config.env
runs/
*.log
GITIGNORE
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
