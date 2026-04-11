# flow-code V3 Final Architecture: Goal-Driven Adaptive Engine over MCP

> **Status**: Design Document — Ready for Implementation
> **Date**: 2026-04-11
> **Author**: z23cc + Claude Opus 4.6
> **Supersedes**: V1 (pipeline state machine), V2 (pure fitness-function), V3 draft (merged ExecutionMode)
> **Lineage**: V3 core ideas + V3.1 review corrections (orthogonal model, goal-scoped storage, policy engine, provider registry)

---

## 1. Executive Summary

flow-code V3 Final 是 V3 draft 和 V3.1 review 的融合版。三个核心转变不变：

1. **从 Pipeline 到 Goal**：不再是"走完 6 个阶段"，而是"让目标达成"
2. **从 CLI 到 MCP**：flowctl 从 Bash 调用的 CLI 工具变成持久运行的 MCP Server
3. **从静态 Skill 到知识复利**：52 个手写 SKILL.md 精简为 3 个入口 + 三层自动积累的知识金字塔

融合后的关键修正：

| 维度 | V3 Draft | V3.1 Review | Final 决策 |
|------|----------|-------------|-----------|
| 核心模型 | `ExecutionMode` 单 enum | `PlanningMode × SuccessModel` 正交 | **正交维度** |
| 存储布局 | 扁平目录 + 原地改 graph | Goal-scoped + PlanVersion 递增 | **Goal-scoped** |
| Hook 策略 | 几乎全部移入 MCP | PolicyEngine + adapters | **PolicyEngine + 2 adapters** |
| Provider | RP/Codex 写进内核 | trait Registry | **trait Registry** |
| Crate 拆分 | 3 crate | 5 crate | **3 crate**（core 稳定后再拆） |
| MCP Tool 数 | 16 个 | 12 个（合并 lock/knowledge） | **16 个**（Worker 需主动调用 lock/knowledge） |
| 前门数 | 3 个 slash commands | 5 个 | **3 个** + intent 参数 |
| 实施顺序 | 5 Phase 从代码开始 | 7 步从宪法改写开始 | **7 步**（先改约束文档） |

核心公式：**Goal × (PlanningMode, SuccessModel) × Escalation × Learn = 自适应引擎**

---

## 2. Design Principles

### 2.1 来源与致谢

本架构综合了 78 个开源项目的最佳实践：

| 原则 | 来源项目 | 核心洞察 |
|------|---------|---------|
| Fitness Function 驱动 | goal-md | 给 agent 一个分数脚本，让它自主优化 |
| 分层升级控制 | SWE-AF | inner loop（重试）→ middle（换策略）→ outer（重规划） |
| Hook 物理拦截 | PACEflow | 不是建议遵守流程，而是在 tool 层阻止违规 |
| 知识复利 | Compound Engineering | 每次工程循环产出的经验有衰减生命周期 |
| 自我改进方法论 | arscontexta | hook 不是静态的，agent 可以修改自己的方法 |
| 并行意见合成 | agent-council | 多个模型独立回答，Chairman 综合 |
| 风险比例资源分配 | SWE-AF | 简单任务轻量检查，复杂任务重度 QA |
| 跨 agent 知识传播 | SWE-AF + Hermes | 共享记忆层让后续 agent 避免重复错误 |
| 运行时 DAG 变形 | SWE-AF | 执行图是可变的运行时产物 |
| 闭合学习环 | Hermes Agent | 执行 → 记录 → 提取技能 → 改进 → 再执行 |
| 零 API 文件协调 | multi-agent-shogun | agent 间通信通过磁盘文件 |
| 原子 Worker | Spec-Flow | Worker 做完一个任务就退出，状态全在磁盘 |

### 2.2 Anthropic 指导原则

> "Start with the simplest solution that works. Add complexity only when simpler solutions demonstrably fail."
> — Anthropic, Building Effective Agents

> "Each subagent needs an objective, an output format, guidance on the tools and sources to use, and clear task boundaries."
> — Anthropic, Multi-Agent Research System

### 2.3 融合决策原则

1. **正交优于混合**：独立维度不要塞进同一个 enum
2. **聚合优于散装**：一个 goal 的所有数据放在一起
3. **物理阻断优于 prompt 建议**：规则在 tool 层拦截，不靠 LLM 自觉遵守
4. **延迟拆分**：crate/module 边界等代码量够大再切
5. **Worker 自主权**：Worker 保留 lock 和 knowledge 的主动调用权

---

## 3. Architecture Overview

### 3.1 Claude Code 运行模型

```
Claude（LLM）= CPU
SKILL.md     = 程序（指令）
flowctl MCP  = 数据库 + 运行时引擎
.flow/       = 持久存储
```

MCP Server 改变了调用模型：

```
之前（CLI）：Claude → Bash("flowctl status --json") → fork → 读文件 → stdout → 进程死亡
之后（MCP）：Claude → MCP tool(goal.status) → 持久进程内存操作 → 结构化返回
```

### 3.2 MCP Server 解锁的能力

| 纯 Plugin 约束 | MCP Server 解除 |
|----------------|----------------|
| 无后台进程 | MCP server 是持续运行的进程 |
| 无事件系统 | MCP notifications 机制 |
| 工具调用是字符串拼接 | 结构化 schema + 类型校验 |
| 无并发状态管理 | Server 内部管理并发状态 |
| 每次 Bash 调用有 fork 开销 | 持久连接，零 fork |

### 3.3 三层架构

```
┌─────────────────────────────────────────────────────────┐
│                    MCP Tool Layer                        │
│          16 个语义化 tools（Claude 的唯一接口）            │
├─────────────────────────────────────────────────────────┤
│                    Core Engine Layer                      │
│  Goal │ Planner │ Scheduler │ Quality │ Knowledge        │
│  PolicyEngine │ ProviderRegistry │ Escalation            │
├─────────────────────────────────────────────────────────┤
│                    Storage Layer                          │
│  .flow/goals/{id}/ │ .flow/knowledge/ │ .flow/runtime/   │
└─────────────────────────────────────────────────────────┘
```

### 3.4 Crate 结构（3 crate）

```
flowctl/crates/
├── flowctl-core/     # Domain types + Storage + Runtime engine
│   src/
│   ├── domain/       # Goal, Node, PlanVersion, Attempt, Status enums
│   ├── storage/      # GoalStore, KnowledgeStore, LockStore, EventLog
│   ├── engine/       # GoalEngine, Planner, Scheduler, Escalation
│   ├── knowledge/    # Learner, Pattern extraction, Compound, Decay
│   ├── quality/      # Guard, PolicyEngine, ReviewProtocol
│   ├── provider/     # ProviderRegistry, ReviewProvider trait, PlanningProvider trait
│   ├── graph/        # CodeGraph, NgramIndex (existing, preserved)
│   └── compat/       # Legacy Epic/Task/Phase types (read-only, for migration)
│
├── flowctl-mcp/      # MCP Server (stdio transport via rmcp)
│   src/
│   ├── server.rs     # MCP server setup, tool registration
│   ├── goal.rs       # goal.open / goal.status / goal.close
│   ├── plan.rs       # plan.build / plan.next / plan.mutate
│   ├── node.rs       # node.start / node.finish / node.fail
│   ├── quality.rs    # quality.run
│   ├── lock.rs       # lock.acquire / lock.release
│   ├── knowledge.rs  # knowledge.search / knowledge.record / knowledge.compound / knowledge.refresh
│   └── assess.rs     # codebase.assess
│
├── flowctl-cli/      # Thin CLI facade + legacy compat
│   src/
│   ├── main.rs       # Clap CLI: `serve` + goal/plan/node/knowledge subcommands
│   ├── output.rs     # JSON output (api_version, --json flag)
│   ├── legacy/       # phase next/done, worker-phase, epic CRUD (compat shim → core engine)
│   └── commands/     # Direct CLI wrappers for each MCP tool
```

