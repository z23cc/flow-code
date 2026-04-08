# 深度对比：flow-code vs BMAD-METHOD

> flow-code v0.1.42 vs BMAD-METHOD v6.2.2 | 2026-04-08

---

## 总结

两个插件都覆盖了从构思到交付的完整软件开发生命周期，但设计哲学截然不同：

- **flow-code**：工程自动化派 — Rust 二进制状态引擎，零交互全自动，DAG 依赖图，文件锁，三层质量门禁，Teams 并行 Worker
- **BMAD-METHOD**：人机协作派 — 纯 Markdown 步骤文件架构，强调"AI 放大人类思维而非替代"，结构化引导对话，对抗式审查，角色化 Agent 人格

---

## 1. 核心哲学

| 维度 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **设计理念** | 全自动工程化 — AI 自主完成从构思到 PR 的全流程 | 人机协作 — AI 作为专家协作者引导人类做出最佳决策 |
| **交互模式** | 零交互（`/flow-code:go` 全程无需人工输入） | 引导式对话（Agent 提问 → 人类回答 → Agent 深化） |
| **核心信条** | "代码即证据" — 基于证据的完成验证 | "AI 放大思维，而非替代思维" — Human Amplification |
| **复杂度处理** | 统一流水线，通过 plan-depth 分级 | Scale-Domain-Adaptive — 从 bug fix 到企业系统自动调整深度 |
| **目标用户** | 需要高度自动化的独立开发者/团队 | 需要结构化引导的产品团队（PM、架构师、开发者） |

### 分析

这是最根本的分歧。flow-code 认为 AI 应该**做完所有事**（零交互合约）；BMAD 认为 AI 应该**引导人类做出更好的决策**。两种哲学都有其适用场景：flow-code 适合已有明确需求的重复性工作，BMAD 适合需求不明确、需要深度思考的新项目。

---

## 2. 架构

| 维度 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **运行时** | Rust 二进制 (`flowctl`)，37 个 CLI 命令 | 无二进制 — 纯 Markdown 技能文件 + YAML 配置 |
| **状态存储** | `.flow/` 目录下 JSON/JSONL 文件，`fs2` advisory lock | Sprint Status YAML + Story frontmatter (`stepsCompleted`) |
| **并发安全** | advisory 文件锁，所有读写操作原子化 | 无并发控制 — 单会话单 Agent 执行 |
| **依赖管理** | `petgraph` DAG — 拓扑排序、环检测 | CSV 定义的 `after`/`before` 依赖（`module-help.csv`） |
| **状态机** | 8 种任务状态（todo → in_progress → done/blocked/skipped/failed） | 4 种 Story 状态（backlog → ready-for-dev → in-progress → review → done） |
| **配置系统** | `.flow/config.json` 点分路径 | `_bmad/bmm/config.yaml` 变量模板（`{user_name}`, `{planning_artifacts}`） |
| **安装方式** | Claude Code 插件市场 + `flowctl` 二进制 | Claude Code 插件市场 + `bmad-cli.js` 安装器 |

### 分析

flow-code 的 Rust 引擎提供形式化保证（并发安全、DAG 验证、文件锁），代价是构建复杂度高。BMAD 用 YAML/Markdown 管理状态，简单直观但缺乏并发保护。flow-code 适合多 Worker 并行场景；BMAD 的单会话模型不需要并发控制。

---

## 3. 工作流编排

### 阶段系统

| 方面 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **顶层阶段** | 6 阶段：brainstorm → plan → plan_review → work → impl_review → close | 4 阶段：Analysis → Planning → Solutioning → Implementation |
| **阶段内细分** | Worker 级 12 内部阶段（验证→重锚定→调研→TDD→实现→验证→提交→证据→目标校验→记忆→完成→清理） | 步骤文件架构（PRD 创建 12 步、代码审查 4 步，每步独立 .md 文件） |
| **阶段强制** | `flowctl phase next/done` 二进制强制 | 步骤文件 JIT 加载 + `stepsCompleted` 数组追踪 |
| **并行执行** | Teams 模式：多 Worker 并行 + 文件锁 + 波次检查点 | 子 Agent 并行（审查、研究），但工作流本身串行 |
| **会话恢复** | `flowctl phase next` 从中断处恢复 | Continuation 检测 — 检查 frontmatter 中 `stepsCompleted`，路由到恢复步骤 |

