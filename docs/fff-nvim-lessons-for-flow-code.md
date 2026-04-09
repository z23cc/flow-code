# fff.nvim 经验借鉴深度分析

> flow-code 可以从 fff.nvim 学到什么 | 2026-04-09

---

## 概述

fff.nvim 是一个高性能模糊文件查找 Neovim 插件 + MCP 服务器，核心是 Rust 后端（6 个 crate）+ Lua 前端。虽然和 flow-code 的功能领域不同（搜索 vs 开发编排），但其**架构模式**有多处值得借鉴。

识别出 **8 个可借鉴模式**，按对 flow-code 的适用性排序。

---

## 1. MCP 服务器暴露 flowctl 功能

### 影响：⬛⬛⬛⬛⬛ 极高 | 难度：中

### fff.nvim 怎么做的

fff.nvim 的 `fff-mcp` crate 把搜索能力暴露为 MCP 服务器，让 AI agent 直接调用搜索（带 frecency 记忆），减少 token 消耗和往返次数。

安装一行命令：
```bash
claude mcp add -s user fff -- /path/to/fff-mcp
```

### flow-code 现状

flowctl 只能通过 Bash 命令调用。每次调用都要启动进程、解析参数、JSON 序列化。MCP 调用会更高效：
- 无进程启动开销
- 结构化参数/返回值
- Claude Code 原生集成

### 适配方案

给 flowctl 加 MCP 服务器模式（新 crate `flowctl-mcp`）：

```rust
// crates/flowctl-mcp/src/main.rs
// 暴露核心 flowctl 操作为 MCP tools：
// - flowctl.epic.create / show / list
// - flowctl.task.create / start / done / ready
// - flowctl.phase.next / done
// - flowctl.checklist.verify
// - flowctl.guard
// - flowctl.status
```

注册方式：
```bash
claude mcp add -s user flowctl -- flowctl mcp-serve
```

### 预期收益
- Agent 调用 flowctl 零进程开销
- 结构化参数（不用拼 CLI 字符串）
- 与 Claude Code MCP 生态无缝集成
- 为其他 MCP 客户端（Cursor、Codex 等）开放 flowctl 能力

---

## 2. Frecency 记忆系统（基于频率 + 时效的智能排序）

### 影响：⬛⬛⬛⬛ 高 | 难度：中

### fff.nvim 怎么做的

fff.nvim 的 frecency 系统结合**访问频率**和**近期度**来排序文件：
- 每次打开文件自动记录（BufEnter autocmd）
- 存储在 heed ACID 数据库中
- 搜索时 frecency 分数参与排名

另外还有 **Combo Boost**：如果同一查询反复选择同一个文件（≥3 次），分数乘以 100x。

### flow-code 现状

flow-code 的记忆系统（`flowctl memory`）记录的是**教训和决策**，不是使用模式。没有基于频率/时效的任何排序：
- 研究侦察兵每次从零搜索
- 没有"上次这类任务改了哪些文件"的记忆
- 没有"哪些文件经常一起改"的关联

### 适配方案

Frecency 已集成到 flowctl：

```bash
# 搜索时自动使用 frecency 排序（内置在 flowctl search 中）
flowctl search "auth" --limit 10 --json
# → 经常修改的文件排名更高

# frecency 数据通过 flowctl done 自动记录（无需手动调用）
# 存储在 .flow/frecency.json
```

**集成点**：
- `flowctl done` 完成任务时自动记录修改文件到 frecency 存储
- `flowctl search` 搜索时 frecency 分数参与排名
- 无需手动调用 frecency 命令 — 完全自动化

存储：`.flow/frecency.json`（或 `.flow/frecency.db`）

### 预期收益
- 规划阶段更准确的文件预测
- Worker 不用从零搜索
- "这类改动通常涉及哪些文件"的智能建议

---

## 3. Health Check 系统

### 影响：⬛⬛⬛⬛ 高 | 难度：低

### fff.nvim 怎么做的

`:FFFHealth` 命令检查 6 个维度：
- 二进制可用性和可加载性
- Git 仓库检测（libgit2 版本）
- 文件索引初始化状态
- Frecency 数据库健康（条目数、磁盘大小、路径）
- 查询追踪器数据库健康
- 图片预览能力

