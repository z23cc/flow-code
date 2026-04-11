---
name: flow-code-go-v3
description: V3 Goal-driven adaptive execution. Creates goal, selects mode, runs optimization/execution loop.
user_invocable: false
---

# V3 Go — Goal-Driven Adaptive Execution

> Entry point for V3 architecture. Routes to Direct or Graph mode based on goal assessment.

## Entry

Call `goal_open` MCP tool (or `flowctl goal open`) with the user's request.

## Route by `planning_mode`

### Direct Mode (`planning_mode = "direct"`)

1. Execute the required modifications directly
2. Call `quality_run` (depth from response)
3. Call `goal_close`
4. Done

### Graph Mode (`planning_mode = "graph"`)

1. Call `plan_build` — generate execution graph with nodes and edges
2. Call `knowledge_search` — retrieve relevant past experience
3. **Execution loop:**
   a. Call `plan_next` — get currently ready nodes
   b. For each ready node, spawn a Worker subagent (parallel if multiple)
   c. Worker calls `node_start` → works → `node_finish`
   d. If `success_model` includes numeric: check `goal_status` for score delta
   e. If consecutive failures: `node_fail` returns escalation action
   f. If L3 escalation: call `plan_mutate` to restructure the graph
4. **Completion condition** (by `success_model`):
   - `criteria`: all `acceptance_criteria` are MET
   - `numeric`: `score_current >= score_target`
   - `mixed`: all criteria MET AND score reached
5. Call `quality_run` (depth = max guard_depth across all nodes)
6. Call `goal_close` (auto-triggers `knowledge_compound`)

## Worker Prompt Template

```
## Your Task
{node.objective}

## Constraints
{node.constraints}

## Risk Level
{node.risk.estimated_scope} — {node.risk.risk_rationale}
Guard depth: {node.risk.guard_depth}

## Past Experience (from knowledge base)
{node.injected_patterns}

## Owned Files
{node.owned_files}

## Available MCP Tools
- lock_acquire / lock_release — manage file locks
- quality_run — verify code quality (run before finishing)
- knowledge_record — record important discoveries
- node_finish — complete the task (pass summary + changed_files)
- node_fail — report failure (pass error details)

## Completion Criteria
1. Code changes committed
2. quality_run passes
3. Call node_finish with results
```