### Agent 模型

| 方面 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **Agent 数量** | 24 个子 Agent（研究侦察兵 + Worker + 审计器） | 6 个角色化 Agent + 技能内子 Agent |
| **Agent 风格** | 功能型（repo-scout, practice-scout, context-scout） | 人格型（Mary 分析师、John PM、Winston 架构师、Amelia 开发者、Sally UX、Paige 技术写作） |
| **Agent 交互** | SendMessage 纯文本协议（"Task complete:", "Blocked:"） | 引导式对话（Agent 提问，人类回答） |
| **Agent 配置** | model/maxTurns/disallowed-tools | config.yaml 变量注入（user_name, language, output paths） |

### 分析

flow-code 的编排是**机器驱动**的 — 二进制强制阶段顺序，Worker 自主执行，人类不参与。BMAD 的编排是**对话驱动**的 — 每个步骤都是 Agent 与人类的结构化交互，步骤文件 JIT 加载节省 token。

BMAD 的步骤文件架构是一个亮点：每步独立 .md 文件，按需加载，不把 12 步全部塞进上下文。flow-code 的技能是整体加载的。

---

## 4. 质量门禁

### flow-code：三层独立门禁

| 层 | 工具 | 时机 | 内容 |
|----|------|------|------|
| 1. Guard | `flowctl guard` | Worker Phase 6、波次检查点、Close | lint、类型检查、测试 |
| 2. RP Plan-Review | RP context_builder / Codex | Plan 阶段 | 规格-代码对齐 |
| 3. Codex 对抗审查 | `flowctl codex adversarial` | Epic 完成 | 安全、并发、边界（不同模型族） |

熔断机制：Plan review 最多 2 轮，impl review 最多 3 轮。

### BMAD：对抗式审查 + 定义完成清单

| 机制 | 详情 |
|------|------|
| **对抗式审查规则** | 审查者**必须找到问题**，零发现 = 暂停重新分析 |
| **三层并行代码审查** | Blind Hunter（盲审）+ Edge Case Hunter（边界分析）+ Acceptance Auditor（验收标准审计），各自独立上下文 |
| **定义完成清单** | 24 项检查（上下文 4 + 实现 5 + 测试 6 + 文档 5 + 状态 4），全部通过才能标记 Done |
| **PRD 验证** | 全面性、精炼度、组织性、一致性检查 |
| **实现准备门禁** | PRD + UX + 架构 + Epics/Stories 对齐验证 |

### 对比

| 维度 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **跨模型审查** | 有（Codex 是不同模型族） | 无（同一模型的多个子 Agent） |
| **审查多样性** | 3 层（确定性 + RP + 对抗） | 3 个并行审查者 + 验收审计 |
| **熔断机制** | 有（2-3 次迭代上限） | 无 |
| **对抗式理念** | "尝试打破代码" | "必须找到问题，零发现=暂停" |
| **定义完成** | evidence-based completion（证据 JSON） | 24 项结构化清单 |
| **确定性检查** | `flowctl guard`（lint/type/test） | `npm run validate:skills`（14 条规则） |

### 分析

两个系统都重视对抗式审查，但实现方式不同。flow-code 用**跨模型**多样性（Claude + Codex），BMAD 用**同模型多 Agent**多样性（Blind Hunter + Edge Case Hunter + Acceptance Auditor，各自独立上下文）。flow-code 的熔断机制防止无限循环，BMAD 缺少这个安全网。

BMAD 的 24 项 DoD 清单比 flow-code 的 evidence JSON 更结构化、更易审计。

---

## 5. 知识管理