**延迟拆分约定**：当 `flowctl-core` 超过 20K LOC 时，将 `domain/` + `storage/` 拆为 `flowctl-domain` 和 `flowctl-storage` 独立 crate。在此之前保持单 core crate 减少编译边界和 API 承诺。

---

## 4. Core Engine Design

### 4.1 Goal（取代 Epic + 6 Phases）

```rust
/// Goal 是 V3 的核心单位，取代 Epic + Pipeline。
/// 关键改进（V3.1）：PlanningMode 和 SuccessModel 是正交维度。
struct Goal {
    id: GoalId,                      // g-42-add-oauth
    request: String,                 // 用户原始请求
    intent: GoalIntent,              // Execute / Plan / Brainstorm
    planning_mode: PlanningMode,     // Direct / Graph
    success_model: SuccessModel,     // Criteria / Numeric / Mixed
    status: GoalStatus,              // Open / Active / Done / Failed
    current_plan_rev: u32,           // 当前使用的 PlanVersion 序号

    // Numeric 模式字段
    fitness_script: Option<String>,  // 可执行的评分脚本路径
    score_baseline: Option<f64>,     // 初始分数
    score_target: Option<f64>,       // 目标分数
    score_current: Option<f64>,      // 当前分数
    action_catalog: Vec<Action>,     // 可以涨分的操作列表

    // Criteria 模式字段
    acceptance_criteria: Vec<Criterion>,

    // 通用字段
    constraints: Vec<String>,        // ADR、invariants、non-goals
    known_facts: Vec<Fact>,          // 执行中持续更新的已知信息
    open_questions: Vec<String>,     // 未解答的问题
    providers: ProviderSet,          // 绑定的 review/planning providers
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// 用户意图——决定走多远
enum GoalIntent {
    Execute,     // 全流程：规划 + 执行 + 验证
    Plan,        // 只规划不执行
    Brainstorm,  // 只探索不规划
}

/// 规划方式——独立于成功判定
enum PlanningMode {
    Direct,  // 不拆图，单节点直做（≤2 个文件的简单改动）
    Graph,   // 生成执行图，多节点并行
}

/// 成功判定——独立于规划方式
enum SuccessModel {
    Criteria,  // 验收项驱动（全部 MET 即完成）
    Numeric,   // fitness function 驱动（score >= target 即完成）
    Mixed,     // 验收项是门槛 + 分数是加速器（criteria 必须全 MET，分数用于优先级排序）
}

/// 正交组合示例：
/// - Direct + Numeric  = "fix all lint errors"（直做，lint 错误数归零）
/// - Direct + Criteria  = "fix typo in README"（直做，改完即止）
/// - Graph + Numeric   = "提升覆盖率到 80%"（拆任务，按分数迭代）
/// - Graph + Criteria   = "add OAuth login"（拆任务，按验收项推进）
/// - Graph + Mixed     = "重构 auth 模块"（拆任务，验收为门槛，复杂度分数辅助排序）

/// 自动模式选择
fn assess_goal(request: &str, codebase: &CodebaseIndex) -> (PlanningMode, SuccessModel) {
    let file_estimate = codebase.estimate_affected_files(request);
    let has_metric = detect_natural_metric(request, codebase);
    let has_criteria = detect_acceptance_criteria(request);

    let planning = if file_estimate <= 2 && is_trivial(request) {
        PlanningMode::Direct
    } else {
        PlanningMode::Graph
    };

    let success = match (has_metric, has_criteria) {
        (true, false) => SuccessModel::Numeric,
        (false, _)    => SuccessModel::Criteria,
        (true, true)  => SuccessModel::Mixed,
    };

    (planning, success)
}
```

### 4.2 PlanVersion（V3.1 核心改进：显式版本化）

```rust
/// PlanVersion 是一次规划的快照。
/// replan 不修改已有版本——创建新版本。
/// 回滚 = 切换到旧版本。审计 = diff 两个版本。
struct PlanVersion {
    goal_id: GoalId,
    rev: u32,                    // 递增：0001, 0002, ...
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    rationale: String,           // 为什么生成/修改这个计划
    trigger: PlanTrigger,        // Initial / Replan / ScopeChange
    created_at: DateTime<Utc>,
}

enum PlanTrigger {
    Initial,                     // 首次规划
    Replan { reason: String },   // L3 升级触发的重规划
    ScopeChange { delta: String }, // 用户修改了需求
    ScoreRegression { from: f64, to: f64 }, // 分数下降触发
}
```

### 4.3 Node + Attempt（执行单位）

```rust
/// Node 是执行图中的一个任务节点。
/// Worker 执行一个 Node，产出一个 Attempt。
struct Node {
    id: NodeId,
    objective: String,           // 目标描述（不是步骤）
    constraints: Vec<String>,    // 约束
    owned_files: Vec<String>,    // 用于 locking
    risk: RiskProfile,           // IssueGuidance（SWE-AF）
    status: NodeStatus,          // Ready / InProgress / Done / Failed / Skipped
    injected_patterns: Vec<PatternRef>,  // 从知识库注入的经验
}

/// SWE-AF IssueGuidance：风险比例标注
struct RiskProfile {
    estimated_scope: Scope,      // Trivial / Small / Medium / Large
    needs_deeper_qa: bool,       // 是否需要深度 QA
    touches_interfaces: bool,    // 是否跨模块
    risk_rationale: String,      // 为什么这样判断
    guard_depth: GuardDepth,     // Trivial / Standard / Thorough
}

enum GuardDepth {
    Trivial,    // 只跑 guard（lint/type/test）
    Standard,   // guard + 自检
    Thorough,   // guard + 多轮 review + adversarial
}

enum NodeStatus {
    Ready,       // 所有依赖已满足
    InProgress,  // Worker 正在执行
    Done,        // 完成
    Failed,      // 失败（可能触发 escalation）
    Skipped,     // 跳过（视同 Done 用于依赖解析）
}

/// Attempt 是对一个 Node 的一次执行尝试。
/// 一个 Node 可能有多次 Attempt（重试/换策略后）。
struct Attempt {
    node_id: NodeId,
    seq: u32,                    // 第几次尝试
    summary: String,
    changed_files: Vec<String>,
    commits: Vec<String>,
    tests: Vec<TestResult>,
    findings: Vec<Finding>,
    suggested_mutations: Vec<GraphMutation>,  // Worker 可以建议图变形
    duration_seconds: u32,
    created_at: DateTime<Utc>,
}
```

### 4.4 三层分级升级控制（来自 SWE-AF）

