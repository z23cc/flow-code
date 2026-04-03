---
name: flow-code:update
description: Update flow-code plugin to latest version
argument-hint: ""
---

# Update flow-code plugin

Run these commands in sequence to update to the latest version:

```bash
# Step 1: Remove old marketplace cache
/plugin marketplace remove flow-code

# Step 2: Re-add marketplace (fetches latest)
/plugin marketplace add https://github.com/z23cc/flow-code

# Step 3: Install latest version
/plugin install flow-code

# Step 4: Reload
/reload-plugins
```

**Run each command above in order.** Tell the user to execute them — do NOT try to run them via Bash (they are Claude Code CLI commands, not shell commands).