每个维度返回 ok/warn/error 状态。

### flow-code 现状

`flowctl doctor` 已有基础健康检查，但不够全面：
- 不检查二进制版本是否匹配插件版本
- 不检查 review backend 工具是否可用（rp-cli、codex）
- 不检查 `.flow/` 状态文件完整性
- 不检查磁盘空间
- 不检查 Git 状态（是否在仓库中、分支状态）

### 适配方案

增强 `flowctl doctor`：

```bash
flowctl doctor --comprehensive --json
```

新增检查维度：
```json
{
  "binary": {"version": "0.1.43", "matches_plugin": true},
  "flow_dir": {"exists": true, "writable": true, "size_mb": 2.3},
  "review_backends": {
    "rp_cli": {"available": true, "version": "1.2.0"},
    "codex": {"available": false, "reason": "not in PATH"}
  },
  "git": {"is_repo": true, "branch": "main", "clean": false, "uncommitted": 3},
  "state_integrity": {
    "epics": {"count": 2, "valid": true},
    "tasks": {"count": 7, "orphaned": 0},
    "locks": {"stale": 0}
  },
  "project_context": {"exists": true, "path": ".flow/project-context.md"},
  "checklists_dir": {"exists": true}
}
```

### 预期收益
- 用户自助排障（"为什么 review 不工作？" → `flowctl doctor` 显示 codex 不在 PATH）
- CI 集成（`flowctl doctor --json` 作为预检）
- 插件更新后自动检测版本不匹配

---

## 4. 时间预算保护（Time Budget）

### 影响：⬛⬛⬛ 中高 | 难度：低

### fff.nvim 怎么做的

grep 搜索有 `time_budget_ms = 150` 配置——如果搜索超过 150ms，返回已有结果而不是让 UI 卡住。

### flow-code 现状

- Worker 有 30 分钟超时（`worker.timeout_minutes`）
- 但没有更细粒度的时间预算：
  - 研究侦察兵可能跑很久
  - `flowctl guard` 没有超时（lint/test 可能卡住）
  - RP context_builder 调用没有超时

### 适配方案

给 flowctl 的关键操作加时间预算：

```toml
# .flow/config.json
{
  "time_budgets": {
    "guard_seconds": 300,        # guard 最多 5 分钟
    "scout_seconds": 120,        # 每个侦察兵最多 2 分钟
    "review_seconds": 180        # 每次 review 最多 3 分钟
  }
}
```

`flowctl guard` 超时时返回部分结果 + 警告：
```json
{"verdict": "TIMEOUT", "completed": ["lint", "typecheck"], "timed_out": ["test"], "elapsed_seconds": 300}
```

### 预期收益
- 防止流水线因单个长 guard/test 永远卡住
- 侦察兵不会无限探索
- Ralph 无人值守模式更健壮

---

## 5. 跨模式建议（Cross-Mode Suggestions）

### 影响：⬛⬛⬛ 中 | 难度：低

### fff.nvim 怎么做的

搜索无结果时，自动切换到另一种搜索模式：
- 文件名搜索无结果 → 展示内容匹配结果
- 内容搜索无结果 → 展示文件名匹配结果
- 明确标注 "No results found. Suggested ..."

### flow-code 现状

研究侦察兵各自独立运行，没有"搜索无结果时的降级策略"：
- repo-scout 找不到相关代码 → 空结果
- context-scout RP 不可用 → 跳过
- 没有跨侦察兵的结果补充机制

### 适配方案

在 plan 阶段的研究步骤加降级策略：

```markdown
## Research Fallback Chain
1. RP context_builder (最快、最全)
2. 如果 RP 不可用 → repo-scout + practice-scout 并行
3. 如果 repo-scout 无结果 → 扩大搜索范围（去掉路径限制）
4. 如果仍无结果 → 用 `flowctl search` 模糊搜索（内置 frecency 排序）
5. 最终兜底 → Web search（如果可用）
```

### 预期收益
- 研究阶段不会因单个侦察兵失败而信息不足
- 自动降级确保总有结果

---