```rust
enum EscalationLevel {
    /// L1: Worker 内部重试（换方法，不换目标）
    /// 触发条件：单个 node 执行失败
    /// 动作：换工具、换方法、换顺序重试
    WorkerRetry,

    /// L2: 策略层修改（换 action catalog，不重规划）
    /// 触发条件：L1 连续 3 次失败
    /// 动作：修改 action catalog、调整 node constraints、请求更多上下文
    StrategyChange,

    /// L3: 重规划（创建新 PlanVersion）
    /// 触发条件：L2 也失败
    /// 动作：拆分 node、插入新 node、修改依赖、甚至修改 goal
    Replan,
}

/// replan 返回建议（Mutations），不是直接修改。
/// Claude 看到建议后决定是否执行——保持人/AI 在环的控制力。
enum GraphMutation {
    AddNode { node: Node, deps: Vec<NodeId> },
    RemoveNode { id: NodeId },
    SplitNode { id: NodeId, into: Vec<Node>, chain: bool },
    SkipNode { id: NodeId, reason: String },
    AddEdge { from: NodeId, to: NodeId },
    RemoveEdge { from: NodeId, to: NodeId },
    UpdateConstraints { id: NodeId, new_constraints: Vec<String> },
}

impl Escalation {
    fn handle_failure(&self, node_id: NodeId, error: &str, history: &[Attempt]) -> EscalationAction {
        let consecutive_fails = history.iter().rev().take_while(|a| !a.findings.is_empty()).count();

        match consecutive_fails {
            0..=2 => EscalationAction::Retry {
                node_id,
                suggestion: self.suggest_alternative(node_id, error),
            },
            3..=4 => EscalationAction::ChangeStrategy {
                node_id,
                new_constraints: self.revise_constraints(node_id, error),
                catalog_update: self.suggest_catalog_change(error),
            },
            _ => EscalationAction::Replan {
                affected_nodes: self.impact_analysis(node_id),
                suggestion: self.suggest_graph_mutation(node_id, error),
                // Replan 创建新 PlanVersion，不修改当前版本
            },
        }
    }
}
```

### 4.5 三层知识金字塔（来自 Compound + Hermes + arscontexta）

```rust
/// Layer 1: Learning（原子经验，每次执行产出）
struct Learning {
    id: String,
    goal_id: String,
    node_id: Option<String>,
    kind: LearningKind,        // Success / Failure / Discovery / Pitfall
    content: String,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    verified: bool,            // 后续使用是否验证过
    use_count: u32,
}

/// Layer 2: Pattern（从多次 learnings 归纳，有衰减生命周期）
struct Pattern {
    id: String,
    name: String,
    description: String,
    approach: String,          // 推荐做法
    anti_patterns: Vec<String>,
    source_learnings: Vec<String>,
    confidence: f64,           // 0.0-1.0，随使用验证上升
    freshness: DateTime<Utc>,
    decay_days: u32,           // 默认 90 天
    use_count: u32,
}

/// Layer 3: Methodology（核心规则，arscontexta 模式：可被 agent 修改）
struct Methodology {
    rules: Vec<MethodRule>,
    last_revised: DateTime<Utc>,
    revision_trigger: String,
}

/// 知识管理引擎
struct Learner {
    learnings: Vec<Learning>,
    patterns: Vec<Pattern>,
    methodology: Methodology,
    index: NgramIndex,         // 复用已有 trigram 基础设施
}

impl Learner {
    /// Goal 完成后自动调用（也可被 Worker 中途主动调用）
    fn record(&mut self, goal_id: &str, node_id: &str, outcome: &str, kind: LearningKind) {
        let learning = Learning {
            goal_id: goal_id.to_string(),
            node_id: Some(node_id.to_string()),
            kind,
            content: outcome.to_string(),
            ..Default::default()
        };
        self.learnings.push(learning);
        self.index.add(&learning.content, &learning.id);
    }

    /// 为新 node 注入相关经验
    fn inject_for_node(&self, node: &Node) -> Vec<&Pattern> {
        self.index.search(&node.objective, 3)
    }

    /// 定期归纳：多个 learnings → 一个 pattern（Compound 模式）
    fn compound(&mut self, goal_id: &str) {
        // 1. 找到本次 goal 的所有 learnings
        // 2. 按 tags 聚类
        // 3. 某 tag 下有 3+ learnings → 归纳为 pattern
        // 4. 已有 pattern 被验证 → 提升 confidence
        // 5. 已有 pattern 被否定 → 降低 confidence
    }

    /// 衰减刷新（Compound Engineering 模式）
    fn refresh_stale(&mut self) {
        let now = Utc::now();
        for pattern in &mut self.patterns {
            let age = now - pattern.freshness;
            if age.num_days() > pattern.decay_days as i64 {
                pattern.confidence *= 0.8;
                // confidence < 0.3 → 标记需要人工审查或自动淘汰
            }
        }
    }

    /// 统一检索三层
    fn search(&self, query: &str, limit: usize) -> KnowledgeResult {
        KnowledgeResult {
            patterns: self.search_patterns(query, limit),
            learnings: self.search_learnings(query, limit),
            methodology_rules: self.match_rules(query),
        }
    }
}
```

### 4.6 Provider Registry（V3.1 改进：trait 抽象）

```rust
/// Provider 不写进内核——通过 trait 注册。
/// 将来换 RP、换 Codex、甚至换空实现，不污染核心状态模型。

trait ReviewProvider: Send + Sync {
    fn review(&self, diff: &str, spec: &str) -> Result<ReviewResult>;
    fn name(&self) -> &str;
}

trait PlanningProvider: Send + Sync {
    fn assess(&self, request: &str, context: &str) -> Result<Assessment>;
    fn name(&self) -> &str;
}

trait AskProvider: Send + Sync {
    fn ask(&self, question: &str, context: &str) -> Result<String>;
    fn name(&self) -> &str;
}

/// Registry 管理所有注册的 providers
struct ProviderRegistry {
    review_providers: HashMap<String, Box<dyn ReviewProvider>>,
    planning_providers: HashMap<String, Box<dyn PlanningProvider>>,
    ask_providers: HashMap<String, Box<dyn AskProvider>>,
    default_review: Option<String>,
    default_planning: Option<String>,
}

/// Goal 绑定特定 providers
struct ProviderSet {
    review: Option<String>,     // provider name → lookup from registry
    planning: Option<String>,
    ask: Option<String>,
}

/// 内置 provider 实现
struct NoneProvider;             // 空实现（跳过）
struct RpProvider { tier: RpTier }   // RepoPrompt (MCP/CLI/None)
struct CodexProvider;            // Codex adversarial
// 未来可加：struct OllamaProvider, struct CustomScriptProvider, ...
```

### 4.7 PolicyEngine（V3.1 改进：物理阻断）

```rust
/// PolicyEngine 统一管理所有治理规则。
/// 关键洞察（PACEflow）：prompt 建议只有 ~70% 遵守率，物理阻断是 100%。
///
/// Claude 可以绕过 MCP 直接调用 Edit/Write/Bash。
/// 所以不能只在 MCP tool 内部校验——必须保留外部 hook 阻断。

struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

struct PolicyRule {
    id: String,
    condition: PolicyCondition,     // 什么情况下触发
    action: PolicyAction,           // Block / Warn / Allow
    reason: String,
}

enum PolicyCondition {
    /// 编辑受保护文件（locked by another node）
    EditLockedFile { file_pattern: String },
    /// 没跑 guard 就提交
    CommitWithoutGuard,
    /// review 前写 receipt
    WriteReceiptBeforeReview,
    /// 编辑 .flow/ 状态文件（只允许通过 flowctl）
    DirectStateEdit,
}

enum PolicyAction {
    Block { message: String },       // 硬阻断
    Warn { message: String },        // 告警但允许
    Allow,
}

/// 两个 adapter（不是 V3.1 的 3 个——CLI 退化为 thin facade，走 runtime 层）
///
/// 1. McpAdapter：MCP tool 内部校验（拦截走 MCP 的调用）
/// 2. HookAdapter：PreToolUse hook 阻断（拦截 Claude 直接调用 Edit/Write/Bash）
///
/// CLI 不需要独立 adapter——CLI 命令最终调用 runtime 层的同一套 PolicyEngine。

impl PolicyEngine {
    /// MCP tool 内部调用（node.start 前检查 lock，node.finish 前检查 guard）
    fn check_mcp(&self, action: &str, context: &PolicyContext) -> PolicyDecision;

    /// PreToolUse hook 调用（Edit/Write/Bash 前检查文件锁和保护规则）
    fn check_hook(&self, tool_name: &str, tool_input: &serde_json::Value) -> PolicyDecision;
}
```

