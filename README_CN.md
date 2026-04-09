<div align="center">

**[English](README.md)** | **[中文](README_CN.md)**

# Flow-Code

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](../../LICENSE)
[![Claude Code](https://img.shields.io/badge/Claude_Code-Plugin-blueviolet)](https://claude.ai/code)
[![Version](https://img.shields.io/badge/Version-0.1.46-green)](https://github.com/z23cc/flow-code/releases)

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

所有状态存在 `.flow/` 目录。无外部服务。单个 Rust 二进制（`flowctl`，71 个命令）。卸载：删除 `.flow/`。

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

# 恢复 — 读取 .flow 状态继续执行
/flow-code:go fn-1
```

## 核心工作流

```
brainstorm → plan → plan_review → work → impl_review → close
```

| 阶段 | 执行内容 |
|------|---------|
| **Brainstorm** | AI 自我访谈，结构化深化（Pre-mortem/第一性原理/逆向思维） |
| **Plan** | 并行侦察兵研究代码库，创建带依赖的任务 DAG |
| **Plan Review** | RP context_builder 或 Codex 验证规格-代码对齐 |
| **Work** | Teams 模式：每波并行 Worker，文件锁，波次检查点 |
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
| `/flow-code:go "想法"` | 全自动：brainstorm → plan → work → review → PR |
| `/flow-code:go "修复" --quick` | 小改动快速路径 |
| `/flow-code:plan "功能"` | 仅研究 + 任务分解 |
| `/flow-code:work fn-1` | 执行 Epic 中的任务 |
| `/flow-code:brainstorm --auto "想法"` | AI 自我访谈 + 结构化深化 |
| `/flow-code:prime` | 代码库就绪评估（8 维 48 项） |
| `/flow-code:map` | 生成架构文档 |
| `/flow-code:auto-improve "目标"` | 自主代码优化循环 |
| `/flow-code:ralph-init` | 搭建自主运行框架 |
| `flowctl find "<查询>"` | 智能搜索：自动路由 regex/符号/精确/模糊 |
| `flowctl graph refs <符号>` | 谁引用了这个符号？ |
| `flowctl graph impact <路径>` | 改这个文件会影响什么？ |
| `flowctl edit --file <f> --old --new` | 智能编辑：精确匹配 + 模糊回退 |

完整命令参考：[docs/commands.md](docs/commands.md) | 所有标志：[CLAUDE.md](CLAUDE.md)

## flowctl CLI

单个 Rust 二进制，71 个顶层命令。所有命令支持 `--json` 机器可读输出。

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

完整 CLI 参考：[docs/flowctl.md](docs/flowctl.md)

## 架构

```
commands/flow-code/*.md    → 22 个斜杠命令（用户入口）
skills/*/SKILL.md          → 54 个技能（工作流 + 领域）
  └─ steps/*.md            → 步骤文件架构（JIT 加载）
agents/*.md                → 24 个子 Agent（侦察兵、Worker、审查器）
flowctl/                   → Rust Cargo 工作区（core + cli）
  └─ bin/flowctl           → 单二进制，71 个命令
prompts/                   → 审查模板（blind-hunter, edge-case, acceptance-auditor）
templates/                 → project-context.md 模板
.flow/                     → 运行时状态（JSON/JSONL，按项目）
```

## 核心特性

**全自动** — `/flow-code:go` 零问题。AI 自动决定分支、审查后端、研究深度。

**Teams 模式** — 就绪任务并行 Worker 执行，文件锁，陈旧锁恢复，波次检查点。

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
| [docs/flowctl.md](docs/flowctl.md) | 完整 CLI 参考（71 个命令） |
| [docs/skills.md](docs/skills.md) | 技能清单（54 个技能，分层分类） |
| [CHANGELOG.md](CHANGELOG.md) | 版本历史 |
| [docs/CODEBASE_MAP.md](docs/CODEBASE_MAP.md) | 自动生成的架构地图 |

## 许可证

MIT
</content>
</invoke>