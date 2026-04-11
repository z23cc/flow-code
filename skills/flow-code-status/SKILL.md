---
name: flow-code-status
description: V3 Goal status — show active goals and their progress.
user_invocable: true
---

# V3 Status — Goal Progress

> Show status of active V3 goals.

## Usage

### Single Goal Status
```bash
flowctl goal status <goal-id> --json
```
Returns: goal details, planning_mode, success_model, current score (if numeric), acceptance criteria status.

### MCP Tool
When MCP server is running:
- `goal_status` — get goal status with suggested next action