| 维度 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **记忆系统** | `flowctl memory` — 原子条目，自动捕获（Worker Phase 10），陈旧检测 | 无专门记忆系统 |
| **项目上下文** | CLAUDE.md + `.flow/` 状态 | `project-context.md` — 所有实现 Agent 共享的技术标准文档 |
| **知识沉淀** | `/flow-code:retro` 回顾 + memory 条目 | Sprint 回顾 + Story 内 Dev Agent Record |
| **文档生成** | `/flow-code:map` 代码库地图 | Paige Agent — Document Project / Write Document / Validate Document / Mermaid Generate |
| **研究能力** | 研究侦察兵（repo-scout, practice-scout, context-scout） | 三类研究 Agent（Market Research, Domain Research, Technical Research） |
| **多语言** | 仅英文 | 5 种语言文档（EN, FR, ZH-CN, VI, CS） + Agent 通信语言 vs 文档语言可分开配置 |

### 分析

flow-code 的记忆系统更自动化（自动捕获 + 陈旧检测），但 BMAD 的 `project-context.md` 模式更优雅 — 一个文件统一所有 Agent 的技术决策上下文，防止 Agent 间决策冲突。

BMAD 的研究能力更偏产品侧（市场研究、领域研究），flow-code 更偏代码侧（仓库侦察、上下文侦察）。

---

## 6. 独特功能对比

### flow-code 独有

| 功能 | 描述 |
|------|------|
| **Rust 状态引擎** | 形式化状态机，DAG 环检测，advisory 文件锁，37 个 CLI 命令 |
| **Teams 并行模式** | 多 Worker 并行执行 + 文件锁 + 波次检查点 |
| **三层质量门禁** | Guard + RP + Codex 对抗（跨模型多样性） |
| **熔断机制** | Review 迭代上限（2-3 次） |
| **架构不变量** | `flowctl invariant add/check` — 注册并验证架构规则 |
| **Gap 管理** | 追踪缺失需求，Epic close 时强制解决 |
| **任务重启级联** | `flowctl restart` 重置任务并级联下游 |
| **Auto-improve** | 分析驱动的自动代码优化循环 |
| **Ralph 自主运行** | 全自动无人值守运行守护进程 |
| **代码库地图** | `/flow-code:map` 自动生成架构文档 |
| **就绪评估** | `/flow-code:prime` 8 维 48 项评估 |
| **write-file 命令** | pipeline 全程通过 flowctl 写文件，不触发权限弹窗 |

### BMAD 独有

| 功能 | 描述 |
|------|------|
| **角色化 Agent 人格** | 6 个有名字、有性格、有沟通风格的 Agent（Mary/John/Winston/Amelia/Sally/Paige） |
| **步骤文件架构** | 每步独立 .md 文件，JIT 加载，节省 token，支持会话恢复 |
| **Party Mode** | 真正的多 Agent 讨论（非角色扮演），各自独立上下文，防止共识坍缩 |
| **Quick Dev** | 快速意图→代码流程，小改动跳过规划直接实现 |
| **高级引导** | 命名式深化方法（pre-mortem, 第一性原理, 逆向思维, 红队, 苏格拉底式, 约束移除, 利益相关者映射, 类比推理） |
| **三层并行代码审查** | Blind Hunter + Edge Case Hunter + Acceptance Auditor，各自独立上下文 |
| **对抗式审查规则** | "必须找到问题，零发现=暂停重新分析" |
| **24 项 DoD 清单** | 结构化定义完成（上下文 4 + 实现 5 + 测试 6 + 文档 5 + 状态 4） |
| **project-context.md** | 所有 Agent 共享的项目技术标准，防止决策冲突 |
| **Scale-Domain-Adaptive** | 自动根据复杂度调整规划深度 |
| **多语言输出** | Agent 通信语言 vs 文档输出语言可分开配置 |
| **PRFAQ Working Backwards** | Amazon 式逆向工作法，从新闻稿倒推需求 |
| **研究子 Agent** | 市场研究 + 领域研究 + 技术研究并行 |
| **技能验证框架** | 14 条确定性规则 + 推理验证，CI 集成 |
| **模块化扩展** | 官方模块系统（BMad Builder, Test Architect, Game Dev Studio, Creative Intelligence Suite） |

---

## 7. 交叉学习机会

### flow-code 可以从 BMAD 学习

