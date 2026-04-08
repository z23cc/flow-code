# Step 1: Mode Detection

## Input

Full request: $ARGUMENTS

Accepts:
- Feature/bug description in natural language
- `--auto` flag: enable AI self-interview mode (no human questions)
- Empty: ask "What idea or problem should we brainstorm? Describe it in 1-5 sentences."

Examples:
- `/flow-code:brainstorm Add real-time collaboration to the editor`
- `/flow-code:brainstorm --auto migrate from REST to GraphQL`
- `/flow-code:brainstorm --auto We keep getting auth token expiry bugs`
- `/flow-code:brainstorm We keep getting auth token expiry bugs`

## Mode Detection

Parse `$ARGUMENTS` for `--auto` flag:
- If `--auto` present: remove flag from arguments, set AUTO_MODE=true
- Otherwise: AUTO_MODE=false (interactive, original behavior)

**Pipeline auto-detection**: If this skill is invoked as part of the `/flow-code:go` pipeline (detected by: epic already exists, or `flow-code-run` is the caller), ALWAYS use Auto mode regardless of flags. The go pipeline has a zero-interaction contract.

## Pre-check: Local Setup Version

If `.flow/meta.json` exists and has `setup_version`, compare to plugin version:
```bash
SETUP_VER=$(jq -r '.setup_version // empty' .flow/meta.json 2>/dev/null)
# Portable: Claude Code uses .claude-plugin, Factory Droid uses .factory-plugin
PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.claude-plugin/plugin.json"
[[ -f "$PLUGIN_JSON" ]] || PLUGIN_JSON="${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT}}/.factory-plugin/plugin.json"
PLUGIN_VER=$(jq -r '.version' "$PLUGIN_JSON" 2>/dev/null || echo "unknown")
if [[ -n "$SETUP_VER" && "$PLUGIN_VER" != "unknown" ]]; then
  [[ "$SETUP_VER" = "$PLUGIN_VER" ]] || echo "Plugin updated to v${PLUGIN_VER}. Run /flow-code:setup to refresh local scripts (current: v${SETUP_VER})."
fi
```
Continue regardless (non-blocking).

## Next Step

Read `steps/step-02-context-gather.md` and execute.
