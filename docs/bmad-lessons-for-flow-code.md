# BMAD 经验借鉴深度分析

> flow-code 可以从 BMAD-METHOD 学到什么 | 2026-04-08

---

## 概述

经过对 BMAD-METHOD v6.2.2 的深度拆解，识别出 **7 个可借鉴模式**，按影响力和实施难度排序。每个模式都附带具体的 flow-code 适配方案。

---

## 1. 步骤文件架构（Step-File Architecture）

### 影响：⬛⬛⬛⬛⬛ 极高 | 难度：中

### BMAD 怎么做的

BMAD 的 PRD 创建技能把 12 步拆成 12 个独立 .md 文件（共 2,890 行 / 148KB），每步 100-350 行。核心机制：

```
skill-name/
  SKILL.md                  ← 入口，极简
  workflow.md               ← 编排层，配置加载
  steps-c/
    step-01-init.md         ← JIT 加载，只在执行时读入
    step-01b-continue.md    ← 断点续传处理器
    step-02-discovery.md
    ...
    step-12-complete.md     ← 终止步骤
```

**状态追踪**：YAML frontmatter 中的 `stepsCompleted` 数组
```yaml
stepsCompleted: ["step-01-init.md", "step-02-discovery.md"]
```

**断点续传**：检测到未完成 artifact（`stepsCompleted` 不含最终步骤）→ 自动路由到 `step-01b-continue.md` → 查表确定下一步 → 从中断处精确恢复

**每步结构**：
```markdown
# Step N: 标题
## 强制执行规则
## 执行协议
## 上下文边界
## 你的任务
## 具体步骤序列
## 成功指标
## 失败模式
## 下一步
```

### flow-code 现状

flow-code 已有**原型 BMAD 模式**但未形式化：
- `flow-code-plan` 有 `steps.md`（159 行），`flow-code-work` 有 `phases.md`（207 行）
- 但都是**整体加载**，无检查点，无断点续传
- 最大技能 `flow-code-brainstorm` 335 行，全部一次性载入上下文

**当前 token 开销**：
| 技能 | 当前加载量 | BMAD 化后 | 节省 |
|------|-----------|-----------|------|
| flow-code-plan | ~2,000 tokens | ~600-800 | 55-60% |
| flow-code-work | ~2,350 tokens | ~700-900 | 55-60% |
| flow-code-brainstorm | ~2,000 tokens | ~600-700 | 55-70% |
| **合计** | **~6,350** | **~2,000-2,400** | **62-65%** |

### 适配方案

**Phase 1：拆分三个最大技能**

将 `flow-code-plan/steps.md` 拆为：
```
steps/
  step-01-init.md              ← 输入解析，epic 检测
  step-02-clarity-check.md     ← 需求澄清
  step-03-research.md          ← 并行侦察兵（repo/context/practice scout）
  step-04-gap-analysis.md      ← 缺口分析
  step-05-depth-decision.md    ← short/standard/deep 分级
  step-06-task-breakdown.md    ← 任务分解 + DAG
  step-07-dependencies.md      ← 依赖声明
  step-08-acceptance.md        ← 验收标准
  step-09-output.md            ← 写入 .flow/
```

将 `flow-code-brainstorm` 拆为：
```
steps/
  step-01-mode-detect.md       ← --auto 标志检测
  step-02-context-gather.md    ← 代码分析，git log，现有 specs
  step-03-self-interview.md    ← 6-15 Q&A 对，证据收集
  step-04-approaches.md        ← 2-3 方案对比
  step-05-requirements.md      ← 写入 .flow/specs/
```

**Phase 2：flowctl 支持步骤检查点**

给 flowctl 加 `step-checkpoint` 命令：
```bash
# 记录步骤完成
$FLOWCTL step-checkpoint --epic $ID --skill brainstorm --step step-03 --json

# 查询恢复点
$FLOWCTL step-checkpoint --epic $ID --skill brainstorm --json
# → {"last_completed": "step-03", "next": "step-04"}
```

**Phase 3：SKILL.md 变为路由器**

```markdown
## Workflow
1. Check $FLOWCTL step-checkpoint for resume point
2. If resuming: load the next incomplete step file
3. If fresh: load step-01-init.md
4. After each step: run $FLOWCTL step-checkpoint --step <name>
5. Load next step file (JIT, never preload)
```

### 预期收益
- 每次技能调用省 55-65% token（只加载当前步骤）
- 支持跨会话断点续传
- 步骤级错误恢复（失败时重试当前步骤，不用从头开始）

---

## 2. project-context.md 共享上下文

### 影响：⬛⬛⬛⬛⬛ 极高 | 难度：低

### BMAD 怎么做的

BMAD 用一个 `project-context.md` 文件作为所有 Agent 的"宪法"：