---

## 5. MCP Tool Design（16 个 Tools）

### 5.1 设计原则

- **不超过 20 个 tools**：Anthropic 研究表明 tool 数量越少越好
- **每个 tool 有精确的 JSON Schema**：类型安全，agent 不需要猜参数格式
- **tool 描述是 agent 的主要"文档"**：描述精确，包含"什么时候用"和"什么时候不用"
- **Worker 保留 lock 和 knowledge 的主动调用权**：不合并为副作用

### 5.2 Tool 列表（按角色分组）

#### Goal Tools（Orchestrator 调用）

```rust
#[tool(description = "Open a goal from user request. Analyzes codebase, selects planning_mode \
    (direct/graph) and success_model (criteria/numeric/mixed). \
    intent: 'execute' (full pipeline), 'plan' (plan only), 'brainstorm' (explore only). \
    Returns goal_id, planning_mode, success_model, baseline score (if numeric).")]
async fn goal_open(&self,
    request: String,
    intent: Option<String>,       // execute | plan | brainstorm
    constraints: Option<Vec<String>>,
) -> Result<CallToolResult, Error>;

#[tool(description = "Get current goal status: progress, active/blocked/completed nodes, \
    current escalation level, score (if numeric), and suggested next action. Works in all modes.")]
async fn goal_status(&self, goal_id: String) -> Result<CallToolResult, Error>;

#[tool(description = "Mark goal as complete. Triggers: knowledge.record for all nodes, \
    knowledge.compound, and session summary. Call when score >= target (numeric) \
    or all acceptance criteria MET (criteria/mixed).")]
async fn goal_close(&self, goal_id: String) -> Result<CallToolResult, Error>;
```

#### Plan Tools（Orchestrator 调用）

```rust
#[tool(description = "Generate execution graph (PlanVersion) from goal. Each node gets RiskProfile \
    (scope, guard_depth, needs_deeper_qa). Searches knowledge for similar past work and injects \
    patterns into nodes. Returns nodes with deps, parallel levels, risk annotations. \
    Creates PlanVersion rev N+1 (never mutates existing versions).")]
async fn plan_build(&self, goal_id: String) -> Result<CallToolResult, Error>;

#[tool(description = "Return currently executable nodes (all deps satisfied). Each node includes: \
    objective, constraints, injected patterns, RiskProfile, owned files. \
    Returns empty list if all nodes are done or blocked.")]
async fn plan_next(&self, goal_id: String) -> Result<CallToolResult, Error>;

#[tool(description = "Apply graph mutations (add/remove/split/skip nodes, add/remove edges). \
    Creates a new PlanVersion with the mutations applied. \
    Used after graph_escalate suggests restructuring. Caller decides which mutations to apply.")]
async fn plan_mutate(&self,
    goal_id: String,
    mutations: Vec<GraphMutation>,
    rationale: String,
) -> Result<CallToolResult, Error>;
```

#### Node Tools（Worker 调用）

```rust
#[tool(description = "Start working on a node. Validates all deps satisfied. \
    Transitions node to InProgress. Does NOT auto-acquire locks — \
    call lock.acquire separately for the files you need.")]
async fn node_start(&self, node_id: String) -> Result<CallToolResult, Error>;

#[tool(description = "Mark node as done. Records attempt with summary, changed files, commits, tests. \
    Runs guard at risk-proportional depth (from node's RiskProfile). \
    Triggers score update (if numeric mode). Auto-records a learning (fallback). \
    Returns: guard result, score delta, newly unblocked nodes, escalation status.")]
async fn node_finish(&self,
    node_id: String,
    summary: String,
    changed_files: Vec<String>,
    commits: Option<Vec<String>>,
    tests: Option<Vec<String>>,
) -> Result<CallToolResult, Error>;

#[tool(description = "Report node failure with error details. Triggers three-level escalation: \
    L1 (retry with different approach), L2 (modify constraints/catalog), L3 (suggest graph restructure). \
    Returns escalation action — caller decides whether to follow suggestion.")]
async fn node_fail(&self,
    node_id: String,
    error: String,
) -> Result<CallToolResult, Error>;
```

#### Quality Tool（Worker 或 Orchestrator 调用）

```rust
#[tool(description = "Run quality guard at risk-proportional depth. \
    trivial: lint only. standard: lint + type + test. thorough: lint + type + test + review. \
    Depth auto-selected from node's RiskProfile, or override with depth param. \
    If no node_id, runs against entire working directory.")]
async fn quality_run(&self,
    node_id: Option<String>,
    depth: Option<String>,       // trivial | standard | thorough
) -> Result<CallToolResult, Error>;
```

#### Lock Tools（Worker 主动调用）

```rust
#[tool(description = "Acquire file locks for a node. Prevents parallel workers from editing same files. \
    Call BEFORE editing files, not just at node.start. Workers may discover additional files mid-work. \
    Returns lock status and any conflicts detected.")]
async fn lock_acquire(&self,
    node_id: String,
    files: Vec<String>,
) -> Result<CallToolResult, Error>;

#[tool(description = "Release file locks held by a node. Call after node.finish or on failure cleanup. \
    Idempotent — safe to call even if no locks held.")]
async fn lock_release(&self, node_id: String) -> Result<CallToolResult, Error>;
```

#### Knowledge Tools（Worker 或 Orchestrator 调用）

```rust
#[tool(description = "Search across all three knowledge layers: learnings (raw experience), \
    patterns (distilled knowledge), methodology rules. Returns ranked results with relevance scores. \
    Use BEFORE starting new work to leverage past experience.")]
async fn knowledge_search(&self,
    query: String,
    limit: Option<u32>,
) -> Result<CallToolResult, Error>;

#[tool(description = "Record a learning mid-execution or after completion. Workers should call this \
    when they discover something important (not just at the end). \
    kind: success | failure | discovery | pitfall. Automatically tagged and indexed.")]
async fn knowledge_record(&self,
    goal_id: String,
    node_id: Option<String>,
    content: String,
    kind: String,                // success | failure | discovery | pitfall
) -> Result<CallToolResult, Error>;

#[tool(description = "Run knowledge compounding after goal completion: cluster learnings by tags, \
    promote frequent learnings to patterns, validate/decay existing patterns. \
    This is how the system gets smarter over time.")]
async fn knowledge_compound(&self, goal_id: String) -> Result<CallToolResult, Error>;

#[tool(description = "Refresh stale patterns: decay confidence on unused patterns, \
    surface patterns needing validation, suggest consolidation. \
    Run periodically or when knowledge_search returns low-confidence results.")]
async fn knowledge_refresh(&self) -> Result<CallToolResult, Error>;
```

#### Codebase Tool（Orchestrator 调用）

```rust
#[tool(description = "Analyze codebase for a query: affected files, risk assessment, related symbols, \
    cross-module impact. Uses code graph + trigram index (zero external dependency). \
    Used during goal.open for complexity assessment and plan.build for node scoping.")]
async fn codebase_assess(&self, query: String) -> Result<CallToolResult, Error>;
```

### 5.3 Tool 调用者映射

