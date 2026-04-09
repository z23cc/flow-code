# 12 个执行问题深度分析与修复方案

> 基于真实项目使用反馈 + 开源最佳实践 | 2026-04-09

---

## 问题分级

| 级别 | 问题 | 影响 |
|------|------|------|
| **P0 阻断** | #1 状态丢失, #3 状态不持久, #9 .flow目录设计 | 流水线无法完成 |
| **P1 严重** | #11 缺少恢复能力, #5 Worker并行失效, #8 guard不可用 | 功能形同虚设 |
| **P2 痛点** | #2 ID太长, #12 slug不可控, #6 brainstorm浪费, #7 review虚设 | 效率和体验差 |
| **P3 改进** | #4 epic互不感知, #10 零交互冲突 | 设计权衡 |

---

## P0: 状态丢失（#1 #3 #9）— 根因统一

### 根因分析

三个问题的根因相同：**`get_flow_dir()` 硬编码为 `$CWD/.flow/`**。

```rust
// 当前实现 (helpers.rs:14-18)
pub fn get_flow_dir() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)  // 就是 ".flow"
}
```

问题：
- 如果 `cd backend/` 再执行 flowctl → 找的是 `backend/.flow/`（不存在）
- 如果 `.flow` 是符号链接到 `.git/flow-state/flow/` → 在子目录执行时链接不可见
- 写入成功但下次从不同目录读取 → "epic not found"

### 修复方案：向上遍历找 .flow（git 同款模式）

```rust
pub fn get_flow_dir() -> PathBuf {
    // 1. 环境变量优先（显式覆盖）
    if let Ok(dir) = env::var("FLOW_STATE_DIR") {
        return PathBuf::from(dir);
    }
    
    // 2. 向上遍历目录树找 .flow（同 git 找 .git 的逻辑）
    let mut current = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        let candidate = current.join(FLOW_DIR);
        if candidate.exists() {
            return candidate;
        }
        if !current.pop() {
            break;
        }
    }
    
    // 3. 回退到 CWD/.flow（init 时创建）
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FLOW_DIR)
}
```

**这一个修复同时解决 #1、#3、#9。** 在任何子目录执行 flowctl 都能找到根目录的 .flow。

开源参考：git 本身的 `discover_git_directory()` 就是向上遍历。Cargo 的 `find_root_manifest()` 也是同样模式。

---

## P0+P1: 状态恢复（#11）

### 根因
代码已写好、已提交，但 flowctl 不知道。没有"从文件系统状态重建 .flow 状态"的能力。

### 修复方案：`flowctl recover`

```bash
flowctl recover --epic fn-3
# 扫描 git log 找到该 epic 的 commits
# 检查每个 task spec 对应的文件是否已存在/已修改
# 自动标记已完成的 task 为 done
# 输出恢复报告
```

实现逻辑：
1. 读取 epic 的所有 task specs
2. 对每个 task：检查 `--files` 字段的文件是否被 git 追踪了新变更
3. 检查 git log 中是否有 `Task: fn-N.M` 格式的 commit message
4. 匹配到的 task 标记为 done，附带 evidence（git commits）
5. 未匹配的保持 todo/in_progress

---

## P2: ID 太长（#2 #12）

### 根因
`slugify(title, 40)` 对长标题截断不够激进。"Django+React platform with account management, payment automation, and screenshot logging" → 40 字符的 slug 仍然很长。

### 修复方案：双层 ID

```
显式短 ID:  fn-3        ← epic 编号（已支持）
完整 ID:    fn-3-django-react-platform  ← slug（截断到 20 字符）
Task 短 ID: fn-3.1      ← 永远可用
Task 完整:  fn-3-django-react-platform.1
```

改动：
1. **slug 最大长度从 40 → 20**
2. **所有命令支持短 ID `fn-N`**（已部分支持，需要补全）
3. **dep 命令支持 `.N` 相对引用**：`--deps ".1,.2"` 自动展开为当前 epic 的 task

```rust
// id.rs 修改
let slug = slugify(title, 20);  // 从 40 改为 20

// dep.rs 修改：支持相对 ID
if dep_id.starts_with('.') {
    format!("{}{}", current_epic_id, dep_id)
}
```

开源参考：GitHub 用 `#123`（纯数字），Linear 用 `PRJ-123`（前缀+数字），都极短。

---

## P1: guard 不可用（#8）

### 根因
`flowctl guard` 尝试执行 test/lint/typecheck 命令，但项目没有配置 → 命令失败 → 阻塞 close 阶段。

### 修复方案：graceful fallback

```rust
// guard.rs 修改
fn run_guard_command(cmd: &str, layer: &str) -> GuardResult {
    // 如果命令为空，跳过
    if cmd.is_empty() { return GuardResult::Skipped; }
    
    // 检查命令的第一个词是否在 PATH 中
    let program = cmd.split_whitespace().next().unwrap_or("");
    if which::which(program).is_err() {
        return GuardResult::Skipped(format!("{program} not found in PATH"));
    }
    
    // 执行命令
    match Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(output) if output.status.success() => GuardResult::Pass,
        Ok(output) => GuardResult::Fail(stderr),
        Err(e) => GuardResult::Skipped(format!("cannot run: {e}")),
    }
}
```

