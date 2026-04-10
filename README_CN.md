<div align="center">

**[English](README.md)** | **[中文](README_CN.md)**

# Flow-Code

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](../../LICENSE)
[![Claude Code](https://img.shields.io/badge/Claude_Code-Plugin-blueviolet)](https://claude.ai/code)
[![Version](https://img.shields.io/badge/Version-0.1.48-green)](https://github.com/z23cc/flow-code/releases)

**Claude Code 的生产级开发框架。从想法到 PR，全自动。**

</div>

---

## 这是什么？

一条命令，从想法到 PR — 规划、并行实现、三层质量门禁、跨模型对抗审查，全部自动化。

```
/flow-code:go "添加 OAuth 登录"
  → AI 自我访谈（自动头脑风暴）
  → 研究侦察兵（仓库、上下文、实践）
  → 依赖排序的任务 DAG
  → Teams 并行 Worker（文件锁）
  → 三层审查（guard + RP + Codex 对抗）
  → 自动推送 + 草稿 PR
```

所有状态存在 `.flow/` 目录。无外部服务。单个 Rust 二进制（`flowctl`，70+ 个命令）。卸载：删除 `.flow/`。

## 快速开始

**前置条件**：`git`、`jq`、`gh`（GitHub CLI）。可选：`rp-cli`（Layer 2 审查）、`codex`（Layer 3 对抗）。

```bash
# 安装
/plugin marketplace add https://github.com/z23cc/flow-code
/plugin install flow-code

# 配置（推荐 — 设置审查后端，复制 flowctl）
/flow-code:setup

# 全自动 — 从想法到 PR
/flow-code:go "添加 OAuth 支持"

# 快速模式 — 小改动跳过规划
/flow-code:go "修复 README 错别字" --quick

# 仅规划 — 只做研究与任务拆解，暂不执行
/flow-code:plan "添加 OAuth 支持"

# 恢复 — 读取 .flow 状态继续执行
/flow-code:go fn-1

# 需求先行，再交给后续计划/执行
/flow-code:spec "为管理后台引入 OAuth"

# 记录架构决策与备选方案
/flow-code:adr "选择 token/session 策略"
```

## 如何选择入口

| 如果你想... | 使用这个入口 |
|---|---|
| 执行完整流程，或恢复已有工作 | `/flow-code:go "想法"` 或 `/flow-code:go fn-1` — 全自动执行入口，也负责恢复 |
| 只停在规划阶段 | `/flow-code:plan "想法"` — 仅规划；只有在你已经走 `go` 路径时才用 `go --plan-only` |
| 先探索、发散、压测思路 | `/flow-code:brainstorm "想法"` — 先做开放式探索 |
| 先写一份可复用需求文档 | `/flow-code:spec "想法 / 变更 / 重构"` — 先产出 requirements artifact，再交给计划/执行 |
| 记录长期有效的架构决策 | `/flow-code:adr "决策"` — 持久化记录决策、备选方案与后果 |
| 安全替换或移除旧能力 | [`flow-code-deprecation`](skills/flow-code-deprecation/SKILL.md) — 技能入口（不是 slash command），用于替换/移除指导 |

## 核心工作流

```
brainstorm → plan → plan_review → work → impl_review → close
```

| 阶段 | 执行内容 |
|------|---------|
| **Brainstorm** | AI 自我访谈，结构化深化（Pre-mortem/第一性原理/逆向思维） |
| **Plan** | 并行侦察兵研究代码库，创建带依赖的任务 DAG |
| **Plan Review** | RP context_builder 或 Codex 验证规格-代码对齐 |
| **Work** | Teams 模式：连续并行调度 Worker，文件锁，以及最终 integration checkpoint |
| **Impl Review** | 三层并行审查：Blind Hunter + Edge Case Hunter + Acceptance Auditor |
| **Close** | 验证，guard，预发布清单，推送 + 草稿 PR |

每个任务属于一个 Epic（`fn-N`）。任务编号为 `fn-N.M`。即使是一次性请求也会创建 Epic 容器。

## 三层质量门禁

| 层 | 工具 | 时机 | 捕获 |
|----|------|------|------|
| **1. Guard** | `flowctl guard` | 每次提交 | Lint、类型、测试失败 |
| **2. RP Plan-Review** | RepoPrompt context_builder | Plan 阶段 | 规格-代码不一致 |
| **3. Codex 对抗** | `flowctl codex adversarial` | Epic 完成 | 安全、并发、边界情况 |

零发现规则：审查者必须找到问题。零发现 → 暂停重新分析。熔断机制：最多 2-3 轮迭代。

## 核心命令

| 命令 | 用途 |
|------|------|
| `/flow-code:go "想法"` | 全自动执行入口：brainstorm → plan → work → review → PR |
| `/flow-code:go "修复" --quick` | 小改动快速路径 |
| `/flow-code:go fn-1` | 从当前阶段恢复已有 Epic |
| `/flow-code:plan "功能"` | 仅规划：研究 + 任务拆解，暂不执行 |
| `/flow-code:plan-review fn-1` | 在开工前执行正式的 plan review gate |
| `/flow-code:work fn-1` | 执行 Epic 中的任务 |
| `/flow-code:impl-review fn-1.2 --base <commit>` | 对任务级或分支级实现变更做审查 |
| `/flow-code:epic-review fn-1` | 在 close 前按 spec 校验整个 Epic |
| `/flow-code:brainstorm --auto "想法"` | 在 plan/spec 前做开放式探索与压力测试 |
| `/flow-code:spec "想法 / 变更 / 重构"` | 先产出 artifact-first 的规划就绪需求文档 |
| `/flow-code:adr "决策"` | 记录可长期追溯的架构决策与备选方案 |
| `/flow-code:prime` | 代码库就绪评估（8 维 48 项） |
| `/flow-code:map` | 生成架构文档 |
| `/flow-code:auto-improve "目标"` | 自主代码优化循环 |
| `/flow-code:ralph-init` | 搭建自主运行框架 |
| `flowctl find "<查询>"` | 智能搜索：自动路由 regex/符号/精确/模糊 |
| `flowctl graph refs <符号>` | 谁引用了这个符号？ |
| `flowctl graph impact <路径>` | 改这个文件会影响什么？ |
| `flowctl edit --file <f> --old --new` | 智能编辑：精确匹配 + 模糊回退 |

命令索引：[commands/flow-code/](commands/flow-code/) | 所有标志：[CLAUDE.md](CLAUDE.md)

## flowctl CLI

单个 Rust 二进制，70+ 个顶层命令。所有命令支持 `--json` 机器可读输出。

```bash
flowctl init                          # 初始化 .flow/
flowctl epic create --title "..."     # 创建 Epic
flowctl task create --epic fn-1 ...   # 创建带依赖的任务
flowctl ready fn-1                    # 列出就绪任务
flowctl start fn-1.1                  # 开始任务
flowctl done fn-1.1 --summary "..."   # 完成并提交证据
flowctl guard                         # 运行 lint/type/test
flowctl checklist verify --task fn-1.1 # 验证完成清单
flowctl dag fn-1                      # ASCII 依赖图
flowctl codex adversarial --base main # 跨模型审查
flowctl write-file --path f --stdin   # 流水线文件 I/O
```

CLI 参考：[flowctl/README.md](flowctl/README.md)（以及 `flowctl --help`）

## 架构

```
commands/flow-code/*.md    → 20+ 个斜杠命令（用户入口，含 spec/adr）
skills/*/SKILL.md          → 50+ 个技能（工作流 + 领域）
  └─ steps/*.md            → 步骤文件架构（JIT 加载）
agents/*.md                → 20+ 个子 Agent（侦察兵、Worker、审查器）
flowctl/                   → Rust Cargo 工作区（core + cli）
  └─ bin/flowctl           → 单二进制，70+ 个命令
prompts/                   → 审查模板（blind-hunter, edge-case, acceptance-auditor）
templates/                 → project-context.md 模板
.flow/                     → 运行时状态（JSON/JSONL，按项目）
```

## 核心特性

**全自动** — `/flow-code:go` 零问题。AI 自动决定分支、审查后端、研究深度。

**Teams 模式** — 就绪任务连续并行调度 Worker 执行，带文件锁、陈旧锁恢复，以及最终 integration checkpoint。

**步骤文件架构** — 技能拆分为步骤文件（`steps/step-01-init.md` 等），JIT 加载，每次调用省 ~60% token。

**项目上下文** — `.flow/project-context.md` 提供所有 Worker 共享的技术标准。

**定义完成** — `flowctl checklist` 8 项默认清单，4 个类别（上下文、实现、测试、文档）。

**Ralph** — 自主运行框架，无人值守循环执行完整流水线。

**重锚定** — 每个 Worker 执行前读取任务 spec + 项目上下文 + 记忆。跨 compaction 存活。

**DAG 运行时变更** — `flowctl task split/skip`、`dep rm` 运行时可用。Worker 通过协议消息请求变更。

## 详细文档

| 文档 | 内容 |
|------|------|
| [CLAUDE.md](CLAUDE.md) | 架构、设计决策、命令标志、测试 |
| [commands/flow-code/](commands/flow-code/) | 斜杠命令索引（含 `spec` 与 `adr`） |
| [skills/flow-code-guide/SKILL.md](skills/flow-code-guide/SKILL.md) | 技能/命令发现流程图 |
| [skills/flow-code-documentation/SKILL.md](skills/flow-code-documentation/SKILL.md) | 文档实践（spec/ADR/README/changelog） |
| [skills/flow-code-deprecation/SKILL.md](skills/flow-code-deprecation/SKILL.md) | 弃用、替换与移除指南 |
| [CHANGELOG.md](CHANGELOG.md) | 版本历史 |

## 许可证

MIT