| Tool | Orchestrator | Worker | 说明 |
|------|:-----------:|:------:|------|
| goal.open | x | | 创建/恢复 goal |
| goal.status | x | | 检查进度 |
| goal.close | x | | 完成 goal |
| plan.build | x | | 生成执行图 |
| plan.next | x | | 获取可执行节点 |
| plan.mutate | x | | 图变形 |
| node.start | | x | 开始执行 |
| node.finish | | x | 完成节点 |
| node.fail | | x | 报告失败 |
| quality.run | x | x | 质量检查 |
| lock.acquire | | x | 获取文件锁 |
| lock.release | | x | 释放文件锁 |
| knowledge.search | x | x | 检索经验 |
| knowledge.record | | x | 记录发现 |
| knowledge.compound | x | | 归纳知识 |
| knowledge.refresh | x | | 衰减刷新 |
| codebase.assess | x | | 代码分析 |

实际 16 个 tool（goal.open 合并了 V3 的 goal_create + codebase_assess 的评估功能）。`codebase.assess` 保留为独立 tool 供 plan.build 和外部使用。

---

## 6. Execution Flows

### 6.1 Direct + Numeric（极简 + 数值指标）

```
用户："fix all lint errors"

goal.open("fix all lint errors")
  → planning_mode: Direct, success_model: Numeric
  → fitness_script: "cargo clippy --message-format json 2>&1 | grep 'warning' | wc -l"
  → score_baseline: 23, score_target: 0

Claude 直接修复 lint 错误（无需生成图）

quality.run(depth: "standard")
  → lint: 0 errors

goal.close
  → knowledge.record("fixed 23 clippy warnings by ...")
  → done

总 tool calls: 3-4
```

### 6.2 Direct + Criteria（极简 + 验收）

```
用户："fix typo in README.md"

goal.open("fix typo in README.md")
  → planning_mode: Direct, success_model: Criteria
  → acceptance_criteria: ["Typo corrected"]

Claude 直接编辑文件

quality.run(depth: "trivial")
  → lint pass

goal.close
  → done

总 tool calls: 3
```

### 6.3 Graph + Numeric（拆任务 + 分数迭代）

```
用户："提升测试覆盖率到 80%"

goal.open("提升测试覆盖率到 80%")
  → planning_mode: Graph, success_model: Numeric
  → fitness_script: "npx jest --coverage --json | jq '.coveragePercentage'"
  → score_baseline: 62, score_target: 80
  → action_catalog: [
      {desc: "Add tests for auth module", impact: +5},
      {desc: "Add tests for user routes", impact: +3},
      {desc: "Add branch coverage for error paths", impact: +2},
    ]

plan.build → nodes: [research, auth-tests, route-tests, edge-tests, verify]

knowledge.search("test coverage improvement")
  → patterns: ["先测核心路径，再测边界情况"]

LOOP:
  plan.next → node-1 (auth-tests) ready

  Worker spawns:
    node.start(auth-tests)
    lock.acquire(auth-tests, ["src/auth/", "tests/auth/"])
    ... 编写测试 ...
    knowledge.record(discovery, "auth module needs DI for testability")
    lock.release(auth-tests)
    node.finish(auth-tests, summary, files)
      → guard(standard), score: 67 (+5)
      → newly_ready: [route-tests]

  plan.next → node-2 ready
  ... 继续循环 ...

  IF score 下降 → revert + node.fail → escalation L1
  IF 连续 3 次失败 → escalation L2 → modify action_catalog
  IF L2 也失败 → escalation L3 → plan.mutate (new PlanVersion)

UNTIL score >= 80

goal.close → knowledge.compound
  → 提取 pattern: "覆盖率提升：先核心路径 → 再路由 → 最后边界"
```

### 6.4 Graph + Criteria（拆任务 + 验收）

```
用户："add OAuth login with Google and GitHub"

goal.open("add OAuth login", intent: "execute")
  → planning_mode: Graph, success_model: Criteria
  → acceptance_criteria: [
      "Google OAuth 登录可用",
      "GitHub OAuth 登录可用",
      "Token 安全存储",
      "登出功能正常",
      "现有用户系统不受影响",
    ]

plan.build → nodes + levels:
  L0: [research]
  L1: [db-migration]
  L2: [google-oauth, github-oauth]   ← 并行！
  L3: [logout]
  L4: [integration-test]

knowledge.search("OAuth implementation")
  → patterns: ["token refresh 容易出问题——确保有 rotation"]

LOOP:
  plan.next → research ready → Worker 执行
  plan.next → db-migration ready → Worker 执行
  plan.next → [google-oauth, github-oauth] ready → 两个 Worker 并行

  lock.acquire(google-oauth, ["src/auth/google.ts"])
  lock.acquire(github-oauth, ["src/auth/github.ts"])
  ... 并行执行 ...
  lock.release(google-oauth)
  lock.release(github-oauth)

  IF Worker 发现需要额外工作:
    knowledge.record(discovery, "need token refresh cron job")
    node.fail → escalation → plan.mutate → 插入新 node

  所有 criteria 验证通过:
    quality.run(thorough)  ← 因为 touches_interfaces
    goal.close → knowledge.compound

DONE
```

### 6.5 Graph + Mixed（拆任务 + 验收门槛 + 分数加速）

```
用户："重构 auth 模块，降低耦合度"

goal.open("重构 auth 模块")
  → planning_mode: Graph, success_model: Mixed
  → acceptance_criteria: ["auth 模块不再直接依赖 user 模块", "所有现有测试通过"]
  → fitness_script: "cargo modules structure | grep 'auth -> ' | wc -l"
  → score_baseline: 7 (7 个直接依赖), score_target: 2

plan.build → nodes (按分数影响排序优先级)
... 执行循环（criteria 是门槛，score 辅助排序哪个 node 先做）...

goal.close 条件：all criteria MET AND score <= target
```

---

## 7. Worker Design（极简化）

### 7.1 当前 Worker（12 个固定 phase）——移除

```
Phase 1: Verify Config → Phase 2: Re-anchor → Phase 3: Investigation →
Phase 4: TDD Red-Green → Phase 5: Implement → Phase 6: Verify & Fix →
Phase 7: Commit → Phase 8: Review → Phase 9: Outputs Dump →
Phase 10: Complete → Phase 11: Memory → Phase 12: Return
```

### 7.2 V3 Final Worker（目标 + 约束 + 工具）

Worker 是一个 subagent。过度约束反而降低效果。Worker prompt 模板：

```markdown
## 你的任务
{node.objective}

## 约束
{node.constraints}

## 风险级别
{node.risk.estimated_scope} — {node.risk.risk_rationale}
Guard 深度：{node.risk.guard_depth}

## 历史经验（来自知识库）
{node.injected_patterns}

## 你拥有的文件
{node.owned_files}

## 可用 MCP 工具
- lock.acquire / lock.release — 管理文件锁（发现需要新文件时主动获取）
- quality.run — 验证代码质量（完成前必须运行）
- knowledge.record — 记录重要发现（不要等到最后）
- node.finish — 完成任务（传入 summary + changed_files）
- node.fail — 报告失败（传入 error 详情）

## 完成标准
1. 代码变更已提交
2. quality.run 通过
3. 调用 node.finish 汇报结果
```

**对比**：V1 Worker 需要理解 12 个 phase、receipt 格式、plan contract、concurrent worker protocol。V3 Worker 只需要理解 objective + 6 个 tool。

---

## 8. Skill 精简

### 8.1 当前：52 个 SKILL.md + 27 个 slash commands

### 8.2 V3 Final：3 个入口 + intent 参数

