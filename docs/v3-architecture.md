# flow-code V3 Architecture: Goal-Driven Adaptive Engine over MCP

> **Status**: Design Document — Ready for Implementation
> **Date**: 2026-04-10
> **Author**: z23cc + Claude Opus 4.6
> **Supersedes**: V1 (pipeline state machine), V2 (pure fitness-function)

---

## 1. Executive Summary

flow-code V3 重构的核心是三个转变：

1. **从 Pipeline 到 Goal**：不再是"走完 6 个阶段"，而是"让目标达成"
2. **从 CLI 到 MCP**：flowctl 从 Bash 调用的 CLI 工具变成持久运行的 MCP Server
3. **从静态 Skill 到知识复利**：52 个手写 SKILL.md 精简为 1 个决策树入口 + 三层自动积累的知识金字塔

核心公式：**Goal + Score + Loop + Learn = 自适应引擎**

---

## 2. Design Principles

### 来源与致谢

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

### Anthropic 指导原则

> "Start with the simplest solution that works. Add complexity only when simpler solutions demonstrably fail."
> — Anthropic, Building Effective Agents

> "Each subagent needs an objective, an output format, guidance on the tools and sources to use, and clear task boundaries."
> — Anthropic, Multi-Agent Research System

---

## 3. Architecture Overview

### 3.1 Claude Code 运行模型的真相

```
Claude（LLM）= CPU
SKILL.md     = 程序（指令）
flowctl MCP  = 数据库 + 运行时引擎
.flow/       = 持久存储
```

Claude Code 插件中没有独立的"运行时"——Claude 自己就是运行时。但 MCP Server 改变了这个等式：

```
之前（CLI）：Claude → Bash("flowctl status --json") → fork → 读文件 → stdout → 进程死亡
之后（MCP）：Claude → MCP tool(goal_status) → 持久进程内存操作 → 结构化返回
```

### 3.2 MCP Server 解锁的能力

| 纯 Plugin 约束 | MCP Server 解除 |
|----------------|----------------|
| 无后台进程 | MCP server 是持续运行的进程 |
| 无事件系统 | MCP notifications 机制 |
| 无实时通信 | Server 可以主动推送状态变更 |
| 工具调用是字符串拼接 | 结构化 schema + 类型校验 |
| 无并发状态管理 | Server 内部管理并发状态 |
| 每次 Bash 调用有 fork 开销 | 持久连接，零 fork |

### 3.3 三层架构

```
┌──────────────────────────────────────────────────────┐
│                   MCP Tool Layer                      │
│         16 个精心设计的 tools（Claude 的接口）          │
├──────────────────────────────────────────────────────┤
│                   Core Engine Layer                    │
│  GoalContext │ AdaptiveGraph │ Learner │ Guard         │
├──────────────────────────────────────────────────────┤
│                   Storage Layer                       │
│  .flow/goals/ │ .flow/graph/ │ .flow/learned/ │ ...  │
└──────────────────────────────────────────────────────┘
```

---

## 4. Core Engine Design

### 4.1 GoalContext（取代 Epic + 6 Phases）

```rust
struct GoalContext {
    id: String,                   // g-1-add-oauth
    request: String,              // 用户原始请求
    mode: ExecutionMode,          // Direct / ScoreDriven / GoalDriven
    complexity: Complexity,       // Trivial / Standard / Complex
    
    // Score-driven 模式
    fitness_script: Option<String>,  // 可执行的评分脚本路径
    score_baseline: Option<f64>,     // 初始分数
    score_target: Option<f64>,       // 目标分数
    score_current: Option<f64>,      // 当前分数
    action_catalog: Vec<Action>,     // 可以涨分的操作列表
    
    // Goal-driven 模式
    acceptance_criteria: Vec<Criterion>,  // 验收标准
    
    // 通用
    constraints: Vec<String>,        // 约束（ADR、invariants、non-goals）
    known_facts: Vec<Fact>,          // 执行中持续更新的已知信息
    open_questions: Vec<String>,     // 未解答的问题
    iteration: u32,                  // 当前迭代次数
    progress: f32,                   // 0.0 → 1.0
    created_at: DateTime,
    updated_at: DateTime,
}

enum ExecutionMode {
    /// 极简任务：无规划，直接执行
    Direct,
    /// 有数值指标的任务：fitness function 驱动优化循环
    ScoreDriven,
    /// 创造性/无数值指标的任务：验收标准驱动
    GoalDriven,
}

struct Action {
    description: String,     // "Add tests for uncovered route handlers"
    estimated_impact: f64,   // "+2 points"
    risk: Risk,              // Low / Medium / High
    tried: bool,             // 是否已尝试
    outcome: Option<String>, // 尝试结果
}
```

### 4.2 自动模式选择

