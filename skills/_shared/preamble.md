<!-- Skills should include: "Before executing, follow the Startup Sequence in skills/_shared/preamble.md" -->

# Startup Sequence

**CRITICAL: flowctl is BUNDLED.** Always use:
```bash
FLOWCTL="$HOME/.flow/bin/flowctl"
```

## Step 1: Check .flow/ state
```bash
$FLOWCTL detect --json
```
If no `.flow/` exists and skill requires it, run `$FLOWCTL init`.

## Step 2: Check for interrupted work
```bash
$FLOWCTL status --interrupted --json
```
If interrupted tasks exist, notify the user before proceeding.

## Step 3: Load relevant memory
```bash
$FLOWCTL memory inject --tags "{{SKILL_TAGS}}" --json 2>/dev/null || true
```
Load memory entries relevant to the current skill context. Replace `{{SKILL_TAGS}}` with skill-specific tags.

## Step 4: Session context
- Current branch: `git branch --show-current`
- Active epic: check `.flow/` for in-progress epics
- Plugin version: read from plugin.json