| 机会 | 影响 | 难度 | 说明 |
|------|------|------|------|
| **步骤文件架构** | 高 | 中 | 技能按步骤拆分为独立 .md 文件，JIT 加载节省 token，支持会话恢复 |
| **project-context.md** | 高 | 低 | 所有 Worker 共享的项目技术标准文档，防止 Agent 间决策冲突 |
| **三层并行代码审查** | 中 | 中 | Blind Hunter + Edge Case Hunter + Acceptance Auditor 模式 |
| **"零发现=暂停"规则** | 中 | 低 | 强制审查者找到问题，防止橡皮图章式通过 |
| **24 项 DoD 清单** | 中 | 低 | 比 evidence JSON 更结构化的完成验证 |
| **Quick Dev 快速路径** | 中 | 中 | 小改动跳过完整规划流程，直接进入实现 |
| **高级引导方法** | 低 | 低 | 命名式深化（pre-mortem 等），brainstorm 时使用 |
| **PRFAQ Working Backwards** | 低 | 低 | 新闻稿倒推法，产品构思阶段使用 |
| **多语言文档输出** | 低 | 低 | config 支持通信语言和文档语言分别配置 |

### BMAD 可以从 flow-code 学习

| 机会 | 影响 | 难度 | 说明 |
|------|------|------|------|
| **二进制状态引擎** | 高 | 高 | 形式化状态机 + DAG + 并发安全 |
| **并行 Worker + 文件锁** | 高 | 高 | Teams 模式支持真正的多 Worker 并行 |
| **跨模型审查** | 高 | 中 | 不同模型族（Claude vs Codex）交叉验证 |
| **零交互模式** | 中 | 中 | 全自动流水线选项，适合需求明确的场景 |
| **熔断机制** | 中 | 低 | Review 迭代上限，防止无限循环 |
| **记忆系统** | 中 | 中 | 自动捕获 + 陈旧检测 + Worker 注入 |
| **架构不变量** | 中 | 低 | 注册并验证架构规则 |
| **Auto-improve** | 中 | 中 | 分析驱动的自动优化循环 |
| **Gap 管理** | 低 | 低 | 追踪缺失需求，close 时强制解决 |

---

## 8. 总结矩阵

| 维度 | flow-code | BMAD-METHOD |
|------|-----------|-------------|
| **哲学** | 全自动，零交互 | 人机协作，引导式 |
| **架构** | Rust 二进制，JSON 状态 | 纯 Markdown，YAML 状态 |
| **并发** | 文件锁 + DAG + Teams | 单会话，无并发控制 |
| **阶段系统** | 6 Epic 阶段 + 12 Worker 阶段，二进制强制 | 4 大阶段 + 步骤文件 JIT 加载 |
| **Agent 风格** | 功能型侦察兵 | 人格型角色（有名字和性格） |
| **质量门禁** | 3 层 + 熔断 | 对抗审查 + 24 项 DoD |
| **跨模型** | 有（Codex 对抗） | 无 |
| **知识管理** | 自动记忆 + 陈旧检测 | project-context.md + Sprint 回顾 |
| **研究** | 代码侧（repo/context/practice scout） | 产品侧（市场/领域/技术研究） |
| **Token 效率** | 技能整体加载 | 步骤文件 JIT 加载 |
| **会话恢复** | flowctl phase next 恢复 | stepsCompleted 数组 + continuation 检测 |
| **多语言** | 仅英文 | 5 语言 + 双语言配置 |
| **自动化程度** | 极高（Ralph 守护进程） | 中等（需要人工参与对话） |

---

## 一句话总结

> **flow-code** 是**工程自动化引擎** — 用 Rust 保证正确性，零交互全自动，适合需求明确的批量开发。
>
> **BMAD-METHOD** 是**人机协作框架** — 用角色化 Agent 引导思维，对抗式审查保证质量，适合需求探索和深度产品思考。
>
> 两者互补而非竞争：BMAD 擅长的"需求发现和深度思考"正是 flow-code 假设已完成的前置工作。

---

*Generated 2026-04-08 by flow-code pipeline*