```rust
fn assess_mode(request: &str, codebase: &CodebaseIndex) -> ExecutionMode {
    let file_estimate = codebase.estimate_affected_files(request);
    let has_natural_metric = detect_natural_metric(request, codebase);
    // has_natural_metric: 检测请求是否涉及可量化的目标
    // 例如："提升测试覆盖率" → true（覆盖率是数值）
    // 例如："添加 OAuth 登录" → false（新功能没有自然指标）
    // 例如："修复 lint 错误" → true（lint 错误数是数值）
    
    match (file_estimate, has_natural_metric) {
        (0..=2, _) if is_trivial(request)  => Direct,
        (_, true)                           => ScoreDriven,
        _                                   => GoalDriven,
    }
}

fn detect_natural_metric(request: &str, codebase: &CodebaseIndex) -> bool {
    // 检查是否存在可运行的评分命令
    // 1. 项目有 test runner → 测试通过率/覆盖率
    // 2. 项目有 linter → lint 错误数
    // 3. 项目有 benchmark → 性能指标
    // 4. 请求包含数值目标（"达到 80% 覆盖率"）
    // 5. 请求是修复类（"fix all type errors" → tsc 错误数）
    let stack = codebase.detect_stack();
    stack.has_test_runner() 
        || stack.has_linter() 
        || request.contains_numeric_target()
        || request.is_fix_type()
}
```

### 4.3 AdaptiveGraph（动态执行图）

```rust
struct AdaptiveGraph {
    nodes: HashMap<NodeId, TaskNode>,
    edges: Vec<(NodeId, NodeId)>,  // 依赖边
    levels: Vec<Vec<NodeId>>,      // 拓扑排序后的并行层级（Kahn's algorithm）
    
    // 运行时状态（内存常驻）
    completed: HashMap<NodeId, CompletionEvidence>,
    failed: HashMap<NodeId, FailureInfo>,
    escalation_level: EscalationLevel,
}

struct TaskNode {
    id: NodeId,
    title: String,
    objective: String,         // 目标描述（不是步骤）
    constraints: Vec<String>,  // 约束
    
    // SWE-AF IssueGuidance（风险比例标注）
    guidance: IssueGuidance,
    
    // 注入的历史经验
    injected_patterns: Vec<Pattern>,
    
    status: NodeStatus,        // Ready / InProgress / Done / Failed / Skipped
    files: Vec<String>,        // 拥有的文件（用于 locking）
}

/// 来自 SWE-AF 的 IssueGuidance 模式
struct IssueGuidance {
    estimated_scope: Scope,       // Trivial / Small / Medium / Large
    needs_deeper_qa: bool,        // 是否需要深度 QA
    touches_interfaces: bool,     // 是否跨模块
    risk_rationale: String,       // 为什么这样判断
    guard_depth: GuardDepth,      // Trivial / Standard / Thorough
}

enum GuardDepth {
    Trivial,    // 只跑 guard（lint/type/test）
    Standard,   // guard + 自检
    Thorough,   // guard + 多轮 review + adversarial
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
    
    /// L3: 重规划（修改图结构）
    /// 触发条件：L2 也失败
    /// 动作：拆分 node、插入新 node、修改依赖、甚至修改 goal
    Replan,
}

impl AdaptiveGraph {
    fn handle_failure(&mut self, node_id: NodeId, error: &str) -> EscalationAction {
        let fail_count = self.consecutive_failures(node_id);
        
        match fail_count {
            0..=2 => EscalationAction::RetryWithDifferentApproach {
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
            },
        }
    }
    
    /// 运行时图变形（来自 SWE-AF: Runtime Plan Mutation）
    fn replan(&mut self, trigger: ReplanTrigger) -> Vec<GraphMutation> {
        match trigger {
            ReplanTrigger::NodeFailed { id, error } => {
                self.suggest_recovery(id, &error)
            }
            ReplanTrigger::NewDiscovery { finding } => {
                self.suggest_expansion(&finding)
            }
            ReplanTrigger::ScopeChange { .. } => {
                self.suggest_restructure()
            }
            ReplanTrigger::ScoreRegression { delta } => {
                self.suggest_revert_and_reroute(delta)
            }
        }
    }
}

/// replan 返回建议（Mutations），不是直接修改。
/// Claude 看到建议后决定是否执行——保持人/AI 在环的控制力。
enum GraphMutation {
    AddNode { node: TaskNode, deps: Vec<NodeId> },
    RemoveNode { id: NodeId },
    SplitNode { id: NodeId, into: Vec<TaskNode>, chain: bool },
    SkipNode { id: NodeId, reason: String },
    AddEdge { from: NodeId, to: NodeId },
    RemoveEdge { from: NodeId, to: NodeId },
    UpdateConstraints { id: NodeId, new_constraints: Vec<String> },
}
```

### 4.5 三层知识金字塔（来自 Compound + Hermes + arscontexta）

