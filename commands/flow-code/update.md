---
name: flow-code:update
description: Update flow-code plugin to latest version
argument-hint: ""
---

# Update flow-code plugin

Tell the user to run these CLI commands in order (they are Claude Code commands, NOT shell — do NOT run them via Bash):

```
/plugin marketplace remove flow-code
/plugin marketplace add https://github.com/z23cc/flow-code
/plugin install flow-code
/reload-plugins
```

After `/reload-plugins`, the next SessionStart hook will automatically provision the correct `flowctl` binary (downloads from GitHub Releases, falls back to `cargo build` if source is present). No manual compile or copy needed.

**To sync immediately without waiting for the next session**, you MAY run the ensure script via Bash:

```bash
PLUGIN_ROOT=$(ls -dt ~/.claude/plugins/cache/flow-code/flow-code/*/ 2>/dev/null | head -1)
CLAUDE_PLUGIN_ROOT="${PLUGIN_ROOT%/}" "${PLUGIN_ROOT%/}/scripts/hooks/ensure-flowctl.sh"
```

This runs the same logic the SessionStart hook uses — idempotent and safe to rerun.