**所有 guard 命令都有三种结果**：Pass / Fail / Skipped。Skipped 不阻塞流水线，只输出警告。

开源参考：pre-commit hooks 的 `language_version: system` 就是"用系统有的，没有就跳过"。

---

## P2: brainstorm 浪费（#6）

### 根因
brainstorm 阶段不区分任务复杂度。"修复 typo" 和 "重构认证系统" 走同样的 6-10 Q&A。

### 修复方案：已有 `--quick`，但自动检测不够激进

当前 `--quick` 需要显式传入。改为**自动检测**：

```markdown
## Quick 路径自动检测（flow-code-run SKILL.md）

自动跳过 brainstorm 的信号（满足 ANY ONE 即跳过）：
- 用户输入 ≤ 10 个词
- 输入包含 "fix"/"typo"/"config"/"update"/"bump"/"rename"
- 输入指向单一文件 (路径出现在输入中)
- `--quick` 标志

自动跳过 brainstorm + plan 的信号：
- 满足上述 + 涉及文件 ≤ 2 个
- 用户说 "simple"/"trivial"/"small"/"minor"
```

---

## P2: review 虚设（#7）

### 根因
`review-backend` 返回 `rp` 但 RP 实际不可用（没有窗口/没有 rp-cli）。技能层看到 backend=rp 就尝试执行，失败后 agent 手动 phase done 跳过。

### 修复方案：backend 可用性验证

```rust
// review_backend.rs 修改
pub fn cmd_review_backend(json: bool, ...) {
    let configured = get_configured_backend();  // rp / codex / none
    
    // 验证实际可用性
    let available = match configured.as_str() {
        "rp" => which::which("rp-cli").is_ok() || check_rp_mcp_available(),
        "codex" => which::which("codex").is_ok(),
        "none" => true,
        _ => false,
    };
    
    // 如果配置了但不可用，降级到 none 并警告
    let effective = if available { configured } else {
        eprintln!("warning: review backend '{}' configured but not available, skipping", configured);
        "none".to_string()
    };
    
    output(effective);
}
```

**配置了但不可用 = 自动降级到 none + 警告**，而不是让 agent 去碰壁。

---

## P1: Worker 并行失效（#5）

### 根因
不是 Worker 机制本身的问题——是 #1（状态丢失）导致 task 无法标记完成，进而让并行变得不可能。

### 修复
修复 #1 (get_flow_dir 向上遍历) + #11 (recover) 后，Worker 并行自然恢复。

额外改进：Worker 失败时应该自动标记 `failed`（而不是卡住）：
```rust
// worker timeout 后
flowctl fail <task-id> --reason "worker timeout"
// 下一个 wave 可以继续其他 ready tasks
```

---

## P3: 零交互冲突（#10）

### 根因
设计权衡——零交互适合 Ralph（无人值守），但用户在场时希望能介入。

### 修复方案：模式区分

```
/flow-code:go "idea"              ← 零交互（默认，适合 Ralph）
/flow-code:go "idea" --interactive ← 关键决策点询问用户
```

关键决策点（只在 --interactive 时询问）：
1. brainstorm 后：确认选中的方案
2. plan 后：确认任务分解
3. impl_review 后：确认是否需要修复

默认仍然零交互（不破坏 Ralph）。

---

## P3: epic 互不感知（#4）

### 根因
epic 之间通过 `epic add-dep` 可以声明依赖，但状态恢复后无法自动恢复另一个 epic 的任务。

### 修复
这个已经有基础设施（`epic add-dep`、`plan-sync` cross-epic）。真正需要的是 #1 和 #11 的修复——状态不丢失，cross-epic 自然工作。

---

## 实施优先级

| 优先级 | 修复 | 影响面 | 代码量 | 依赖 |
|--------|------|--------|--------|------|
| **P0-1** | get_flow_dir 向上遍历 | 解决 #1 #3 #5 #9 | ~30 行 Rust | 无 |
| **P0-2** | flowctl recover 命令 | 解决 #11 | ~150 行 Rust | 无 |
| **P1-1** | guard graceful fallback | 解决 #8 | ~40 行 Rust | 无 |
| **P1-2** | review-backend 可用性验证 | 解决 #7 | ~30 行 Rust | 无 |
| **P2-1** | slug 截断到 20 + 相对 dep ID | 解决 #2 #12 | ~20 行 Rust | 无 |
| **P2-2** | brainstorm 自动检测跳过 | 解决 #6 | ~10 行 Markdown | 无 |
| **P3-1** | --interactive 模式 | 解决 #10 | ~20 行 Markdown | 无 |

**总计：~300 行 Rust + ~30 行 Markdown。零新依赖。**

最关键的一个修复（get_flow_dir 向上遍历）同时解决 4 个 P0/P1 问题。

---

*Generated 2026-04-09 by flow-code analysis*