```rust
/// Layer 1: Learnings（经验 = 每次执行产出）
/// 最底层，积累最快，原子化，有时间戳
struct Learning {
    id: String,
    goal_id: String,           // 来源 goal
    node_id: Option<String>,   // 来源 node
    kind: LearningKind,        // Success / Failure / Discovery / Pitfall
    content: String,           // 具体经验
    tags: Vec<String>,         // 分类标签
    created_at: DateTime,
    verified: bool,            // 是否被后续使用验证过
    use_count: u32,            // 被检索使用的次数
}

/// Layer 2: Patterns（模式 = 从多次 learnings 中归纳）
/// 中间层，定期归纳，有衰减生命周期
struct Pattern {
    id: String,
    name: String,              // "OAuth token refresh pattern"
    description: String,       // 模式描述
    approach: String,          // 推荐做法
    anti_patterns: Vec<String>,// 要避免的做法
    source_learnings: Vec<String>, // 归纳自哪些 learnings
    confidence: f64,           // 0.0-1.0，随使用验证上升
    freshness: DateTime,       // 最后刷新时间
    decay_days: u32,           // 衰减周期（默认 90 天）
    use_count: u32,
}

/// Layer 3: Methodology（方法论 = 核心流程规则）
/// 最顶层，偶尔修订，arscontexta 模式：可被 agent 修改
/// 对应当前的 SKILL.md 中的核心流程指令
struct Methodology {
    rules: Vec<MethodRule>,
    last_revised: DateTime,
    revision_trigger: String,  // 什么条件下触发修订
}

/// 知识管理引擎
struct Learner {
    learnings: Vec<Learning>,
    patterns: Vec<Pattern>,
    methodology: Methodology,
    
    // 检索索引（复用已有 trigram 基础设施）
    index: NgramIndex,
}

impl Learner {
    /// Goal 完成后自动调用
    fn record(&mut self, goal_id: &str, node_id: &str, outcome: &CompletionEvidence) {
        // 记录 learning
        let learning = Learning {
            goal_id: goal_id.to_string(),
            node_id: Some(node_id.to_string()),
            kind: if outcome.success { LearningKind::Success } else { LearningKind::Failure },
            content: outcome.summary.clone(),
            tags: outcome.tags.clone(),
            ..Default::default()
        };
        self.learnings.push(learning);
    }
    
    /// 为新 node 注入相关经验
    fn inject_for_node(&self, node: &TaskNode) -> Vec<&Pattern> {
        // 检索相关 patterns（比 raw learnings 更有价值）
        self.index.search(&node.objective, 3)
    }
    
    /// 定期归纳：多个 learnings → 一个 pattern（Compound 模式）
    fn compound(&mut self, goal_id: &str) {
        // 1. 找到本次 goal 的所有 learnings
        // 2. 按 tags 聚类
        // 3. 如果某个 tag 下有 3+ learnings → 归纳为 pattern
        // 4. 如果已有 pattern 被本次 learnings 验证 → 提升 confidence
        // 5. 如果已有 pattern 被本次 learnings 否定 → 降低 confidence
    }
    
    /// 衰减刷新（Compound Engineering 模式）
    fn refresh_stale_patterns(&mut self) {
        let now = Utc::now();
        for pattern in &mut self.patterns {
            let age = now - pattern.freshness;
            if age.num_days() > pattern.decay_days as i64 {
                pattern.confidence *= 0.8; // 衰减
                if pattern.confidence < 0.3 {
                    // 标记为需要人工审查或自动淘汰
                }
            }
        }
    }
    
    /// 检索（统一检索三层）
    fn search(&self, query: &str, limit: usize) -> SearchResult {
        SearchResult {
            patterns: self.search_patterns(query, limit),
            learnings: self.search_learnings(query, limit),
            methodology_rules: self.match_rules(query),
        }
    }
}
```

---

## 5. MCP Tool Design（16 个 Tools）

### 设计原则

- **不超过 20 个 tools**：Anthropic 的研究表明 tool 数量越少越好
- **每个 tool 有精确的 JSON Schema**：类型安全，agent 不需要猜参数格式
- **tool 描述是 agent 的主要"文档"**：描述要精确，包含"什么时候用"和"什么时候不用"

### 5.1 Goal Tools（4 个）

```rust
#[tool(description = "Create a goal from user request. Analyzes codebase, attempts to construct \
    a fitness function, selects execution mode (direct/score_driven/goal_driven). \
    Returns goal_id, mode, baseline score (if score_driven), and initial action catalog.")]
async fn goal_create(&self, request: String, constraints: Option<Vec<String>>) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Run the fitness function and return current score with per-dimension breakdown. \
    Only available in score_driven mode. Returns score, delta from last run, and suggested next action.")]
async fn goal_score(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Get current goal status: progress percentage, active/blocked/completed nodes, \
    current escalation level, and suggested next action. Works in all modes.")]
async fn goal_status(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Mark goal as complete. Triggers: learning extraction from all nodes, \
    pattern compounding, session summary generation. Call only when score >= target (score_driven) \
    or all acceptance criteria MET (goal_driven).")]
async fn goal_complete(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;
```

