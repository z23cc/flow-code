# Superpowers 集成详情

> 本文件是 pace-workflow 的 Superpowers 集成参考。详细 brainstorming 流程、执行策略对比和开发方法。

---

## P 阶段：Brainstorming 详细流程

invoke `superpowers:brainstorming` 完整 6 步：

1. **探索项目上下文** — 扫描代码库结构和关键文件
2. **提问澄清需求** — 向用户确认模糊需求
3. **提出 2-3 方案** — 含 trade-offs 对比分析
4. **逐段展示设计** — 分模块讲解设计决策
5. **写设计文档** — `docs/plans/YYYY-MM-DD-<topic>-design.md` + git commit
6. **自动写计划** — invoke `superpowers:writing-plans` → `docs/plans/YYYY-MM-DD-<feature>.md`

### 搜索资源优先级

1. **Context7 MCP**：库/框架官方文档（优先，不消耗配额）
2. **互联网搜索**：通用问题、Stack Overflow、博客
3. **GitHub Issues/Discussions**：特定库的已知问题

---

## E 阶段：执行策略详细对比

### Step 1 — Worktree 隔离（推荐）

`EnterWorktree` 创建隔离分支。PACEflow 在 worktree 中完全可用（`resolveProjectCwd` 使用 `CLAUDE_PROJECT_DIR` 定位项目根，vault artifacts 正常访问）。

降级条件（不使用 worktree）：HOTFIX / 单文件修改 / 用户指定不用。

### Step 2 — 选择执行方式

| 条件 | 执行 skill | 说明 |
|------|-----------|------|
| task 有依赖 / 高风险 / 核心模块 | `superpowers:executing-plans` | 每 3 task 停下等人工反馈 |
| 独立 task + 不同 domain | `superpowers:dispatching-parallel-agents` | 多 agent 真并行 |
| 独立 task + 同 domain（**默认**） | `superpowers:subagent-driven-development` | 自动 Spec + Code Quality 双审 |
| 降级（HOTFIX / 简单任务） | 直接执行 | 不使用 Superpowers |

### Step 3 — TDD 开发方法（推荐）

invoke `superpowers:test-driven-development` — 写失败测试 → 确认失败 → 写最小实现 → 确认通过 → commit。

降级条件：HOTFIX / UI 样式改动 / 无测试框架。

### Step 4 — 收尾

invoke `superpowers:finishing-a-development-branch` — 验证测试 → 选择 merge/PR/keep/discard。

---

## E 阶段：纠偏触发条件示例

以下情况应走纠偏流程（暂停 → 诊断 → 修正 → 重新批准 → 恢复）：

- 选定的技术方案不可行（如 API 不支持预期功能）
- 架构决策错误导致后续多个任务需要重写
- 发现关键依赖冲突影响整体方案

以下情况**不需要**纠偏，直接调整：

- 修改单个任务的实现方式
- 补充遗漏的步骤
- 调整实现细节不影响整体架构

---

## V 阶段：验证分级

| 类别 | 要求 | 示例 |
|------|------|------|
| **必须测试** | 自动化测试 | API 端点、数据处理函数、安全逻辑 |
| **建议测试** | 自动化或手动 | 业务逻辑函数、工具函数 |
| **可选测试** | 手动验证即可 | UI 组件、一次性脚本 |

验证替代：无测试框架时通过 Terminal/Browser 手动验证，结果记录到 walkthrough.md。

---

## Hook 强制行为汇总

| 阶段 | Hook | 行为 |
|------|------|------|
| C | PreToolUse | 无 `<!-- APPROVED -->` 或 `[/]`/`[!]` 任务 → **deny** |
| C | PreToolUse | impl_plan 无 `[/]` 索引 → **deny** |
| E | PreToolUse | 项目外文件豁免 PACE 检查 |
| V | Stop | `[x]` 无 `<!-- VERIFIED -->` → **block** |
| V | Stop | walkthrough 无今日日期 → warning |
