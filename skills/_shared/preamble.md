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

Run memory injection to surface relevant learnings, pitfalls, and conventions:

```bash
# Inject entries matching skill domain (replace SKILL_TAGS with skill-specific tags)
$FLOWCTL memory inject --tags "{{SKILL_TAGS}}" --json 2>/dev/null || true

# If working on a specific epic, also search for related memories
$FLOWCTL memory search "{{EPIC_CONTEXT}}" --type pitfall 2>/dev/null || true
```

Memory levels:
- **L1 (headlines)**: Default — shows entry titles for quick scan
- **L2 (summaries)**: Use `--full` for entries matching current context
- **L3 (full content)**: Only when debugging a specific known pitfall

If memory returns critical-severity pitfalls, **pause and review them** before proceeding.

## Step 4: Review backend health check

Verify review backends are reachable before entering work/review phases:

```bash
# Check configured review backend
REVIEW_BACKEND=$($FLOWCTL review-backend 2>/dev/null || echo "none")

if [ "$REVIEW_BACKEND" = "rp" ]; then
  # Check if rp-cli or RP MCP is available
  which rp-cli >/dev/null 2>&1 || echo "WARNING: review backend is 'rp' but rp-cli not found. Reviews will fail. Set to 'none' via: $FLOWCTL config set review.backend none"
elif [ "$REVIEW_BACKEND" = "codex" ]; then
  which codex >/dev/null 2>&1 || echo "WARNING: review backend is 'codex' but codex CLI not found. Reviews will fail."
fi
```

If backend is misconfigured, warn early — don't wait until mid-epic to discover reviews can't run.

## Step 5: Session context
- Current branch: `git branch --show-current`
- Active epic: check `.flow/` for in-progress epics
- Plugin version: read from plugin.json

## Skill Tag Reference

| Skill | Recommended Tags |
|-------|-----------------|
| flow-code-run | workflow,pipeline,planning |
| flow-code-debug | debugging,testing,errors |
| flow-code-auto-improve | performance,quality,security |
| flow-code-api-design | api,architecture,contracts |
| flow-code-cicd | ci,deployment,automation |
| flow-code-django | django,python,backend |
| flow-code-performance | performance,optimization,benchmarks |
| flow-code-qa | testing,qa,browser,visual |
| flow-code-design-review | design,css,visual,ui |
| (default) | (use skill name as tag) |
