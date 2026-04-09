# flow-code ADR 策略设计

> 基于开源最佳实践 + 现有架构分析 | 2026-04-09

---

## 核心洞察

flow-code 已经有了 ADR 集成的**完整基础设施**，只是没有串起来：

| 已有组件 | 作用 | ADR 角色 |
|---------|------|---------|
| `flowctl invariant add --verify` | 注册架构规则 + 验证命令 | **ADR 执行器** — 每个 ADR 的 verify 命令注册为 invariant |
| `flowctl guard` | 质量门禁 | **ADR 合规层** — guard 调 `invariants check` |
| `project-context.md` Non-Goals | 已解析的禁止项 | **ADR 约束源** — Non-Goals 即 ADR 的 Constraint |
| `project-context.md` Architecture Decisions | 已解析的决策 | **轻量 ADR** — 一行式 Y-Statement |
| brainstorm/plan 读 Non-Goals | 避免提议已排除方案 | **ADR 预检** — 规划前检查 |
| Acceptance Auditor 检查 Non-Goals | 审查合规 | **ADR 后验** — 代码审查时检查 |

**关键发现：不需要造新轮子。把 ADR 的 verify 命令注册到 `flowctl invariant`，guard 就自动执行合规检查。**

---

## 方案设计

### ADR 格式：MADR v3 精简版 + verify 字段

```markdown
---
id: ADR-004
status: accepted
date: 2026-04-09
tags: [core, state-machine]
verify: "cargo test --test adr_004"
scope: "flowctl/crates/flowctl-core/src/state_machine.rs"
---
# ADR-004: Eight-State Task Machine

## Context
任务需要比 3 态（pending/running/done）更丰富的状态来支持重试、级联失败、手动跳过。

## Decision
8 种状态：todo, in_progress, done, blocked, skipped, failed, up_for_retry, upstream_failed。
状态转换有形式化验证（state_machine.rs），非法转换编译期报错。

## Consequences
- **约束**：新增状态必须更新转换表和所有 match 分支
- **收益**：Worker 超时 → failed → 自动传播 upstream_failed 到下游
- **代价**：8 态比 3 态复杂，match 分支更多

## Rejected Alternatives
- 3 态（pending/running/done）：无法表达 blocked/failed，Worker 超时无法恢复
- 5 态（加 blocked + failed）：缺少 skipped 和级联传播
```

**关键字段**：
- `verify`：shell 命令，返回 0 = 合规（注册到 `flowctl invariant`）
- `scope`：影响的文件范围（guard 只对范围内文件的改动触发检查）

### ADR 存储

```
docs/decisions/
  ADR-001-pure-file-based-state.md      ← 已有
  ADR-002-three-layer-quality-gates.md  ← 已有
  ADR-003-rust-single-binary.md         ← 已有
  ADR-004-eight-state-task-machine.md   ← 新增
  ADR-005-project-context-markdown.md   ← 新增
  ADR-006-regex-code-structure.md       ← 新增
  ADR-007-nucleo-fuzzy-search.md        ← 新增
  ADR-008-frecency-exponential-decay.md ← 新增
  ADR-009-wave-checkpoint-concurrency.md← 新增
  ADR-010-zero-interaction-default.md   ← 新增
```

### 集成到 flowctl 现有命令

**不需要新命令。** 利用已有的 `flowctl invariant`：

```bash
# 每个 ADR 的 verify 命令注册为 invariant
flowctl invariant add \
  --name "ADR-001: No database imports" \
  --verify "! grep -rn 'libsql\|rusqlite\|diesel' flowctl/crates/"

flowctl invariant add \
  --name "ADR-003: No unsafe code" \
  --verify "! grep -rn 'unsafe ' flowctl/crates/ --include='*.rs' | grep -v 'forbid(unsafe'"

flowctl invariant add \
  --name "ADR-006: No tree-sitter deps" \
  --verify "! grep -n 'tree-sitter' flowctl/Cargo.toml"

# guard 自动执行所有 invariant 检查
flowctl guard  # → 包含 invariants check
```