## 6. 配置弃用迁移系统（Deprecation Migration）

### 影响：⬛⬛⬛ 中 | 难度：低

### fff.nvim 怎么做的

`conf.lua` 中有完整的弃用迁移系统：
- 自动检测旧配置路径
- 自动迁移到新路径
- 显示警告告知用户
- 旧路径继续工作一段时间

### flow-code 现状

`.flow/config.json` 没有版本迁移机制。如果配置格式变化：
- 用户需要手动更新
- 旧配置可能导致静默失败
- 没有弃用警告

### 适配方案

给 `flowctl config` 加迁移子命令：

```bash
flowctl config migrate --json
# → {"migrated": [{"old": "review_backend", "new": "review.backend"}], "warnings": [...]}
```

在 `flowctl init` 和 `flowctl status` 时自动检测并提示迁移。

### 预期收益
- 升级不破坏现有配置
- 用户知道哪些配置已弃用

---

## 7. 多格式 FFI 导出（C/Lua/Node/Bun/MCP）

### 影响：⬛⬛ 中低 | 难度：高

### fff.nvim 怎么做的

一套 Rust 核心，6 种消费方式：
- `fff-nvim`：Lua FFI（mlua）
- `fff-c`：C FFI（cbindgen）
- `fff-mcp`：MCP 服务器
- `fff-bun`：Bun.js 绑定
- `fff-node`：Node.js 绑定
- 直接 CLI

### flow-code 现状

flowctl 只有 CLI 接口。虽然有 `flowctl codex sync` 导出到 Codex 格式，但核心功能只能通过 CLI 调用。

### 适配方案

长期目标：flowctl-core 作为库暴露给多种消费者：
- `flowctl-cli`（已有）
- `flowctl-mcp`（优先，见模式 1）
- `flowctl-wasm`（Web 编辑器集成）
- `flowctl-py`（Python 绑定，给 CI 脚本用）

**短期优先**：先做 MCP（模式 1），其他后续。

---

## 8. Profiling 二进制（性能基准）

### 影响：⬛⬛ 中低 | 难度：低

### fff.nvim 怎么做的

Rust workspace 包含多个 profiling 二进制：
- `search_profiler`
- `grep_profiler`
- `grep_vs_rg`（与 ripgrep 对比基准）
- `jemalloc_profile`

### flow-code 现状

没有性能基准。不知道：
- `flowctl` 命令的响应时间分布
- JSON 序列化/反序列化的开销
- `.flow/` 状态文件在大项目中的读写性能

### 适配方案

加 `flowctl stats benchmark` 子命令：

```bash
flowctl stats benchmark --json
# → {"init_ms": 12, "status_ms": 5, "ready_ms": 8, "phase_next_ms": 3, ...}
```

在 CI 中跟踪回归：
```bash
# 每次 release 前运行
flowctl stats benchmark --compare baseline.json
```

### 预期收益
- 发现性能回归
- 大项目（100+ 任务）优化依据

---

## 实施优先级总结

| # | 模式 | 影响 | 难度 | 建议 |
|---|------|------|------|------|
| 1 | **MCP 服务器** | 极高 | 中 | 下一版本重点 — flowctl 作为 MCP server |
| 2 | **Frecency 记忆** | 高 | 中 | 与 MCP 一起做，增强规划智能 |
| 3 | **Health Check 增强** | 高 | 低 | 立即可做 — 扩展 flowctl doctor |
| 4 | **时间预算保护** | 中高 | 低 | 立即可做 — guard/scout/review 超时 |
| 5 | **跨模式建议** | 中 | 低 | 下次规划技能更新时加入 |
| 6 | **配置弃用迁移** | 中 | 低 | 下次配置格式变化时加入 |
| 7 | **多格式 FFI** | 中低 | 高 | 长期目标，先做 MCP |
| 8 | **Profiling 基准** | 中低 | 低 | CI 优化时加入 |

**最高优先**：MCP 服务器（模式 1）+ Health Check 增强（模式 3）+ 时间预算（模式 4）。这三个直接提升 flow-code 的可靠性和集成能力。

---

*Generated 2026-04-09 by flow-code analysis*
