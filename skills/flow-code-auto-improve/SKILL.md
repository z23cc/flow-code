---
name: flow-code-auto-improve
description: Autonomous code improvement loop inspired by Karpathy's autoresearch. Detects project type, generates improvement program, runs experiments that modify code → test → keep/discard. Use when user wants to optimize code quality, performance, security, or test coverage autonomously. Triggers on /flow-code:auto-improve.
user-invocable: false
---

# Auto-Improve

Autonomous code improvement loop. Agent discovers improvements, implements them, judges results, keeps or discards — repeating until stopped.

Inspired by [Karpathy's autoresearch](https://github.com/karpathy/autoresearch).

**CRITICAL: flowctl is BUNDLED — NOT installed globally.** Always use:
```bash
FLOWCTL="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/scripts/flowctl"
```

## Input

Full request: $ARGUMENTS

**Modes:**

- `--init` — Scaffold `scripts/auto-improve/` with config, program.md, and experiment loop
- `--init --goal "optimize API performance" --scope src/api/` — Init with pre-configured goal and scope
- `--bootstrap` — Also scaffold basic test infrastructure if missing
- (no flags) — Show current config and print run command

**Examples:**
```
/flow-code:auto-improve --init
/flow-code:auto-improve --init --goal "improve test coverage to 80%" --scope tests/ src/
/flow-code:auto-improve --init --bootstrap
/flow-code:auto-improve
```

## --init Mode

### Step 1: Detect project type

```bash
PLUGIN_ROOT="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}"
PROJECT_TYPE=$(python3 "$PLUGIN_ROOT/skills/flow-code-auto-improve/templates/detect-project.py" . --json)
```

Returns: `django`, `nextjs`, `react`, or `generic`.

### Step 2: Create directory

```bash
mkdir -p scripts/auto-improve/runs
```

### Step 3: Copy templates

Copy from plugin templates to project:

```bash
TEMPLATES="$PLUGIN_ROOT/skills/flow-code-auto-improve/templates"

# Core script
cp "$TEMPLATES/auto-improve.sh" scripts/auto-improve/
chmod +x scripts/auto-improve/auto-improve.sh

# Config (preserve existing)
if [[ ! -f scripts/auto-improve/config.env ]]; then
  TAG=$(date -u +%Y%m%d)
  sed "s/{{EXPERIMENT_TAG}}/$TAG/" "$TEMPLATES/config.env" > scripts/auto-improve/config.env
fi

# Prompt template
cp "$TEMPLATES/prompt_experiment.md" scripts/auto-improve/

# Program.md (project-type-specific)
TYPE=$(echo "$PROJECT_TYPE" | python3 -c "import sys,json; print(json.load(sys.stdin)['project_type'])")
cp "$TEMPLATES/programs/${TYPE}.md" scripts/auto-improve/program.md

# Copy flowctl for standalone use
cp "$PLUGIN_ROOT/scripts/flowctl" scripts/auto-improve/ 2>/dev/null || true
cp "$PLUGIN_ROOT/scripts/flowctl.py" scripts/auto-improve/ 2>/dev/null || true
chmod +x scripts/auto-improve/flowctl 2>/dev/null || true
```

### Step 4: Apply user options

If `--goal` provided, update config.env GOAL line.
If `--scope` provided, update config.env SCOPE line.

### Step 5: Create .gitignore

```bash
cat > scripts/auto-improve/.gitignore <<'EOF'
config.env
runs/
*.log
EOF
```

### Step 6: Auto-detect guard command

Scan project for test/lint commands:

```bash
# Check for common guard commands
if [[ -f "pyproject.toml" ]] && grep -q "pytest" pyproject.toml; then
  echo "Detected: pytest"
  # Suggest: GUARD_CMD="python -m pytest -x -q"
fi
if [[ -f "package.json" ]]; then
  if grep -q '"test"' package.json; then
    echo "Detected: npm test"
    # Suggest: GUARD_CMD="npm test"
  fi
  if grep -q '"lint"' package.json; then
    echo "Detected: npm run lint"
    # Suggest: GUARD_CMD="npm run lint && npm test"
  fi
fi
```

Update config.env GUARD_CMD with detected command. If nothing detected, warn user to set it manually.

### Step 7: Show summary

```
Auto-improve initialized!

  Project type: Django
  Program: scripts/auto-improve/program.md (edit to customize)
  Config: scripts/auto-improve/config.env

  Run:
    scripts/auto-improve/auto-improve.sh
    scripts/auto-improve/auto-improve.sh --watch

  Edit config.env to set:
    GOAL — what to improve
    SCOPE — which files to touch
    GUARD_CMD — tests that must pass
```

## --bootstrap Mode

After --init, if `--bootstrap` flag present:

**Django (no pytest):**
```bash
pip install pytest pytest-django pytest-cov
cat > pytest.ini <<'EOF'
[pytest]
DJANGO_SETTINGS_MODULE = config.settings
python_files = tests.py test_*.py *_tests.py
EOF
```

**React/Next.js (no test script):**
```bash
npm install --save-dev jest @testing-library/react @testing-library/jest-dom
```

Show what was installed and suggest updating GUARD_CMD.

## Default Mode (no flags)

If `scripts/auto-improve/` exists, show current config:

```bash
if [[ -d scripts/auto-improve ]]; then
  echo "Auto-improve is configured."
  echo ""
  cat scripts/auto-improve/config.env | grep -E "^(GOAL|SCOPE|GUARD_CMD|MAX_EXPERIMENTS)="
  echo ""
  echo "Run: scripts/auto-improve/auto-improve.sh"
  echo "Edit: scripts/auto-improve/config.env"
  echo "Customize: scripts/auto-improve/program.md"
else
  echo "Not initialized. Run: /flow-code:auto-improve --init"
fi
```