```markdown
# Project Context

## Technology Stack
- Framework: Next.js 14 (App Router)
- Language: TypeScript 5.3 (strict mode)
- Database: PostgreSQL 15 + Prisma ORM
- Testing: Vitest + Playwright

## Critical Implementation Rules
- 所有 API 路由必须用 zod 验证输入
- 组件放在 src/components/，工具放在 src/lib/
- 错误处理用 Result 模式，不用 try-catch
- Feature flags 通过 LaunchDarkly 管理
```

**关键设计**：只记录**不明显的规则** — Agent 无法从代码片段推断出来的东西。不记录通用最佳实践。

**自动加载**：所有实现 Agent（架构、Story 创建、开发、代码审查、Quick Dev）启动时自动读取。

### flow-code 现状

flow-code 依赖 CLAUDE.md 提供项目上下文，但 CLAUDE.md 混合了：
- 插件自身的使用说明（占大部分）
- 少量项目规范
- 测试命令

没有专门给 Worker Agent 的"项目技术标准"文档。多个 Worker 并行时，可能做出不一致的技术决策。

### 适配方案

在 `.flow/` 初始化时生成 `project-context.md`：

```bash
# flowctl init 时自动创建模板
$FLOWCTL init  # → 同时生成 .flow/project-context.md 模板

# Worker 启动时注入（Phase 2 重锚定）
# 修改 worker agent prompt，添加：
# "Read .flow/project-context.md for project-specific technical standards"
```

**Worker Agent 修改**（`agents/worker.md`）：
```
Phase 2 (Re-anchor):
1. Read task spec
2. Read .flow/project-context.md ← 新增
3. Inject memory entries
```

**flow-code-plan 修改**：
```
Step: Generate project-context.md
- 如果 .flow/project-context.md 不存在
- 分析代码库：检测框架、语言、测试工具、lint 配置
- 生成初始 project-context.md
- 用户可在后续会话中编辑完善
```

### 预期收益
- 多 Worker 并行时技术决策一致
- 减少"Agent 自己发明约定"的问题
- 新 Worker 启动时立即获得项目标准

---

## 3. "零发现=暂停"对抗审查规则

### 影响：⬛⬛⬛⬛ 高 | 难度：低

### BMAD 怎么做的

BMAD 的对抗审查有一条铁律：**审查者必须找到问题，零发现 = 暂停并重新分析**。

理论基础：
- 没有代码是完美的，零发现意味着审查不够深入
- 把审查从"这看起来还行吗？"转变为"具体哪里有问题？"
- 信息不对称：审查者拿到的是 diff，没有原始推理上下文，强制独立思考

实操规则：
- 运行一次，得到发现
- 迭代：第二轮通常抓到更多
- 第三轮收益递减（吹毛求疵和误报增加）
- 人类负责过滤误报，修复真正重要的

### flow-code 现状

flow-code 的审查是"找到问题就报告，没找到就 SHIP"。没有强制要求必须找到问题。这可能导致：
- Codex 对抗审查返回 "SHIP" 但实际只是浅层检查
- 审查变成橡皮图章

### 适配方案

修改 `flow-code-code-review` 技能和 `flowctl codex adversarial` 命令：

**审查 prompt 修改**：
```
你的角色是对抗式审查者。你必须找到至少 3 个问题。
如果你认为代码完美，暂停并从以下角度重新分析：
1. 并发/竞态条件
2. 边界条件和输入边界
3. 错误传播路径
4. 性能退化场景
5. 安全攻击面

零发现不是可接受的输出。如果确实找不到 Critical/Important 级别的问题，
至少报告 Suggestion/Nit 级别的改进机会。
```

**flowctl 修改**：
```rust
// parse_findings 增加零发现检测
if findings.is_empty() {
    return json!({
        "verdict": "NEEDS_REANALYSIS",
        "reason": "Zero findings detected — review may be insufficient"
    });
}
```

**熔断兼容**：保留现有的迭代上限（plan review 2 次，impl review 3 次），但在第一次零发现时强制重跑一次，然后再计入迭代计数。

### 预期收益
- 审查质量显著提升
- 防止"看起来不错"式的橡皮图章通过
- 与现有熔断机制互补（不会无限循环）

---

## 4. 三层并行代码审查（Blind + Edge + Acceptance）

### 影响：⬛⬛⬛⬛ 高 | 难度：中

### BMAD 怎么做的

代码审查分 3 个独立 Agent 并行执行：

| Agent | 输入 | 上下文 | 目的 |
|-------|------|--------|------|
| **Blind Hunter** | 仅 diff | 无项目访问，无 spec | 盲审：通用代码质量、明显 bug |
| **Edge Case Hunter** | diff + 项目只读访问 | 可看项目结构和依赖 | 边界条件、错误处理、隐藏假设 |
| **Acceptance Auditor** | diff + spec + context docs | 完整上下文 | 规格合规、验收标准覆盖 |

