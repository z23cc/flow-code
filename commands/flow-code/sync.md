---
name: flow-code:sync
description: "Sync downstream task specs after implementation drift"
argument-hint: "<id> [--dry-run]"
---

# IMPORTANT: This command MUST invoke the skill `flow-code-sync`

The ONLY purpose of this command is to call the `flow-code-sync` skill. You MUST use that skill now.

**Arguments:** $ARGUMENTS

Choose this front door when implementation drift means downstream plan/task specs need refreshing. Use `/flow-code:plan` to create or reshape plans and `/flow-code:work` to execute them.

Pass the arguments to the skill. The skill handles the sync logic.