```
/flow-code:go "idea"              → goal.open(intent: execute)
/flow-code:go --plan-only "idea"  → goal.open(intent: plan)
/flow-code:go --brainstorm "idea" → goal.open(intent: brainstorm)
/flow-code:learn                  → knowledge.search / knowledge.compound
/flow-code:status                 → goal.status (all active goals)
```

**为什么不是 5 个前门**：`brainstorm` 和 `plan` 是 intent 参数而不是独立命令，因为在 Goal 模型下它们走同一个引擎——只是停的位置不同。减少命令面 = 减少 Claude 选择时的认知负担。

**`spec` 和 `adr` 的命运**：它们是 goal 的输出产物（spec = goal with intent brainstorm 的输出；adr = 约束文档），不需要独立 slash command。可以作为 `goal.open` 的 `output_format` 参数。

#### /flow-code:go SKILL.md（决策树，不是 pipeline 脚本）

```markdown
---
name: flow-code:go
description: Goal-driven adaptive execution
---

## 入口

调用 goal.open MCP tool，传入用户请求和 intent。

## 路由

根据返回的 planning_mode 和 success_model 字段：

### Direct 模式 (planning_mode = "direct")
1. 直接执行所需修改
2. 调用 quality.run(depth from response)
3. 调用 goal.close
4. 结束

### Graph 模式 (planning_mode = "graph")
1. 调用 plan.build 生成执行图
2. 调用 knowledge.search 检索相关经验
3. 循环：
   a. 调用 plan.next 获取可执行节点
   b. 为每个 ready 节点 spawn Worker agent（并行）
   c. Worker 调用 node.start → 工作 → node.finish
   d. 如果 success_model 包含 numeric：检查 goal.status 中的 score
   e. 如果连续失败，node.fail 返回 escalation action
   f. 如果 L3 escalation，调用 plan.mutate
4. 完成条件（取决于 success_model）：
   - Criteria: 所有 acceptance_criteria MET
   - Numeric: score >= target
   - Mixed: 所有 criteria MET AND score <= target
5. 调用 quality.run(depth: 取 all nodes 中最高的 guard_depth)
6. 调用 goal.close
7. goal.close 内部自动调用 knowledge.compound
```

### 8.3 52 个领域 skill 的迁移

当前的领域 skill 知识变成 patterns layer 的种子数据：

```bash
# 首次启动 V3 时，执行一次性迁移
flowctl serve --migrate-skills
# 读取 skills/*/SKILL.md → 提取领域知识
# → 写入 .flow/knowledge/patterns/ 作为初始 patterns
# → confidence: 0.8（人类编写，但需实践验证）
# → decay_days: 180（半年内需被实际使用验证）
```

---

## 9. Hook Design（精简）

### 9.1 当前：8 个 hook 点，多个处理器

