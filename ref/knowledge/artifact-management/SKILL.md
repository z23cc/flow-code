---
name: artifact-management
description: >
  Artifact 文件（spec/task/implementation_plan/walkthrough/findings）的格式规范、
  管理规则和变更 ID（CHG-ID）管理。当创建或编辑任何 Artifact 文件时自动激活——
  包括 spec.md、task.md、implementation_plan.md、walkthrough.md、findings.md。
  任何涉及任务编号 T-NNN、变更 ID CHG-YYYYMMDD-NN、状态标记 [ ]/[/]/[x]/[-]/[!]、
  归档操作 ARCHIVE 的操作都应参考此 skill。即使只是修改一个任务状态或添加一条索引，
  也请先查阅。
---

# Artifact 文件管理规则

管理 5 个核心 Artifact 文件的创建、更新和变更追踪。

> **存储位置**：Obsidian Vault（`VAULT_PATH/projects/<projectName>/`）。`getArtifactDir(cwd)` 为唯一路径解析器，hook 自动将 CWD 路径重定向到 vault。`.pace/` 运行时状态保留在项目 CWD。

---

## 核心文件与操作规则

| 文件 | 用途 | 更新方式 | 限制 |
|------|------|----------|------|
| `spec.md` | 项目元数据与技术栈 | `Edit` 直接修改 | 无 ARCHIVE 标记 |
| `task.md` | 任务分解与进度 | `Edit` 活跃区改状态 | **禁止 Write 覆盖** |
| `implementation_plan.md` | 变更方案 | `Edit` 活跃区更新 | **禁止 Write 覆盖** |
| `walkthrough.md` | 工作总结 | `Edit` 活跃区更新 | **禁止 Write 覆盖** |
| `findings.md` | 调研记录 | `Edit` 活跃区更新 | **禁止 Write 覆盖** |

- 文件**不存在**时：用 `Write` 创建（hooks 自动注入模板）
- 文件**已存在**时：用 `Edit` 修改（hook 会 DENY Write 覆盖）
- 例外：文件损坏经用户确认后允许 Write 重建

> Artifact 文件约束的权威定义在 **User Rule G-7**。

---

## 双区结构

除 `spec.md` 外，其他 4 个文件用 `<!-- ARCHIVE -->` 分为活跃区和归档区：

```
┌─ 活跃区（ARCHIVE 上方）─┐  ← 当前状态、未完成项
├─ <!-- ARCHIVE --> ──────┤  ← 分隔标记（仅保留 1 个）
├─ 归档区（ARCHIVE 下方）─┤  ← 已完成的历史记录
└─────────────────────────┘
```

**排列顺序**（统一倒序：新→旧）：**所有索引和归档区**一律新增条目插入顶部。这是强制规范，不是建议。违反排列顺序会导致 SessionStart 截断时丢失最新条目。

**归档 = 移动标记**：内容不动，`<!-- ARCHIVE -->` 标记上移。分两步 Edit：
1. 在待归档内容**上方**插入新 `<!-- ARCHIVE -->`
2. 删除旧 `<!-- ARCHIVE -->`

每步只涉及标记行 + 几行上下文，内容零接触。中间态（双标记）安全：`readActive()` 取第一个标记上方，内容不丢失。
适用条件：待归档内容须在活跃区底部（紧邻 ARCHIVE 上方）。不满足时（如 findings 中间项已解决），留在活跃区等底部项一起归档。

**task.md 归档时机**：`[x]`/`[-]` 任务应及时归档。PostToolUse hook 会检测并提醒。

---

## 状态标记

### 任务状态

| 标记 | 含义 | 转换 |
|------|------|------|
| `[ ]` | 未开始 | → `[/]` 或 `[-]` |
| `[/]` | 进行中 | → `[x]`、`[!]`、`[-]` |
| `[x]` | 完成 | 终态 |
| `[!]` | 阻塞 | ↔ `[/]`（须说明原因） |
| `[-]` | 跳过 | 终态（须说明原因） |

`[P]` 标记表示可并行执行：`- [ ] T-001 [P] 任务描述`

### 变更状态

| 标记 | 含义 | 触发 |
|------|------|------|
| `[ ]` 规划中 | 方案已创建 | A 阶段完成 |
| `[/]` 进行中 | 用户已批准 | C 阶段确认 |
| `[x]` 完成 | 所有任务完成 | E 阶段完成 |
| `[-]` 废弃 | 需求取消 | 用户明确取消 |
| `[!]` 暂停 | 外部阻塞 | → `[/]` 恢复 |

### 调研状态（findings.md 独立含义体系）

| 标记 | 含义 |
|------|------|
| `[x]` | 已采纳/已验证 |
| `[-]` | 已否定（须注明理由） |
| `[ ]` | 待评估 |

### findings 索引完整格式

每条 finding 索引必须包含以下元素（按顺序）：

```
- [状态] 标题 — 关键结论 #finding [date:: YYYY-MM-DD] [change:: CHG-ID] [knowledge:: slug]
```

| 元素 | 必须 | 添加时机 |
|------|------|---------|
| `[状态]` | ✅ | 创建时 `[ ]` |
| `标题 — 关键结论` | ✅ | 创建时 |
| `#finding` | ✅ | 创建时 |
| `[date:: YYYY-MM-DD]` | ✅ | 创建时 |
| `[change:: CHG-ID]` | 有活跃变更时 ✅ | 创建时关联当前 CHG-ID；独立调研不加 |
| `[knowledge:: slug]` | 提取后 ✅ | 执行 Findings→Knowledge SOP 后回写 |

### APPROVED / VERIFIED 标记决策树