### 集成到流水线

```
brainstorm
  └── 读 docs/decisions/ 所有 ADR 的 Context + Decision
  └── 读 project-context.md Non-Goals
  └── 避免提议 Rejected Alternatives

plan  
  └── 如果新功能涉及 ADR scope 内的文件，引用对应 ADR
  └── 如果决策需要新 ADR，在 spec 中标注 "需要 ADR-NNN"

plan_review
  └── 验证 plan 不违反已有 ADR 的 Constraint

work (Worker Phase 6)
  └── flowctl guard → 含 invariants check → 验证 ADR 合规

impl_review
  └── Acceptance Auditor 检查 Non-Goals + ADR Constraints

close
  └── 验证所有 invariant（含 ADR verify）通过
  └── 如果有标注"需要 ADR"的决策，验证 ADR 已写
```

### 补充的 7 个 ADR 内容

| ADR | 核心决策 | verify 命令 |
|-----|---------|------------|
| **004: 八态状态机** | 8 种任务状态 + 形式化转换 | `cargo test state_machine` |
| **005: Markdown 项目上下文** | project-context.md 解析 Markdown 而非 JSON | `test -f templates/project-context.md` |
| **006: Regex 代码结构** | regex 提取而非 tree-sitter（暂时） | `! grep 'tree-sitter' flowctl/Cargo.toml` |
| **007: nucleo 模糊搜索** | nucleo-matcher 而非 frizbee/skim | `grep 'nucleo-matcher' flowctl/Cargo.toml` |
| **008: Frecency 指数衰减** | 14 天半衰期 + 权重 3.0/2.0/1.0 | `grep 'HALF_LIFE_DAYS.*14' flowctl/crates/flowctl-core/src/frecency.rs` |
| **009: 波次检查点并发** | Wave-Checkpoint-Wave + 文件锁 | `grep 'fs2' flowctl/Cargo.toml` |
| **010: 零交互默认** | /flow-code:go 不问问题 | `grep 'ZERO-INTERACTION' skills/flow-code-run/SKILL.md` |

### 为什么不需要 `flowctl adr` 命令

1. **ADR 就是 Markdown 文件** — `Write` 工具直接创建
2. **verify 命令通过 `flowctl invariant add` 注册** — 已有命令
3. **合规检查通过 `flowctl guard` 执行** — guard 已调 `invariants check`
4. **ADR 列表就是 `ls docs/decisions/`** — 不需要专门命令

**零新代码。** 完全靠已有基础设施串联。

---

## 与 project-context.md 的关系

```
project-context.md                      docs/decisions/ADR-*.md
├── Architecture Decisions  ←──────────  轻量版（一行摘要）
├── Non-Goals              ←──────────  ADR Rejected Alternatives 的汇总
├── Critical Rules         ←──────────  ADR Constraints 的汇总
└── Guard Commands         ←──────────  ADR verify 命令的运行层
```

**project-context.md 是 ADR 的摘要视图**：
- Architecture Decisions 段 = 所有 accepted ADR 的一行摘要
- Non-Goals 段 = 所有 ADR 的 Rejected Alternatives 汇总
- Critical Rules 段 = 所有 ADR 的 Constraint 汇总

Worker 读 project-context.md（紧凑），需要深入理解时读完整 ADR。

---

## 实施步骤

1. **给现有 3 个 ADR 加 YAML frontmatter**（verify + scope）
2. **写 7 个新 ADR**（ADR-004 到 ADR-010）
3. **注册所有 verify 命令为 invariant**
4. **更新 project-context.md**（从 ADR 同步 Architecture Decisions 段）
5. **更新 plan 技能**（规划时读 ADR 目录）

总工作量：~10 个 Markdown 文件 + 若干 `flowctl invariant add` 命令。零 Rust 代码。

---

*Generated 2026-04-09 by flow-code analysis*