### 9.2 V3 Final：2 个 hook + MCP server 内部处理大部分逻辑

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write|Bash",
        "hooks": [{
          "type": "command",
          "command": "flowctl policy check-hook --tool $TOOL_NAME --input $TOOL_INPUT",
          "timeout": 3
        }]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "flowctl session snapshot",
          "timeout": 5
        }]
      }
    ]
  }
}
```

**为什么只有 2 个**：

| 旧 Hook | V3 Final 处理方式 |
|---------|------------------|
| SessionStart | MCP server 自动初始化（.mcp.json 启动时）|
| UserPromptSubmit (keyword) | Skill 的 trigger patterns 已有此功能，不需要 hook |
| PreToolUse (ralph-guard) | **保留 → PolicyEngine hook adapter** |
| PostToolUse (verifier) | node.finish 内部处理 |
| PreCompact (state inject) | MCP server 持久内存，不需要重注入 |
| SubagentStart/Stop | Worker 通过 MCP tools 自管理 |
| Stop (snapshot) | **保留 → session snapshot** |
| SessionEnd | Stop hook 覆盖 |

**关键原则**：PreToolUse 是唯一需要物理阻断的 hook（PACEflow 模式）。其余逻辑全部内移到 MCP server 的 tool 实现中。

---

## 10. Storage Design（.flow/ 目录演化）

### 10.1 当前布局（散装）

```
.flow/
├── epics/<epic-id>.json           # Epic 定义
├── tasks/<task-id>.json           # Task 定义
├── specs/<epic-id>.md             # Spec markdown
├── .state/
│   ├── tasks/<id>.state.json      # Runtime 状态
│   ├── pipeline.json              # Epic pipeline 进度
│   ├── phases.json                # Worker phase 进度
│   ├── locks.json                 # 文件锁
│   ├── events.jsonl               # 事件日志
│   └── approvals.json             # 审批记录
├── memory/entries.jsonl
├── reviews/
├── config.json
└── graph.bin, index/ngram.bin
```

### 10.2 V3 Final 布局（Goal-scoped + 共享资源）

```
.flow/
├── goals/                          # 按 Goal 聚合（核心改进）
│   └── g-42-add-oauth/
│       ├── goal.json               # GoalContext
│       ├── plans/                   # PlanVersion 递增（不原地改）
│       │   ├── 0001.json           # 初次规划
│       │   └── 0002.json           # replan 后的新版本
│       ├── attempts/               # 按 node 分组的执行尝试
│       │   ├── n-1/0001.json       # node-1 第 1 次尝试
│       │   └── n-3/0001.json
│       ├── events.jsonl            # 本 goal 的事件日志
│       ├── iterations.jsonl        # fitness score 迭代记录（numeric 模式）
│       ├── reviews/                # review 结果
│       └── outputs/                # 附加产出（spec, adr 等）
│
├── knowledge/                      # 三层知识金字塔（跨 goal 共享）
│   ├── learnings/                  # Layer 1: 原子经验
│   │   └── 2026-04-11-auth-di.json
│   ├── patterns/                   # Layer 2: 归纳模式
│   │   └── oauth-best-practices.json
│   └── rules/                      # Layer 3: 方法论规则
│       └── methodology.json
│
├── runtime/                        # 运行时状态（跨 goal 共享）
│   ├── locks.json                  # 文件锁
│   └── sessions.json               # 活跃 worker sessions
│
├── indexes/                        # 代码智能索引（保留）
│   ├── graph.bin                   # 符号级引用图
│   └── ngram.bin                   # N-gram 全文索引
│
└── config.json                     # 配置
```

### 10.3 存储设计决策

| 决策 | 理由 |
|------|------|
| Goal-scoped 而非扁平 | 一个 goal 的所有数据聚合在一起，方便归档、删除、审计 |
| PlanVersion 递增 | replan 不修改旧版本——回滚 = 切版本，审计 = diff 两版本 |
| Attempts 按 node 分组 | 一个 node 可能重试多次，每次尝试独立记录 |
| Knowledge 跨 goal 共享 | 知识库是项目级资产，不属于某个 goal |
| Runtime 跨 goal 共享 | 锁和 session 是并发控制，跨 goal 互斥 |
| events.jsonl 每 goal 独立 | 避免全局事件日志成为热点文件 |
| 不引入 SQLite | 数据量不需要，保持零外部依赖 |
| 复用 bincode + trigram | knowledge.search 使用已有 NgramIndex |

---

## 11. Implementation Roadmap（7 步）

### Step 1: 宪法改写（原子落地，不写代码）

同时修改以下文件，正式宣布架构升级：

- [ ] `CLAUDE.md` — 添加 V3 架构说明，宣布 pipeline 冻结
- [ ] `.flow-config/project-context.md` — 移除 "no async runtime" 约束，添加 "async 仅限 flowctl-mcp crate"
- [ ] `docs/decisions/ADR-011-v3-mcp-native.md` — 记录架构决策
- [ ] `.flow/invariants.md` — 移除/修改与旧 pipeline 冲突的 invariant verify 命令
- [ ] `pipeline.rs`, `pipeline_phase.rs`, `flow-code-run/SKILL.md` — 添加 FROZEN 注释，不再接新功能

**验收**：所有文档约束一致，`flowctl invariants check` 通过，旧 pipeline 代码只修 bug。

### Step 2: 新类型 + 新 crate 骨架

- [ ] 创建 `flowctl/crates/flowctl-mcp/Cargo.toml`
- [ ] 在 `flowctl-core/src/domain/` 创建 Goal, Node, PlanVersion, Attempt, RiskProfile 类型
- [ ] 在 `flowctl-core/src/domain/` 创建 NodeStatus, GoalStatus, PlanningMode, SuccessModel enums
- [ ] 在 `flowctl-core/src/provider/` 创建 ReviewProvider, PlanningProvider, AskProvider traits
- [ ] 在 `flowctl-core/src/quality/` 创建 PolicyEngine, PolicyRule 类型
- [ ] 旧 types.rs 中的 Epic/Task/Phase 移到 `compat/` 模块（保留，不删除）
- [ ] `flowctl-mcp/src/server.rs` 骨架：rmcp server setup，空 tool handlers
- [ ] 在 `flowctl-cli/src/main.rs` 添加 `Serve` 子命令（启动 MCP server）

**验收**：`cargo build --all` 通过，新类型可编译，MCP server 可启动（空 tools）。

### Step 3: 存储层

- [ ] `flowctl-core/src/storage/goal_store.rs` — Goal CRUD（.flow/goals/{id}/goal.json）
- [ ] `flowctl-core/src/storage/plan_store.rs` — PlanVersion CRUD（递增版本号）
- [ ] `flowctl-core/src/storage/attempt_store.rs` — Attempt 写入（按 node/seq 路径）
- [ ] `flowctl-core/src/storage/knowledge_store.rs` — Learning/Pattern/Methodology CRUD
- [ ] `flowctl-core/src/storage/lock_store.rs` — 从旧 json_store 提取 lock 逻辑
- [ ] `flowctl-core/src/storage/event_store.rs` — 从旧 json_store 提取 event 逻辑，改为 per-goal
- [ ] 单元测试：每个 store 的 CRUD + 边界条件

**验收**：所有 store 单元测试通过，.flow/ 目录结构符合 §10.2 布局。

### Step 4: 引擎层

- [ ] `flowctl-core/src/engine/goal_engine.rs` — assess_goal(), open(), close() 逻辑
- [ ] `flowctl-core/src/engine/planner.rs` — 生成 PlanVersion，计算并行层级，标注 RiskProfile
- [ ] `flowctl-core/src/engine/scheduler.rs` — plan_next()，依赖解析，node 状态推进
- [ ] `flowctl-core/src/engine/escalation.rs` — 三层升级逻辑
- [ ] `flowctl-core/src/knowledge/learner.rs` — record, inject, compound, refresh
- [ ] `flowctl-core/src/quality/guard.rs` — 风险比例 guard depth
- [ ] `flowctl-core/src/quality/policy_engine.rs` — 规则检查 + MCP/Hook adapter
- [ ] `flowctl-core/src/provider/registry.rs` — ProviderRegistry + NoneProvider
- [ ] 集成测试：Direct + Criteria 端到端，Graph + Numeric 端到端

**验收**：两种 execution flow 的集成测试通过，escalation 测试通过。

### Step 5: MCP Server + Thin CLI

- [ ] `flowctl-mcp/` — 16 个 tool handler 接入 engine 层
- [ ] 更新 `.mcp.json` 指向 `flowctl serve`
- [ ] `flowctl-cli/` — 添加 goal/plan/node/knowledge CLI 子命令（调用 engine 层）
- [ ] 验证 Claude Code 能连接 MCP server 并调用 tools
- [ ] `output.rs` — 新 tool 的 JSON 输出加 api_version

**验收**：Claude Code 通过 MCP 可调用所有 16 个 tool，CLI 等价命令也能工作。

### Step 6: Skills + Hooks 改写

- [ ] `skills/flow-code-go/SKILL.md` — 从 pipeline engine 改为决策树路由（§8.2）
- [ ] `skills/flow-code-learn/SKILL.md` — 知识管理入口
- [ ] `skills/flow-code-status/SKILL.md` — 状态查看入口
- [ ] `hooks/hooks.json` — 精简为 2 个 hook（PreToolUse + Stop）
- [ ] Worker prompt 模板（§7.2）
- [ ] 一次性迁移脚本：`flowctl serve --migrate-skills`

**验收**：`/flow-code:go "fix typo"` 端到端通过（Direct 模式），`/flow-code:go "add feature"` 端到端通过（Graph 模式）。

### Step 7: Cutover + Legacy 删除

- [ ] `.mcp.json` 正式切换到 `flowctl serve`
- [ ] `flowctl-core/src/compat/` 保留旧 Epic/Task 只读类型（用于迁移旧 .flow/ 数据）
- [ ] 删除 `pipeline.rs`, `pipeline_phase.rs`
- [ ] 删除 `skills/flow-code-run/SKILL.md`（pipeline engine skill）
- [ ] 删除 52 个旧 domain skill（已迁移为 patterns seed）
- [ ] 删除 24 个旧 agent.md（保留 Worker 模板 + explore scout）
- [ ] 删除多余 hook handlers（ralph_guard, commit_gate, pre_compact 等）
- [ ] 更新 CLAUDE.md 移除旧 pipeline 文档
- [ ] 迁移所有旧测试到新核心
- [ ] 版本号升级到 v0.2.0
- [ ] 全量测试：smoke_test, ci_test, 端到端

**验收**：`cargo test --all` 全部通过，零旧 pipeline 代码残留，v0.2.0 标签。

---

## 12. Technical Decisions

### 12.1 MCP SDK 选型

```toml
# flowctl/crates/flowctl-mcp/Cargo.toml
[dependencies]
rmcp = { version = "0.16", features = ["server", "macros", "schemars"] }
tokio = { version = "1", features = ["full"] }
flowctl-core = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- **rmcp**：官方 Rust MCP SDK
- **tokio**：async runtime（**仅限 flowctl-mcp crate**，domain/storage 保持同步）
- **架构约束**：flowctl-core 不依赖 tokio，所有 engine/storage 方法是同步的。MCP crate 用 `tokio::task::spawn_blocking` 调用同步引擎

### 12.2 传输方式

```json
// .mcp.json
{
  "mcpServers": {
    "flowctl": {
      "command": "${CLAUDE_PLUGIN_ROOT}/bin/flowctl",
      "args": ["serve"]
    }
  }
}
```

- **stdio 传输**：Claude Code 原生支持
- CLI 子命令照常工作（不经过 MCP）
- `flowctl serve` 启动 MCP server 模式

### 12.3 状态持久化

- **内存缓存 + 文件持久化**：Server 运行时状态在内存，每次变更写入 .flow/
- **不引入 SQLite**：保持零外部依赖（数据量不需要）
- **trigram index 复用**：knowledge.search 使用已有 NgramIndex
- **bincode 复用**：graph 状态可以用 bincode 序列化
- **atomic writes**：保留 write-to-temp + rename 模式
- **fs2 file locking**：保留 advisory locks for concurrent access

### 12.4 CLI 兼容性

flowctl CLI 保留所有现有命令 + 新增 goal/plan/node/knowledge 命令：

