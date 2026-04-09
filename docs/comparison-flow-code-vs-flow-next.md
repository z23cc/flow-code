# 深度对比：flow-code vs flow-next (gmickel-claude-marketplace)

> flow-code v0.1.44 vs flow-next v0.29.1 | 2026-04-09

---

## 关系

flow-code 最初 fork 自 flow-next，但已经走上了完全不同的架构路线。两者共享核心理念（plan-first、re-anchoring、.flow/ 状态、零依赖），但在引擎、规模和方向上已大幅分化。

---

## 1. 核心引擎

| 维度 | flow-code | flow-next |
|------|-----------|-----------|
| **引擎语言** | **Rust** (31,419 行) | **Python** (7,716 行 flowctl.py) |
| **二进制** | 编译后单文件 (~3.5MB) | 解释执行 Python 脚本 |
| **启动速度** | ~5ms | ~200-500ms (Python 启动) |
| **CLI 命令数** | **68 个** | ~30 个 (估算，Python 子命令) |
| **并发安全** | fs2 advisory lock + Rust 类型安全 | Python 文件锁 |
| **DAG 引擎** | petgraph (拓扑排序 + 环检测) | 自建依赖排序 |
| **状态机** | 8 种任务状态，形式化 | 隐式状态 (JSON 字段) |

### 分析
flow-code 用 Rust 重写了引擎，代码量是 flow-next 的 4x（31K vs 7.7K 行），但获得了：
- 编译时类型检查（Python 只有运行时错误）
- 毫秒级 CLI 响应（vs Python 数百毫秒）
- 形式化 DAG 验证（petgraph vs 手写排序）
- 68 个 CLI 命令（vs ~30 个）

flow-next 的优势是**修改简单** — 改 Python 脚本不需要编译。

---

## 2. 规模对比

| 维度 | flow-code | flow-next |
|------|-----------|-----------|
| **命令** | 22 个斜杠命令 | 11 个 |
| **技能** | 73 个 | 16 个 |
| **Agent** | 24 个 | 20 个 |
| **flowctl 命令** | 68 个 | ~30 个 |
| **领域技能** | 47 个（安全/认证/缓存/数据库/API/国际化/容器/微服务/实时/状态管理/错误处理/监控/文档等） | 0 个 |
| **步骤文件** | 15 个（JIT 加载） | 0 个（整体加载） |
| **prompt 模板** | 8 个（blind-hunter/edge-case/acceptance-auditor 等） | 0 个 |

### 分析
flow-code 在 fork 后扩展了 4.6x 技能（73 vs 16）和 2x 命令（22 vs 11）。最大的增量是 47 个**领域专用技能**（flow-next 完全没有）和 15 个**步骤文件**（JIT 加载节省 token）。

---

## 3. flow-code 独有功能

| 功能 | 描述 | 对应 flow-next |
|------|------|---------------|
| **Rust 二进制** | 编译时类型安全，毫秒级响应 | Python 脚本 |
| **68 个 CLI 命令** | 完整的 CLI 工具链 | ~30 个 |
| **模糊搜索 (nucleo)** | `flowctl search` — 防错别字 + frecency + git 加权 | 无 |
| **N-gram 索引** | `flowctl index` — trigram 倒排索引，<1ms 搜索 | 无 |
| **代码结构** | `flowctl code-structure` — 9 语言符号提取 | 无 |
| **Repo Map** | `flowctl repo-map` — PageRank 排序符号概览 | 无 |
| **模糊 Patch** | `flowctl patch` — fudiff 3 层回退 | 无 |
| **project-context.md** | 自动检测技术栈 + Guard Commands + File Conventions + 全管线读取 | 无 |
| **结构化完成清单** | `flowctl checklist` — 8 项 DoD | 无 |
| **步骤文件架构** | 15 个步骤文件 JIT 加载，省 60% token | 整体加载 |
| **三层并行代码审查** | Blind Hunter + Edge Case Hunter + Acceptance Auditor | 单一审查 |
| **零发现暂停规则** | 审查必须找到问题 | 无 |
| **高级引导方法** | Pre-mortem/第一性原理/逆向思维 | 无 |
| **Quick Dev** | `--quick` 标志跳过规划 | 无 |
| **47 个领域技能** | 安全/认证/数据库/API/缓存/容器/微服务等 | 无 |
| **8 个 prompt 模板** | 审查/对抗/PR 模板 | 无 |
| **DAG 环检测** | petgraph 拓扑排序 | 无 |
| **任务重启级联** | `flowctl restart` 重置下游 | 无 |
| **架构不变量** | `flowctl invariant add/check` | 无 |
| **Gap 管理** | `flowctl gap` 追踪缺失需求 | 无 |
| **事件溯源** | `flowctl events` 全程记录 | 无 |
| **文件锁 (Teams)** | `flowctl lock/unlock` | 无 (Python 锁不同层级) |
| **write-file** | 流水线零交互文件写入 | 无 |
| **doctor 增强** | 9 类健康检查 | 基础检查 |
| **Frecency 记忆** | 指数衰减文件排序 | 无 |
| **配置弃用迁移** | 自动检测旧配置 | 无 |