### 5.2 Graph Tools（4 个）

```rust
#[tool(description = "Generate execution graph from goal. Each node gets IssueGuidance annotation \
    (scope, risk, qa_depth). Searches learned patterns for similar past work and injects into nodes. \
    Returns nodes with dependencies, parallel levels, and risk annotations.")]
async fn graph_plan(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Return currently executable nodes (all dependencies satisfied). \
    Each node includes: objective, constraints, injected patterns from knowledge base, \
    IssueGuidance (scope/risk/qa_depth), and owned files for locking.")]
async fn graph_next(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Mark node as done. Records completion evidence, runs guard at risk-proportional depth, \
    triggers score update (if score_driven), records learning. Returns: newly unblocked nodes, \
    guard result, score delta, and escalation status. If score decreased, suggests revert.")]
async fn graph_done(&self, node_id: String, summary: String, evidence: Option<String>) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Handle node failure with three-level escalation. L1: suggest alternative approach. \
    L2: modify action catalog and constraints. L3: suggest graph restructuring (add/split/skip nodes). \
    Returns suggested mutations — caller decides whether to apply them.")]
async fn graph_escalate(&self, goal_id: String, node_id: String, error: String) 
    -> Result<CallToolResult, Error>;
```

### 5.3 Knowledge Tools（4 个）

```rust
#[tool(description = "Record a learning from completed or failed work. Captures: what happened, \
    what worked/didn't, what was discovered. Automatically tagged and indexed for future retrieval.")]
async fn learn_record(&self, goal_id: String, node_id: String, outcome: String, kind: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Search across all three knowledge layers: learnings (raw experience), \
    patterns (distilled knowledge), and methodology rules. Returns ranked results with relevance scores. \
    Use before starting new work to leverage past experience.")]
async fn learn_search(&self, query: String, limit: Option<u32>) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Run knowledge compounding after goal completion: extract learnings from all nodes, \
    cluster by tags, promote frequent learnings to patterns, validate/decay existing patterns, \
    update confidence scores. This is how the system gets smarter over time.")]
async fn learn_compound(&self, goal_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Refresh stale patterns: decay confidence on patterns not used recently, \
    surface patterns needing validation, suggest consolidation of similar patterns. \
    Run periodically (e.g., weekly) or when learn_search returns low-confidence results.")]
async fn learn_refresh(&self) 
    -> Result<CallToolResult, Error>;
```

### 5.4 Infrastructure Tools（4 个）

```rust
#[tool(description = "Run quality guard at risk-proportional depth. \
    trivial: lint only. standard: lint + type + test. thorough: lint + type + test + review. \
    Depth auto-selected from node's IssueGuidance, or override with depth parameter.")]
async fn guard_run(&self, scope: Option<String>, depth: Option<String>) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Acquire file locks for a node. Prevents parallel workers from editing same files. \
    Returns lock status and any conflicts detected.")]
async fn lock_acquire(&self, node_id: String, files: Vec<String>) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Release file locks held by a node. Call after graph_done or on failure cleanup.")]
async fn lock_release(&self, node_id: String) 
    -> Result<CallToolResult, Error>;

#[tool(description = "Analyze codebase for a query: affected files, risk assessment, related symbols, \
    cross-module impact. Uses code graph + trigram index (zero external dependency). \
    Use during goal_create for complexity assessment or during graph_plan for node scoping.")]
async fn codebase_assess(&self, query: String) 
    -> Result<CallToolResult, Error>;
```

---

## 6. Execution Flows

### 6.1 Direct Mode（极简任务）

```
用户："fix typo in README.md"

goal_create("fix typo in README.md")
  → mode: Direct, complexity: Trivial
  
Claude 直接编辑文件

guard_run(depth: "trivial")
  → lint pass

goal_complete
  → learn_record (if worth recording)
  → done

总 tool calls: 3-4
```

### 6.2 ScoreDriven Mode（有数值指标的任务）