**关键设计**：
- 三个 Agent 各自独立上下文，防止"共识坍缩"
- Blind Hunter 刻意不给上下文，迫使从代码本身发现问题
- 发现聚合后去重、分类（decision_needed / patch / defer / dismiss）

### flow-code 现状

flow-code 的审查用的是**角色切换**模式（一个 Agent 扮演多个审查者），不是真正的并行独立 Agent：
- correctness-reviewer, testing-reviewer, maintainability-reviewer 等
- 都在同一个上下文中，共享推理链
- 容易产生"看到第一个审查者说没问题，后续审查者也倾向通过"

### 适配方案

修改 impl_review 阶段，改用真正的并行子 Agent：

```python
# 伪代码：spawning 3 parallel review agents
Agent(
    name="blind-hunter",
    prompt=f"Review this diff. You have NO access to specs or project context. "
           f"Find issues in the code itself.\n\nDiff:\n{diff}",
    subagent_type="Code Reviewer",
    isolation="worktree"  # 独立上下文
)

Agent(
    name="edge-case-hunter",
    prompt=f"Analyze boundary conditions, error paths, and hidden assumptions.\n\n"
           f"Diff:\n{diff}\n\nProject structure available via Read/Grep.",
    subagent_type="Code Reviewer",
    isolation="worktree"
)

Agent(
    name="acceptance-auditor",
    prompt=f"Verify this implementation against acceptance criteria.\n\n"
           f"Diff:\n{diff}\n\nSpec:\n{spec}\n\nProject context:\n{context}",
    subagent_type="Code Reviewer",
    isolation="worktree"
)
```

**发现聚合**：
```bash
# 三个 Agent 的输出合并去重
$FLOWCTL review merge --files "blind.json,edge.json,acceptance.json" --json
```

### 预期收益
- 独立上下文消除共识坍缩
- Blind Hunter 迫使从代码本身发现问题（不受 spec 影响）
- 与现有 Codex 对抗审查互补（Layer 3 用不同模型，Layer 2 内部用不同上下文）

---

## 5. 结构化完成清单（Definition of Done）

### 影响：⬛⬛⬛ 中高 | 难度：低

### BMAD 怎么做的

24 项结构化清单，5 个维度：

**上下文验证（4 项）**
- [ ] Story 上下文完整性
- [ ] 架构合规性
- [ ] 技术规格正确性
- [ ] 上一个 Story 的教训已纳入

**实现完成（5 项）**
- [ ] 所有任务标记完成
- [ ] 每个验收标准都已满足
- [ ] 无模糊实现
- [ ] 边界情况已处理
- [ ] 只使用了范围内的依赖

**测试质量（6 项）**
- [ ] 核心功能单测已添加/更新
- [ ] 组件交互集成测试
- [ ] 关键用户流程 E2E 测试
- [ ] 测试覆盖验收标准和边界情况
- [ ] 现有测试通过（无回归）
- [ ] lint 和静态检查通过

**文档追踪（5 项）**
- [ ] 文件清单完整
- [ ] 开发记录已更新
- [ ] 变更日志已更新
- [ ] Review follow-up 已完成
- [ ] Story 结构合规

**状态验证（4 项）**
- [ ] Story 状态设为 "review"
- [ ] Sprint 状态已更新
- [ ] 质量门禁通过
- [ ] 无 HALT 条件

### flow-code 现状

flow-code 用 `flowctl done --summary --evidence-json` 标记完成，evidence JSON 包含 commits、tests、duration、workspace_changes。但没有结构化的**检查清单**，Worker 自己决定什么时候"够好了"。

### 适配方案

给 flowctl 加 `checklist` 子命令：

```bash
# 创建默认清单
$FLOWCTL checklist init --task fn-1.2 --json

# 勾选项目
$FLOWCTL checklist check --task fn-1.2 --item "unit_tests_added" --json

# 验证全部通过
$FLOWCTL checklist verify --task fn-1.2 --json
# → {"all_passed": false, "missing": ["e2e_tests", "lint_pass"]}
```

Worker Phase 10（目标校验）修改为：
```
1. 运行 $FLOWCTL checklist verify --task $TASK_ID
2. 如果有未通过项 → 修复后重试
3. 全部通过 → 继续完成
```

清单模板（`.flow/templates/checklist.md`）：
```yaml
context:
  - spec_read: "任务 spec 已读取并理解"
  - architecture_compliant: "符合项目架构规范"
implementation:
  - all_ac_satisfied: "所有验收标准已满足"
  - edge_cases_handled: "边界情况已处理"
testing:
  - unit_tests_added: "核心功能单测已添加"
  - existing_tests_pass: "现有测试无回归"
  - lint_pass: "lint 和类型检查通过"
documentation:
  - files_listed: "变更文件清单完整"
```