```bash
# 新 MCP tools 的 CLI 等价物
flowctl goal open "add OAuth" --json       # = goal.open
flowctl goal status g-42 --json            # = goal.status
flowctl plan next g-42 --json              # = plan.next
flowctl knowledge search "OAuth" --json    # = knowledge.search

# 管理命令（CLI only，不暴露为 MCP tool）
flowctl knowledge list --patterns --json
flowctl knowledge import --file patterns.md
flowctl config set guard.default_depth standard
flowctl serve --migrate-skills             # 一次性 skill → pattern 迁移

# 旧命令（compat shim，调用新引擎层）
flowctl epic list                          # → 读取 goals/ 并映射为 epic 格式
flowctl phase next --epic fn-1             # → 调用 goal engine
flowctl show fn-1                          # → 调用 goal.status
```

---

## 13. Migration Strategy

### 13.1 旧 .flow/ 数据迁移

```bash
flowctl migrate v3
# 1. 读取 .flow/epics/*.json → 转换为 .flow/goals/
# 2. 读取 .flow/tasks/*.json → 转换为 PlanVersion nodes
# 3. 读取 .flow/.state/ → 转换为 goal events + attempts
# 4. 读取 .flow/memory/ → 转换为 knowledge/learnings/
# 5. 保留 .flow/indexes/ 和 .flow/config.json
# 6. 原始文件移到 .flow/.archive/v1/
```

### 13.2 Keep / Remove / Add 清单

#### Keep（保留）

| 能力 | 理由 |
|------|------|
| flowctl Rust 二进制 | 演化为 MCP server |
| .flow/ JSON 存储 | 零依赖持久化（重新组织为 goal-scoped） |
| graph.bin + ngram index | 代码智能基础设施 |
| file locking | Worker 并行安全 |
| api_version + --input-json | CLI 接口刚对齐（v0.1.53） |
| guard 命令 | 质量门禁（增加风险比例深度） |
| review_protocol.rs | 多模型 consensus 逻辑（接入 ProviderRegistry） |
| dag.rs + petgraph | DAG 基础设施（plan.build 复用） |
| state_machine.rs | NodeStatus 转换逻辑（简化后保留） |

#### Remove（移除）

| 能力 | 理由 |
|------|------|
| 6-phase pipeline（pipeline.rs, pipeline_phase.rs） | GoalContext + PlanVersion 取代 |
| 12-phase worker protocol | Worker 只收 objective + constraints |
| 52 个 SKILL.md | 精简为 3 + patterns seed |
| 24 个 agent.md | 精简为 1 Worker 模板 + explore scout |
| ralph-guard hook | PolicyEngine + PreToolUse hook |
| commit-gate hook | PolicyEngine |
| pre-compact hook | MCP server 内存常驻 |
| SubagentStart/Stop hooks | Worker 通过 MCP tools 自管理 |
| flow-code-run SKILL.md（pipeline engine） | flow-code-go SKILL.md（决策树路由） |

#### Add（新增）

| 能力 | 理由 |
|------|------|
| MCP Server（`flowctl serve`） | 持久进程 + 结构化工具 |
| Goal + PlanningMode × SuccessModel | 正交目标驱动 |
| PlanVersion 递增 | 安全 replan + 审计 |
| Fitness Function | 可执行的分数脚本 |
| 三层升级控制 | SWE-AF 模式 |
| 三层知识金字塔 | Compound + Hermes + arscontexta |
| RiskProfile（IssueGuidance） | 风险比例资源分配 |
| PolicyEngine + 2 adapters | 统一治理 + 物理阻断 |
| ProviderRegistry | trait 抽象的 review/planning providers |

---

## 14. Success Metrics

| 指标 | 当前（V1） | V3 Final 目标 |
|------|-----------|-------------|
| 简单任务 tool calls | 20+ 步（pipeline） | 3-4 步（Direct 模式） |
| 复杂任务失败恢复 | review circuit breaker（max iterations） | 三层升级 + replan（GraphMutation） |
| Worker prompt 大小 | ~2000 tokens（12 phase + receipt） | ~300 tokens（objective + constraints） |
| 知识积累 | 手动 memory entries | 自动 learning → pattern → methodology |
| Hook 数量 | 8 hook 点 × 多处理器 | 2 hook 点 × 1 处理器 |
| Skill 数量 | 52 个 SKILL.md | 3 个入口 + patterns seed |
| 外部依赖 | 零 | 零（tokio 是 Rust 标准，rmcp 是 MCP 必需） |
| 重复做类似任务 | 每次从零开始 | patterns 注入，第 N 次比第 1 次快 |
| 分数可验证 | 无 | iterations.jsonl 记录每步 score delta |

---

## 15. Risk Assessment

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| rmcp API 不稳定 | MCP crate 需要跟进 | pin 版本，封装 adapter 层 |
| Claude 绕过 MCP 直接用 Bash | PolicyEngine 失效 | PreToolUse hook 物理阻断 |
| 旧 .flow/ 数据迁移丢失 | 用户数据损坏 | 原始文件保留在 .archive/v1/ |
| 知识库 pattern 错误 | 注入错误经验 | confidence 衰减 + use_count 验证 |
| async 泄漏到 core | 架构边界破坏 | cargo deny 配置禁止 core 依赖 tokio |
| Goal-scoped 目录过多 | 文件系统性能 | goal.close 后自动归档到 .archive/ |

---

## Appendix A: V3 Draft vs V3.1 Review vs V3 Final 对照

| 维度 | V3 Draft | V3.1 Review | V3 Final |
|------|----------|-------------|----------|
| 核心模型 | ExecutionMode 单 enum | PlanningMode × SuccessModel | **PlanningMode × SuccessModel** |
| Plan 存储 | 原地修改 graph | PlanVersion 递增 | **PlanVersion 递增** |
| Goal 存储 | 扁平 .flow/goals/ | 按 goal 聚合 | **按 goal 聚合** |
| Hook 策略 | 3→0 外部 hook | PolicyEngine + 3 adapter | **PolicyEngine + 2 adapter** |
| Provider | 写进内核 | trait Registry | **trait Registry** |
| Crate 数 | 3 | 5 | **3**（延迟拆分约定） |
| MCP Tool 数 | 16 | 12（合并 lock/knowledge） | **16**（Worker 保留主动权） |
| 前门数 | 3 | 5 | **3** + intent 参数 |
| Worker prompt | objective + constraints | 同 V3 | **同 V3** |
| 实施起点 | 代码（Phase 0） | 宪法改写（Step 1） | **宪法改写** |
| 实施步数 | 5 Phase | 7 Step | **7 Step** |

## Appendix B: Reference Projects

| 项目 | 贡献的核心思想 |
|------|--------------|
| goal-md | Fitness function + action catalog + dual-score |
| SWE-AF | 8 architectural patterns（升级控制、风险比例、知识传播等） |
| PACEflow | Hook 物理拦截（100% vs 70%） |
| Compound Engineering | 知识复利 + 衰减生命周期 |
| arscontexta | 自我改进的方法论 + 认知架构衍生 |
| Hermes Agent | 闭合学习环 + FTS5 检索 + 技能提取 |
| OMC (oh-my-claudecode) | 关键词触发 + model routing + MCP tools |
| agent-council | 多模型并行意见 + Chairman 综合 |
| multi-agent-shogun | 零 API 文件协调 + Bloom 级别路由 |
| Spec-Flow | Domain Memory + 原子 Worker |
| BMAD-METHOD | Party Mode 多角色圆桌 |
| Anthropic Research | Orchestrator-Worker + 编排指导 |
| Anthropic Building Agents | 6 composable patterns + simplicity principle |
| rmcp | Official Rust MCP SDK |