```
用户："提升测试覆盖率到 80%"

goal_create("提升测试覆盖率到 80%")
  → mode: ScoreDriven
  → fitness_script: "npx jest --coverage --json | jq '.coveragePercentage'"
  → score_baseline: 62
  → score_target: 80
  → action_catalog: [
      {desc: "Add tests for auth module", impact: +5},
      {desc: "Add tests for user routes", impact: +3},
      {desc: "Add branch coverage for error paths", impact: +2},
    ]

graph_plan
  → nodes: [research, implement-auth-tests, implement-route-tests, verify]
  → risk annotation: all Standard depth

learn_search("test coverage improvement")
  → patterns: ["先测核心路径，再测边界情况", ...]

LOOP:
  graph_next → node-1 ready
  
  Claude spawn Worker:
    "目标：为 auth 模块添加测试。
     约束：不改业务代码，只加测试。
     经验注入：<patterns from learn_search>"
  
  Worker 完成 → graph_done(node-1, summary, evidence)
    → 自动运行 guard(standard)
    → 自动运行 goal_score → 67 分（+5）→ commit
    → learn_record(success, "auth module tests added, +5 coverage")
    → newly_ready: [node-2]
  
  graph_next → node-2 ready
  ... 继续循环 ...
  
  IF goal_score → 分数下降:
    → revert last change
    → graph_escalate(L1: try different approach)
  
  IF 连续 3 次失败:
    → graph_escalate(L2: modify action catalog)
    → 例如：发现 auth 模块 mock 导致测试不稳定
    → 修改 catalog: 去掉 mock 相关操作，加入 integration test 操作
  
  IF L2 也失败:
    → graph_escalate(L3: replan)
    → 例如：发现需要先重构 auth 模块才能测试
    → graph 插入新 node: "refactor auth for testability"

UNTIL goal_score >= 80

goal_complete → learn_compound
  → 提取 learnings: ["auth 模块需要先解耦才能测试", "route tests 最快涨分"]
  → 归纳 pattern: "覆盖率提升模式: 先核心 → 再路由 → 最后边界"
```

### 6.3 GoalDriven Mode（创造性/无数值指标的任务）

```
用户："add OAuth login with Google and GitHub providers"

goal_create("add OAuth login")
  → mode: GoalDriven
  → acceptance_criteria: [
      "Google OAuth 登录可用",
      "GitHub OAuth 登录可用",
      "Token 存储安全",
      "登出功能正常",
      "现有用户系统不受影响",
    ]

graph_plan
  → nodes: [
      research(scope:Small, qa:false),
      db-migration(scope:Medium, qa:true, touches_interfaces:true),
      google-oauth(scope:Medium, qa:true),
      github-oauth(scope:Medium, qa:true),
      logout(scope:Small, qa:false),
      integration-test(scope:Medium, qa:true),
    ]
  → levels: [[research], [db-migration], [google-oauth, github-oauth], [logout], [integration-test]]

learn_search("OAuth implementation")
  → patterns: ["上次 OAuth 实现时 token refresh 出过问题", ...]

LOOP:
  graph_next → research ready
  
  Claude spawn Worker:
    "目标：研究 OAuth 集成方案。
     输出格式：推荐的库、数据模型、安全注意事项。
     经验注入：'上次 token refresh 出过问题——确保 refresh token 有 rotation'"
  
  Worker 完成 → graph_done(research)
    → board 写入发现
    → guard(trivial)
    → learn_record
    → newly_ready: [db-migration]
  
  ... Worker 执行 db-migration ...
  
  graph_done(db-migration) → newly_ready: [google-oauth, github-oauth]
  
  Claude spawn Worker(google-oauth) + Worker(github-oauth)  ← 并行！
  lock_acquire(google-oauth, ["src/auth/google.ts"])
  lock_acquire(github-oauth, ["src/auth/github.ts"])
  
  两个 Worker 并行完成
  lock_release(google-oauth)
  lock_release(github-oauth)
  
  ... 继续 logout, integration-test ...
  
  IF Worker 发现需要额外工作（比如 token refresh 需要 cron job）:
    → goal_update(discovery: "need token refresh cron")
    → graph_escalate → replan → 插入新 node
  
  所有 criteria 验证通过:
    guard_run(thorough)  ← 因为有 touches_interfaces 节点
    goal_complete → learn_compound

DONE
```

---

## 7. Worker Design（极简化）

### 当前 Worker（12 个固定 phase）——移除

```
Phase 1: Verify Config
Phase 2: Re-anchor
Phase 3: Investigation
Phase 4: TDD Red-Green (optional)
Phase 5: Implement
Phase 6: Verify & Fix
Phase 7: Commit
Phase 8: Review (optional)
Phase 9: Outputs Dump
Phase 10: Complete
Phase 11: Memory Auto-Save
Phase 12: Return
```

### V3 Worker（目标 + 约束 + 工具）

Worker 是一个 subagent，它本身就是一个 Claude。过度约束反而降低效果。

Worker prompt 模板：

```markdown
## 你的任务
{node.objective}

## 约束
{node.constraints}

## 风险级别
{node.guidance.estimated_scope} — {node.guidance.risk_rationale}

## 历史经验（来自知识库）
{node.injected_patterns}

## 可用工具
- 使用 lock_acquire/lock_release 管理文件锁
- 使用 guard_run 验证代码质量
- 使用 learn_record 记录重要发现

## 完成条件
- 代码变更已提交
- guard 通过
- 简短总结写入 graph_done
```

**Anthropic 的研究**："Each subagent needs an objective, an output format, guidance on the tools and sources to use, and clear task boundaries." 仅此而已。