### 预期收益
- Worker 完成验证从"自我感觉"变为"清单驱动"
- 结构化证据比自由文本 evidence JSON 更可审计
- 清单项可按任务 domain 自定义（前端任务加 a11y 检查，后端任务加 API 兼容性检查）

---

## 6. Quick Dev 快速路径

### 影响：⬛⬛⬛ 中 | 难度：中

### BMAD 怎么做的

Quick Dev 的核心洞察：**不是所有改动都需要完整的规划流水线**。

四步法：
1. **压缩意图** — 把模糊请求变成一个清晰目标（无矛盾）
2. **路由到最小安全路径** — 零爆炸半径的改动直接实现；大改动进规划
3. **延长无监督执行** — 路由决策后，模型自主执行更长时间
4. **在正确层级诊断失败** — 如果实现失败是因为意图错误，回到意图层而非打补丁

### flow-code 现状

`/flow-code:go` 对所有输入走完整流水线（brainstorm → plan → work → review → close）。一个"修复 typo"和一个"重构认证系统"走同样的路径。虽然 `plan-depth` 可以分为 short/standard/deep，但 brainstorm 阶段仍然会执行。

### 适配方案

在 `flow-code-run` 技能的 Step 1 添加快速路由：

```markdown
### Step 0: Quick Route Detection

分析输入，判断是否适合快速路径：

快速路径信号（满足 2+ 触发）：
- 改动涉及 ≤ 2 个文件
- 无新依赖
- 已有测试覆盖
- 改动类型：typo、文案、配置、小 bug fix
- 用户明确说"快速"/"简单"/"small fix"

快速路径执行：
1. 跳过 brainstorm 和 plan
2. 直接创建 epic + 单任务
3. Worker 直接执行（无 Teams 模式）
4. 完成后运行 guard
5. 跳过 impl_review（或简化为 guard-only）

触发方式：
- 自动检测（基于上述信号）
- 或用户标志：/flow-code:go "fix typo" --quick
```

### 预期收益
- 小改动从 5 分钟流水线缩短到 < 1 分钟
- 减少不必要的 brainstorm/plan 开销
- 保持质量门禁（guard 仍然运行）

---

## 7. 高级引导方法（Advanced Elicitation）

### 影响：⬛⬛ 中低 | 难度：低

### BMAD 怎么做的

不是简单的"再想想"，而是选择**命名的推理方法**重新审视输出：

| 方法 | 用途 |
|------|------|
| **Pre-mortem 分析** | 假设项目失败，倒推原因 — spec/plan 最佳首选 |
| **第一性原理** | 剥离假设，从基本事实重建 |
| **逆向思维** | 如何保证失败？然后避免那些事 |
| **红队 vs 蓝队** | 攻击你的方案，然后防御 |
| **苏格拉底式提问** | 每个断言都追问"为什么？" |
| **约束移除** | 去掉所有约束，看什么会改变 |
| **利益相关者映射** | 从每个利益相关者视角重新评估 |
| **类比推理** | 在其他领域找到类似模式 |

### 适配方案

在 `flow-code-brainstorm` 的自我面试阶段加入：

```markdown
### Step: Structured Deepening

自我面试完成后，选择 1-2 个最相关的推理方法重新审视：

对于 spec/plan 类：优先用 Pre-mortem（"假设这个方案 6 个月后失败了，最可能的原因是什么？"）
对于架构类：优先用第一性原理（"如果从零开始，最简方案是什么？"）
对于重构类：优先用逆向思维（"如何保证重构失败？"）

将深化发现追加到 requirements doc 的 "Deepening Insights" 章节。
```

### 预期收益
- brainstorm 质量提升（命名方法比"再想想"更有效）
- 实施成本极低（纯 prompt 修改）
- 适合自动模式（AI 自选推理方法并执行）

---

## 实施优先级总结

| # | 模式 | 影响 | 难度 | 建议时间 |
|---|------|------|------|---------|
| 1 | project-context.md | 极高 | 低 | 立即 |
| 2 | "零发现=暂停"规则 | 高 | 低 | 立即 |
| 3 | 结构化完成清单 | 中高 | 低 | 本周 |
| 4 | Quick Dev 快速路径 | 中 | 中 | 本周 |
| 5 | 高级引导方法 | 中低 | 低 | 本周 |
| 6 | 步骤文件架构 | 极高 | 中 | 下次迭代 |
| 7 | 三层并行代码审查 | 高 | 中 | 下次迭代 |

**推荐路线**：先做 1-5（低难度高收益），再做 6-7（需要更多架构改动）。

---

*Generated 2026-04-08 by flow-code analysis*