---

## 4. flow-next 独有功能

| 功能 | 描述 | flow-code 状态 |
|------|------|---------------|
| **Python 引擎** | 修改简单，无需编译 | Rust 需编译 |
| **TUI 监控** | flow-next-tui (Bun/TypeScript) 实时 Ralph 监控 | 无 TUI |
| **Beads 集成** | 原版 flow 插件有 Beads 上下文管理 | 无 |
| **双插件架构** | flow (legacy) + flow-next (推荐) 共存 | 单插件 |
| **watch mode** | Ralph `--watch` / `--watch verbose` | Ralph 无 watch |
| **git add -A** | 总是 git add -A（不选择性 add） | 选择性 git add |

### 分析
flow-next 的独有优势主要是 **TUI 监控**（Ralph 运行可视化）和**修改无需编译**。其他差异是设计选择（git add -A vs 选择性 add）。

---

## 5. 共享核心理念

两者保留的共同基因：

| 理念 | 实现 |
|------|------|
| **Plan-first** | brainstorm → plan → work → review → close |
| **Re-anchoring** | 每个 Worker 读取 spec + 状态 |
| **.flow/ 状态** | JSON 文件，无数据库 |
| **零外部依赖** | 单二进制/脚本 + git/jq/gh |
| **Multi-model review** | RP + Codex 双后端 |
| **Ralph 自主模式** | 无人值守循环执行 |
| **Evidence recording** | 每个任务记录 commits/tests |
| **Memory system** | 学习 pitfalls，跨会话 |
| **研究侦察兵** | 并行 scout 收集上下文 |
| **Prime 评估** | 8 维 48 项就绪评估 |

---

## 6. 架构差异

| 维度 | flow-code | flow-next |
|------|-----------|-----------|
| **Epic 阶段** | 6 阶段（brainstorm → close），flowctl 强制顺序 | 隐式阶段，技能自行管理 |
| **Worker 阶段** | 12 个内部阶段（flowctl worker-phase） | 技能内嵌 phases.md |
| **技能加载** | 步骤文件 JIT 加载（省 60% token） | 整体加载 |
| **审查** | 三层并行（Blind + Edge + Acceptance） | 单层（RP 或 Codex） |
| **Guard** | project-context.md Guard Commands → stack config → 自动检测 | 手动配置 |
| **Domain 分配** | File Conventions 自动匹配 | 手动 |
| **搜索** | nucleo 模糊 + N-gram 索引 + frecency | Grep/Glob |
| **代码理解** | code-structure + repo-map (PageRank) | 无 |
| **Patch** | fudiff 模糊匹配（3 层回退） | 标准 Edit |
| **健康检查** | 9 类（binary/git/state/tools/search/context） | 基础 |
| **跨平台** | Claude Code + Codex sync | Claude Code + Codex + Factory Droid |

---

## 7. 总结

### flow-code 的方向
> **工程深度** — 用 Rust 重写引擎，加入搜索/索引/代码结构/模糊 patch 等底层能力，扩展 47 个领域技能，project-context.md 贯穿全管线。往"AI agent 的操作系统"方向发展。

### flow-next 的方向
> **稳定实用** — 保持 Python 引擎简洁（7.7K 行），加 TUI 监控，稳步迭代。往"可靠的 plan-first 工具"方向发展。

### 数字对比

| 指标 | flow-code | flow-next | 倍数 |
|------|-----------|-----------|------|
| 技能数 | 73 | 16 | **4.6x** |
| 命令数 | 22 | 11 | **2x** |
| CLI 命令 | 68 | ~30 | **2.3x** |
| 引擎代码 | 31,419 行 Rust | 7,716 行 Python | **4x** |
| Agent 数 | 24 | 20 | **1.2x** |
| 领域技能 | 47 | 0 | **∞** |
| 搜索能力 | 4 种（fuzzy/index/structure/map） | 0 | **∞** |

---

*Generated 2026-04-09 by flow-code*