---

## 8. Skill 精简

### 当前：52 个 SKILL.md + 27 个 slash commands

### V3：3 个入口 + 领域知识下沉

```
/flow-code:go    → 主入口（决策树：评估 → 选模式 → 循环）
/flow-code:learn → 知识管理（查看/搜索/刷新 patterns）
/flow-code:score → 运行 fitness function（score_driven 模式专用）
```

#### /flow-code:go 的 SKILL.md（决策树，不是 pipeline 脚本）

```markdown
---
name: flow-code:go
description: Goal-driven adaptive execution. Creates goal, selects mode, runs optimization/execution loop.
---

## 入口

调用 goal_create MCP tool，传入用户请求。

## 路由

根据返回的 mode 字段执行：

### Direct 模式 (mode = "direct")
1. 直接执行所需修改
2. 调用 guard_run(depth: "trivial")
3. 调用 goal_complete
4. 结束

### Score-Driven 模式 (mode = "score_driven")
1. 调用 graph_plan 生成执行图
2. 调用 learn_search 检索相关经验
3. 循环：
   a. 调用 graph_next 获取可执行节点
   b. 为每个 ready 节点 spawn Worker agent（并行）
   c. Worker 完成后调用 graph_done
   d. 检查 goal_score — 分数涨了则继续，跌了则 revert
   e. 如果连续失败，调用 graph_escalate
4. 当 score >= target 时，调用 goal_complete
5. 调用 learn_compound 归纳知识

### Goal-Driven 模式 (mode = "goal_driven")
1. 调用 graph_plan 生成执行图（with IssueGuidance）
2. 调用 learn_search 检索相关经验
3. 循环：
   a. 调用 graph_next 获取可执行节点
   b. 为每个 ready 节点 spawn Worker agent（并行）
   c. Worker 完成后调用 graph_done
   d. 验证 acceptance criteria
   e. 如果发现新信息，调用 graph_escalate 评估 replan
4. 所有 criteria MET 后，调用 guard_run(depth from IssueGuidance)
5. 调用 goal_complete
6. 调用 learn_compound 归纳知识

## Worker 指令
Worker 只需要：objective + constraints + injected_patterns。
不要给 Worker 固定的步骤。让它自主决定怎么完成目标。
```

#### 52 个领域 skill 的命运

当前的领域 skill（flow-code-tdd、flow-code-security、flow-code-database 等）的知识不会丢失——它们**变成 patterns layer 的种子数据**：

```bash
# 首次启动 V3 时，执行一次性迁移
flowctl serve --migrate-skills
# 读取 skills/*/SKILL.md → 提取领域知识
# → 写入 .flow/learned/patterns/ 作为初始 patterns
# → confidence: 0.8（人类编写，但需要实践验证）
# → decay_days: 180（半年内需被实际使用验证）
```

---

## 9. Hook Design（精简）

### 当前：8 个 hook 点，多个处理器

