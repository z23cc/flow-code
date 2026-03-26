<div align="center">

**[English](README.md)** | **[中文](README_CN.md)**

# Flow-Code

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](../../LICENSE)
[![Claude Code](https://img.shields.io/badge/Claude_Code-Plugin-blueviolet)](https://claude.ai/code)

[![Version](https://img.shields.io/badge/Version-0.0.1-green)](../../CHANGELOG.md)

[![Status](https://img.shields.io/badge/Status-Active_Development-brightgreen)](../../CHANGELOG.md)

**先规划，后执行。零外部依赖。**

</div>

---

> **活跃开发中。** [更新日志](../../CHANGELOG.md) | [报告问题](https://github.com/z23cc/flow-code/issues)

> **Codex 审查后端**：跨模型审查现已支持 Linux/Windows（通过 OpenAI Codex CLI）。与 RepoPrompt 使用相同的 Carmack 级审查标准。详见 [跨模型审查](#跨模型审查)。

---

## 目录

- [这是什么？](#这是什么)
- [为什么有效](#为什么有效)
- [快速开始](#快速开始) — 安装、设置、使用
- [何时使用什么](#何时使用什么) — Interview vs Plan vs Work
- [Agent 就绪评估](#agent-就绪评估) — `/flow-code:prime`
- [故障排除](#故障排除)
- [Ralph（自治模式）](#ralph自治模式) — 无人值守运行
- [命令参考](#命令参考) — 所有斜杠命令
- [.flow/ 目录结构](#flow-目录结构)
- [flowctl CLI](#flowctl-cli) — 直接 CLI 使用
- [其他平台](#其他平台) — Factory Droid、OpenAI Codex

---

## 这是什么？

Flow-Code 是一个 Claude Code 插件，用于计划优先的工作流编排。内置任务追踪、依赖图、上下文重锚定和跨模型审查。

所有数据存储在你的仓库中。无外部服务。无全局配置。卸载：删除 `.flow/`（启用了 Ralph 的话还有 `scripts/ralph/`）。

---

## Epic 优先的任务模型

Flow-Code 不支持独立任务。

每个工作单元都属于一个 epic `fn-N`（即使只有一个任务）。

任务始终为 `fn-N.M`，从 epic spec 继承上下文。

原因：保持系统简洁，改善重锚定，使自动化（Ralph）可靠。

---

## 为什么有效

### 你控制粒度

逐任务执行并在每个任务后审查以获得最大控制，或者把整个 epic 交给它处理。

```bash
# 逐任务（每个任务后审查）
/flow-code:work fn-1.1

# 整个 epic（所有任务完成后审查）
/flow-code:work fn-1
```

### 无需担心上下文长度

- **规划时确定任务大小：** 每个任务的范围适合一次工作迭代
- **每个任务重锚定：** 执行前从 `.flow/` 读取最新 spec
- **Ralph 全新上下文：** 每次迭代从干净的上下文窗口开始

### 审查者作为安全网

1. Claude 实现任务
2. GPT 通过 RepoPrompt 审查（看到完整文件，不是 diff）
3. 审查阻塞直到 `SHIP` 判决
4. 修复 → 重新审查循环持续到通过

两个模型互相检查。

### 零摩擦

- **30 秒启动。** 安装插件，运行命令。无需设置。
- **非侵入式。** 无需编辑 CLAUDE.md。无守护进程。
- **干净卸载。** 删除 `.flow/`（启用了 Ralph 的话还有 `scripts/ralph/`）。
- **多用户安全。** 团队在并行分支上工作，无需协调服务器。

---

## 快速开始

### 1. 安装

```bash
# 添加市场
/plugin marketplace add https://github.com/z23cc/flow-code

# 安装 flow-code
/plugin install flow-code
```

### 2. 设置（推荐）

```bash
/flow-code:setup
```

这会：
- **配置审查后端**（RepoPrompt、Codex 或无）— 跨模型审查必需
- 复制 `flowctl` 到 `.flow/bin/` 以便直接 CLI 访问
- 将 flow-code 说明添加到 CLAUDE.md/AGENTS.md
- 创建 `.flow/usage.md` 包含完整 CLI 参考

设置后：
```bash
export PATH=".flow/bin:$PATH"
flowctl --help
flowctl epics                # 列出所有 epic
flowctl tasks --epic fn-1    # 列出 epic 的任务
flowctl ready --epic fn-1    # 查看准备就绪的任务
```

### 3. 使用

```bash
# 规划：研究、创建 epic 和任务
/flow-code:plan 添加一个带验证的联系表单

# 执行：按依赖顺序执行任务
/flow-code:work fn-1

# 或者直接从 spec 文件执行（自动创建 epic）
/flow-code:work docs/my-feature-spec.md
```

Flow-Code 处理研究、任务排序、审查和审计追踪。

---

## 何时使用什么

**关键问题：你的想法有多成熟？**

| 起点 | 推荐顺序 |
|------|---------|
| 新功能，想先确定 spec | Spec → Interview/Plan → Work |
| 模糊想法、粗略笔记 | Interview → Plan → Work |
| 详细 spec/PRD | Plan → Interview → Work |
| 明确需求，需要任务拆分 | Plan → Work |
| 小型单任务，spec 完整 | 直接 Work（创建 1 epic + 1 task） |

**Spec vs Interview vs Plan：**
- **Spec**（直接说"创建一个 spec"）创建带结构化需求的 epic（目标、架构、API 合约、边界情况、验收标准）。无任务，无代码库研究。
- **Interview** 通过深度问答（40+ 问题）完善 epic。仅写回 epic spec — 不创建任务。
- **Plan** 研究最佳实践，分析现有模式，创建带依赖关系的任务。

---

## Agent 就绪评估

```bash
/flow-code:prime
```

评估代码库的 agent 就绪度，扫描 8 大支柱（48 条标准）：

**Agent 就绪度（支柱 1-5）：** CLAUDE.md 质量、测试框架、工具链、构建系统、环境设置

**生产就绪度（支柱 6-8）：** CI/CD、可观测性、安全性（仅报告，不修改）

---

## 故障排除

### 重置卡住的任务

```bash
# 查看任务状态
flowctl show fn-1.2 --json

# 重置为 todo
flowctl task reset fn-1.2

# 重置 + 级联到依赖任务
flowctl task reset fn-1.2 --cascade
```

### 清理 `.flow/`

```bash
rm -rf .flow/
flowctl init
```

### 调试 Ralph 运行

```bash
# 查看运行进度
cat scripts/ralph/runs/*/progress.txt

# 查看迭代日志
ls scripts/ralph/runs/*/iter-*.log
```

---

## 卸载

```bash
rm -rf .flow/               # 核心 flow 状态
rm -rf scripts/ralph/       # Ralph（如果启用）
```

或使用 `/flow-code:uninstall`。

---

## Ralph（自治模式）

> **安全第一**：Ralph 默认 `YOLO=1`（跳过权限提示）。
> - 先用 `ralph_once.sh` 观察一次迭代
> - 考虑使用 [Docker 沙箱](https://docs.docker.com/ai/sandboxes/claude-code/) 进行隔离

Ralph 是仓库本地的自治循环，端到端地规划和执行任务。

### 设置（一次性）

```bash
/flow-code:ralph-init
```

### 运行

```bash
scripts/ralph/ralph.sh
scripts/ralph/ralph.sh --watch          # 实时查看工具调用
scripts/ralph/ralph.sh --watch verbose  # 查看完整输出
```

### 审查模式（per-task vs per-epic）

默认 Ralph 逐任务审查（`per-task`）。使用 `per-epic` 模式可以更快 — 先运行所有任务，然后进行一次全面的 epic 级审查。

**在 `scripts/ralph/config.env` 中配置：**

```bash
# per-epic：跳过逐任务审查，所有任务完成后进行一次 epic 级审查
REVIEW_MODE=per-epic

# 审查后端（rp = RepoPrompt, codex = Codex CLI, none = 跳过）
WORK_REVIEW=rp

# per-epic 模式下 completion review 自动继承 WORK_REVIEW 的后端
# 显式覆盖：COMPLETION_REVIEW=codex
```

**执行流程对比：**

```
per-task（默认）：                      per-epic（推荐，更快）：

 plan → plan_review（可选）              plan → plan_review（可选）
 task 1 → impl_review                   task 1 → 完成（无审查）
 task 2 → impl_review                   task 2 → 完成（无审查）
 task 3 → impl_review                   task 3 → 完成（无审查）
 ...                                    ...
 task N → impl_review                   task N → 完成（无审查）
 epic completion_review                  epic completion_review（覆盖全部）
 ────────────────────────               ────────────────────────
 N+1 次审查                              1 次审查
```

**常用配置：**

```bash
# 快速迭代 + 质量门控（推荐）
REVIEW_MODE=per-epic
WORK_REVIEW=rp

# 最快速度，无审查
REVIEW_MODE=per-epic
WORK_REVIEW=none
COMPLETION_REVIEW=none

# 严格模式，审查一切
REVIEW_MODE=per-task
WORK_REVIEW=rp
COMPLETION_REVIEW=rp
```

### Ralph 的独特之处

- **多模型审查门控**：通过 RepoPrompt 或 Codex CLI 发送审查给不同模型
- **审查循环直到通过**：修复 → 重新审查循环持续到审查者返回 `SHIP`
- **基于回执的门控**：审查必须生成回执 JSON 证明已执行
- **守卫钩子**：插件钩子强制执行工作流规则

### 控制 Ralph

```bash
# 检查状态
flowctl ralph status

# 暂停/恢复/停止
flowctl ralph pause
flowctl ralph resume
flowctl ralph stop

# 任务重试/回滚
flowctl task reset fn-1.3
flowctl task reset fn-1.2 --cascade
```

### 监控

```bash
# 实时查看日志
tail -f scripts/ralph/runs/latest/ralph.log

# 查看当前进度
scripts/ralph/flowctl list
```

---

## 命令参考

| 命令 | 说明 |
|------|------|
| `/flow-code:plan <描述或 fn-N>` | 研究 + 创建 epic 和任务 |
| `/flow-code:work <fn-N 或 fn-N.M>` | 按依赖顺序执行任务 |
| `/flow-code:interview <fn-N 或文件>` | 深度问答完善 spec |
| `/flow-code:plan-review <fn-N>` | Carmack 级计划审查（via RP/Codex） |
| `/flow-code:impl-review` | Carmack 级实现审查（当前分支） |
| `/flow-code:epic-review <fn-N>` | Epic 完成审查 |
| `/flow-code:prime` | 8 支柱 Agent 就绪评估 |
| `/flow-code:setup` | 安装 flowctl + 配置审查后端 |
| `/flow-code:ralph-init` | 搭建 Ralph 自治脚手架 |
| `/flow-code:sync` | 同步 spec 与实现 |
| `/flow-code:uninstall` | 移除 flow-code 文件 |

---

## .flow/ 目录结构

```
.flow/
  bin/
    flowctl          # CLI 工具
    flowctl.py
  config.json        # 项目配置
  epics/
    fn-1-xxx.json    # Epic 元数据
    fn-2-yyy.json
  specs/
    fn-1-xxx.md      # Epic spec（需求、架构、验收标准）
    fn-2-yyy.md
  tasks/
    fn-1-xxx.1.json  # 任务元数据（状态、依赖）
    fn-1-xxx.2.json
    fn-1-xxx.3.json
  usage.md           # CLI 参考文档
```

---

## flowctl CLI

```bash
# 列出所有 epic 和任务
flowctl list

# Epic 操作
flowctl epics                      # 列出所有 epic
flowctl show fn-1 --json           # 查看 epic 详情
flowctl cat fn-1                   # 查看 epic spec

# 任务操作
flowctl tasks --epic fn-1          # 列出 epic 的任务
flowctl ready --epic fn-1          # 查看准备就绪的任务
flowctl show fn-1.2 --json         # 查看任务详情
flowctl start fn-1.2               # 认领任务
flowctl done fn-1.2 --summary-file s.md --evidence-json e.json

# 任务管理
flowctl task reset fn-1.2          # 重置任务为 todo
flowctl task reset fn-1.2 --cascade  # 重置 + 级联依赖

# 验证
flowctl validate --epic fn-1 --json

# Ralph 控制
flowctl ralph status
flowctl ralph pause
flowctl ralph resume
flowctl ralph stop
```

---

## 系统要求

- Python 3.8+
- git
- 可选：[RepoPrompt](https://repoprompt.com/?atp=KJbuL4)（macOS GUI 审查）
- 可选：OpenAI Codex CLI（`npm install -g @openai/codex`，跨平台终端审查）

无审查后端时，审查步骤会被跳过。

---

## 其他平台

### Factory Droid

Flow-Code 原生支持 [Factory Droid](https://factory.ai)。

```bash
/plugin marketplace add https://github.com/z23cc/flow-code
/plugin install flow-code
```

### OpenAI Codex

Flow-Code 支持 OpenAI Codex，安装脚本自动转换插件系统：

```bash
git clone https://github.com/z23cc/flow-code.git
cd flow-code
./scripts/install-codex.sh flow-code
```

Codex 中命令使用 `/prompts:` 前缀：

| Claude Code | Codex |
|-------------|-------|
| `/flow-code:plan` | `/prompts:plan` |
| `/flow-code:work` | `/prompts:work` |
| `/flow-code:impl-review` | `/prompts:impl-review` |

---

<div align="center">

Made by [z23cc](https://github.com/z23cc)

</div>