```
新建变更 → 不加标记（等 C 阶段）
C 阶段用户批准 → <!-- APPROVED -->
HOTFIX 降级（用户给完整需求）→ 自动 <!-- APPROVED -->
Bridge 桥接（用户参与设计）→ 自动 <!-- APPROVED -->
V 阶段验证通过 → <!-- VERIFIED -->（放在 APPROVED 下方）
```

> Hook 强制：PreToolUse 检查 APPROVED + `[/]` 任务；Stop 检查 `[x]` 无 VERIFIED。

---

## 编号规范

**任务编号 T-NNN**：三位数全局递增。读取 task.md 最大编号 +1，无现有任务从 T-001 开始。

**变更 ID**：`CHG-YYYYMMDD-NN`（常规）/ `HOTFIX-YYYYMMDD-NN`（紧急）。读取 implementation_plan.md 索引中当日最大序号 +1。

> T-NNN 标识单个任务，CHG-ID 标识一组关联任务的变更——两个独立体系。

---

## 变更管理快速开始

检测到复杂任务（PACE A 阶段）时：

1. **检查 implementation_plan.md** — 不存在则用 [templates/change-implementation_plan.md](templates/change-implementation_plan.md) 创建
2. **生成变更 ID** — 读取索引中当日最大序号 +1：`CHG-{YYYYMMDD}-{NN}`
3. **更新索引** — 活跃区顶部插入：`- [ ] CHG-ID 标题 #change [tasks:: T-NNN~T-NNN]`
4. **追加详情** — 在 `## 活跃变更详情` 区追加 `### CHG-ID 标题` 段落（**背景（Why）** + **范围（What）** + **技术决策（How）** + **任务分解**：每个 T-NNN 含 `file:line` 定位+改动意图+验收条件）
5. **随进度更新** — 批准后 `[/]`，完成后 `[x]`

> 索引条目**必须**有对应 `### CHG-ID` 详情段落，否则 hook 会 DENY。
> 完整 PACE 集成生命周期和跨 Artifact 联动规则见 [references/change-lifecycle.md](references/change-lifecycle.md)。

---

## 时间戳格式

`YYYY-MM-DDTHH:mm:ss+08:00`（遵循 User Rule G-9）

---

## 格式要求

Hook 检测正则为行首 Markdown checkbox：`- [/] CHG-...`、`- [x] T-...`

**禁止**：表格格式 `| [/] | CHG-... |` 或 emoji 状态 — hook 无法识别。

> 完整格式示例和常见错误速查见 [references/format-reference.md](references/format-reference.md)。

---

## 内容深度要求

> **原则**：可追溯但追溯到空壳等于没追溯。每个 artifact 必须**内容完整到能独立理解**，不依赖其他文件或上下文。参考 pace-knowledge 的 L0/L1/L2 分层模式。

### task.md 任务描述

```
- [ ] T-NNN 任务标题（验收：可验证的完成条件）
```

示例：`- [ ] T-004 修复 getUserSettings null 检查（验收：/api/settings?userId=null 返回 200 + 默认设置）`

### implementation_plan.md 变更详情

每个 `### CHG-ID` 详情段落必须包含 4 段：

1. **背景**（Why）：为什么需要这个变更
2. **范围**（What）：影响哪些文件，预估改动量
3. **技术决策**（How）：选择的方案及理由（如有替代方案需说明为什么不选）
4. **任务分解**：每个 T-NNN 展开为多行，包含三要素：
   - **文件定位**：`file:line` 或 `file:函数名`（精确到修改位置）
   - **改动意图**：当前行为 → 目标行为（不是"具体改动说明"这样的模糊描述）
   - **验收条件**：`验收：` 开头，可验证的完成标准
   - 验证类任务（如"全量验证"）可单行不展开

### findings.md 详情

每条 finding 的 `### 标题` 详情段落必须包含：

| 要素 | 必须 | 说明 |
|------|------|------|
| 现象（含代码位置） | ✅ | 哪个文件哪一行出了什么问题 |
| 根因（含代码片段） | ✅ | 问题代码是什么，为什么错 |
| 影响范围 | ✅ | 影响多大，是否阻塞 |
| 建议方案（含实现要点） | ✅ | 怎么修，改哪些文件 |
| 收益/风险评估 | 建议 | 修了能得到什么，有什么风险 |
| 优先级 P0-P3 | 建议 | 紧急程度 |

> 5-10 行的 finding 是"摘要"，不是"详情"。生产级 finding 通常 30-60 行。

### walkthrough.md 详情

每个 `## YYYY-MM-DD` 详情段落必须包含：

1. **执行摘要**：一句话概括今天做了什么
2. **每个 T-NNN 的变更要点**：修改了什么文件、新增了什么功能、关键实现细节
3. **验证结果**：测试通过情况（语法/单元/E2E/手动验证）
4. **附带修复**（如有）：执行过程中顺带修复的问题

### Corrections 记录

每条 Correction 必须包含 4 要素 + knowledge 评估（详见 `paceflow:pace-knowledge` Corrections 双写流程）：

```
### Correction: 标题
- **错误行为**：做了什么错事
- **正确做法**：应该怎么做
- **触发场景**：什么情况下容易犯
- **根本原因**：为什么会犯
- [knowledge:: slug 或 project-only]
```

### findings 记录时的联动检查

记录新 finding 后，**必须评估**：
1. 当前有活跃变更？→ 索引补 `[change:: CHG-ID]`
2. 该 finding 是否具有跨项目通用价值？→ 是则触发 `paceflow:pace-knowledge` Findings→Knowledge SOP，完成后回写 `[knowledge:: slug]`

---

## 关联 Skill

- **`paceflow:pace-workflow`** — PACE P-A-C-E-V 流程，A 阶段调用本 skill 的变更管理
- **`paceflow:pace-knowledge`** — findings.md 中发现跨项目通用经验时，提取到 knowledge/