### V3：3 个 hook + MCP server 内部处理大部分逻辑

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "echo 'flowctl MCP server auto-starts via .mcp.json'",
          "timeout": 3
        }]
      }
    ],
    "UserPromptSubmit": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "node \"$CLAUDE_PLUGIN_ROOT/scripts/keyword-detector.js\"",
          "timeout": 3
        }]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [{
          "type": "command",
          "command": "node \"$CLAUDE_PLUGIN_ROOT/scripts/session-end.js\"",
          "timeout": 5
        }]
      }
    ]
  }
}
```

**为什么精简到 3 个**：

大部分 hook 逻辑移入 MCP server 内部：
- PreToolUse 的 ralph-guard → MCP server 的 graph state 检查
- PostToolUse 的 task-completed → graph_done tool 内部处理
- PreCompact 的 state 注入 → MCP server 可以通过 notification 处理
- SubagentStart/Stop → Worker 通过 MCP tools 自己管理

新增的 **UserPromptSubmit**（来自 OMC）：
- 关键词检测："go" → 自动调用 /flow-code:go
- 关键词检测："score" → 自动调用 /flow-code:score
- 关键词检测："learn" → 自动调用 /flow-code:learn

新增的 **Stop**（来自 knowledge 项目）：
- session 结束时，触发 learn_record 保存未记录的发现
- 触发 goal 状态快照（便于下次 resume）

---

## 10. Storage Design（.flow/ 目录演化）

### 当前

```
.flow/
├── .state/epics/          # Epic JSON
├── .state/tasks/          # Task JSON
├── .state/.state/         # Task runtime state
├── .state/pipeline.json   # Pipeline phase state
├── .state/memories/       # Memory entries
├── .state/events/         # Event log
├── specs/                 # Spec markdown
├── reviews/               # Review receipts
├── locks/                 # File locks
├── graph.bin              # Code graph
├── index/ngram.bin        # Trigram index
└── config.json            # Config
```

### V3

```
.flow/
├── goals/                 # GoalContext JSON（取代 epics/ + pipeline.json）
│   └── g-1-add-oauth.json
├── graph/                 # AdaptiveGraph state（取代 tasks/ + .state/）
│   └── g-1-add-oauth.graph.json
├── learned/               # 三层知识金字塔（新增）
│   ├── learnings/         # Layer 1: 原子经验（每次执行产出）
│   │   └── 2026-04-10-oauth-token-refresh.md
│   ├── patterns/          # Layer 2: 归纳模式（定期 compound）
│   │   └── oauth-best-practices.md
│   └── sessions/          # 会话历史摘要（可检索）
│       └── g-1-add-oauth.summary.json
├── iterations/            # Fitness function 迭代日志（score_driven 模式）
│   └── g-1.jsonl          # 每行：{iteration, score, action, result, timestamp}
├── locks/                 # 文件锁（保留）
├── graph.bin              # 代码图索引（保留）
├── index/ngram.bin        # Trigram 索引（保留）
└── config.json            # 配置（保留）
```

---

## 11. Implementation Roadmap

### Phase 0: Preparation（准备）
- [ ] 在 flowctl Cargo.toml 中添加 rmcp 依赖
- [ ] 创建 `flowctl/crates/flowctl-mcp/` crate
- [ ] 实现 `flowctl serve` 子命令（启动 MCP server via stdio）
- [ ] 更新 `.mcp.json` 指向 `bin/flowctl serve`
- [ ] 验证 Claude Code 能连接到 MCP server

### Phase 1: Goal + Mode Selection（核心转变）
- [ ] GoalContext 数据结构（.flow/goals/）
- [ ] goal_create tool：codebase_assess + detect_natural_metric + mode selection
- [ ] goal_status tool：返回状态 + suggested_action
- [ ] goal_score tool：运行 fitness function（ScoreDriven 模式）
- [ ] goal_complete tool：触发 learning 提取
- [ ] Direct 模式端到端可用

### Phase 2: Adaptive Graph（动态执行图）
- [ ] AdaptiveGraph 数据结构（.flow/graph/）
- [ ] graph_plan tool：生成图 + IssueGuidance 标注
- [ ] graph_next tool：返回 ready nodes + inject patterns
- [ ] graph_done tool：完成 + guard + score delta
- [ ] graph_escalate tool：三层升级逻辑
- [ ] ScoreDriven 模式端到端可用
- [ ] GoalDriven 模式端到端可用

### Phase 3: Knowledge Pyramid（知识复利）
- [ ] Learner 数据结构（.flow/learned/）
- [ ] learn_record tool：记录 learning
- [ ] learn_search tool：三层统一检索
- [ ] learn_compound tool：归纳 patterns
- [ ] learn_refresh tool：衰减 + 刷新
- [ ] 迁移脚本：52 个 SKILL.md → patterns seed data

### Phase 4: Infrastructure（基础设施）
- [ ] guard_run tool：风险比例深度
- [ ] lock_acquire / lock_release tools
- [ ] codebase_assess tool（复用已有 graph + trigram）
- [ ] UserPromptSubmit hook（关键词检测）
- [ ] Stop hook（session-end learning）

### Phase 5: Migration & Cleanup（迁移与清理）
- [ ] 精简 SKILL.md 到 3 个入口
- [ ] 精简 agent.md 到 1 个 Worker 模板
- [ ] 精简 hooks.json
- [ ] 更新 CLAUDE.md
- [ ] 更新文档
- [ ] 保留 flowctl CLI 作为调试入口（所有 MCP tools 也可通过 CLI 调用）
- [ ] 版本号升级到 v0.2.0

---

## 12. Technical Decisions

### 12.1 MCP SDK 选型

```toml
# flowctl/crates/flowctl-mcp/Cargo.toml
[dependencies]
rmcp = { version = "0.16", features = ["server", "macros", "schemars"] }
tokio = { version = "1", features = ["full"] }
serde = { workspace = true }
serde_json = { workspace = true }
```

- **rmcp**：官方 Rust MCP SDK
- **tokio**：async runtime（MCP server 需要）
- **注意**：这是 flowctl 首次引入 async runtime。仅限 MCP crate，core 保持同步

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
- flowctl CLI 子命令照常工作（不经过 MCP）
- `flowctl serve` 启动 MCP server 模式

### 12.3 状态持久化

- **内存缓存 + 文件持久化**：Server 运行时状态在内存，每次变更写入 .flow/
- **不引入 SQLite**：保持零外部依赖（Hermes 用 SQLite，但我们的数据量不需要）
- **trigram index 复用**：learn_search 使用已有的 NgramIndex
- **bincode 复用**：graph 状态可以用 bincode 序列化（已有基础设施）

### 12.4 CLI 兼容性

flowctl CLI 保留所有现有命令——它们作为**调试和管理入口**：

```bash
# MCP tools 的 CLI 等价物
flowctl goal create "add OAuth" --json        # = goal_create MCP tool
flowctl goal status g-1 --json                # = goal_status MCP tool
flowctl graph next g-1 --json                 # = graph_next MCP tool
flowctl learn search "OAuth" --json           # = learn_search MCP tool

# 管理命令（CLI only，不暴露为 MCP tool）
flowctl learn list --patterns --json          # 查看所有 patterns
flowctl learn import --file patterns.md       # 手动导入 pattern
flowctl config set guard.default_depth standard
```

---

## 13. What We Keep, What We Remove, What We Add

### Keep（保留）

| 能力 | 理由 |
|------|------|
| flowctl Rust 二进制 | 演化为 MCP server（增加 `serve` 子命令） |
| .flow/ JSON 存储 | 零依赖持久化 |
| graph.bin + trigram index | 代码智能基础设施 |
| file locking | Worker 并行安全 |
| api_version + --input-json | CLI 接口刚对齐（v0.1.53） |
| guard 命令 | 质量门禁（增加风险比例深度） |
| 178 个 CLI 子命令 | 作为调试/管理入口 |

### Remove（移除）

| 能力 | 理由 |
|------|------|
| 6-phase pipeline（phase next/done） | GoalContext + AdaptiveGraph 取代 |
| 12-phase worker protocol | Worker 只收 objective + constraints |
| 52 个 SKILL.md | 精简为 3 个入口 + patterns seed |
| 24 个 agent.md | 精简为 1 个 Worker 模板 + 保留必要 scouts |
| ralph-guard hook | 逻辑移入 MCP server |
| commit-gate hook | 逻辑移入 MCP server |
| pre-compact hook | 逻辑移入 MCP server |
| Codex integration | 保留 CLI 命令但不作为 MCP tool |
| RP integration（在 MCP server 中） | 保留 CLI 命令但不作为 MCP tool |

### Add（新增）

| 能力 | 理由 |
|------|------|
| MCP Server（`flowctl serve`） | 持久进程 + 结构化工具 |
| GoalContext + 三模式 | 目标驱动取代过程驱动 |
| Fitness Function | 可执行的分数脚本 |
| 三层分级升级 | SWE-AF 模式 |
| 三层知识金字塔 | Compound + Hermes + arscontexta |
| IssueGuidance（风险比例标注） | SWE-AF 模式 |
| UserPromptSubmit hook | 关键词触发（OMC 模式） |
| iterations.jsonl | 实验日志（goal-md 模式） |

---

## 14. Success Metrics

V3 成功的标志：

1. **简单任务快 10 倍**：Direct 模式 3-4 tool calls vs 当前 pipeline 20+ 步
2. **复杂任务更可靠**：三层升级 + replan vs 当前 review circuit breaker
3. **每次执行都学习**：goal_complete 后知识库增长
4. **第 N 次做类似任务比第 1 次快**：patterns 注入减少试错
5. **分数可验证**：ScoreDriven 模式的 iterations.jsonl 记录每步分数变化
6. **零依赖**：纯 Rust 单二进制，无 Node.js/Python/SQLite

---

## Appendix A: Reference Projects Index

| 项目 | 贡献的核心思想 | 文件 |
|------|--------------|------|
| goal-md | Fitness function + action catalog + dual-score | README.md, CLAUDE.md |
| SWE-AF | 8 architectural patterns（升级控制、风险比例、知识传播等） | docs/ARCHITECTURE.md |
| PACEflow | Hook 物理拦截（100% vs 70%） | hooks/pre-tool-use.js |
| Compound Engineering | 知识复利 + 衰减生命周期 | skills/ce-compound-refresh/SKILL.md |
| arscontexta | 自我改进的方法论 + 认知架构衍生 | methodology/ |
| Hermes Agent | 闭合学习环 + FTS5 检索 + 技能提取 | agent/skill_utils.py, agent/memory_manager.py |
| OMC (oh-my-claudecode) | 关键词触发 + model routing + MCP tools | hooks/hooks.json, AGENTS.md |
| agent-council | 多模型并行意见 + Chairman 综合 | README.md |
| multi-agent-shogun | 零 API 文件协调 + Bloom 级别路由 | instructions/karo.md |
| Spec-Flow | Domain Memory + 原子 Worker | CLAUDE.md |
| BMAD-METHOD | Party Mode 多角色圆桌 | src/core-skills/bmad-party-mode/SKILL.md |
| Anthropic Research | Orchestrator-Worker + 编排指导 | anthropic.com/engineering/multi-agent-research-system |
| Anthropic Building Agents | 6 composable patterns + simplicity principle | anthropic.com/research/building-effective-agents |
| rmcp | Official Rust MCP SDK | github.com/modelcontextprotocol/rust-sdk |
