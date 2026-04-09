# 剩余 3 个问题解决方案

> 基于 5 轮审计 + Claude Code hook 机制分析 | 2026-04-09

---

## 1. Worker Worktree 隔离 — PreToolUse Hook 拦截

### 方案

在 `hooks/hooks.json` 的 `PreToolUse` 中加一个 matcher `"Agent"`，hook 脚本检查：
1. 读取 `$TOOL_INPUT` 环境变量（Claude Code 传给 hook 的 JSON）
2. 检查是否有 `isolation: "worktree"` 参数
3. 如果没有，检查是否有多个 in_progress 任务
4. 如果有并行任务但没有 worktree → hook 返回非零退出码 → Claude Code 阻止调用

```json
{
  "matcher": "Agent",
  "hooks": [
    {
      "type": "command",
      "command": "F=\"${DROID_PLUGIN_ROOT:-${CLAUDE_PLUGIN_ROOT:-}}/bin/flowctl\"; [ -x \"$F\" ] && \"$F\" hook check-worktree || true",
      "timeout": 5
    }
  ],
  "description": "Block Agent spawn without worktree isolation when parallel tasks exist"
}
```

`flowctl hook check-worktree` 实现：
```rust
pub fn cmd_check_worktree() {
    // 1. Read TOOL_INPUT env var
    let input = std::env::var("TOOL_INPUT").unwrap_or_default();
    
    // 2. Check if isolation: "worktree" is present
    if input.contains("\"isolation\"") && input.contains("\"worktree\"") {
        std::process::exit(0); // OK — has worktree
    }
    
    // 3. Check if there are parallel in_progress tasks
    let flow_dir = get_flow_dir();
    // ... count in_progress tasks ...
    
    if in_progress_count > 1 {
        eprintln!("⚠️ BLOCKED: Agent spawn without isolation:\"worktree\" while {} tasks in_progress.", in_progress_count);
        eprintln!("Add isolation: \"worktree\" to the Agent call to prevent race conditions.");
        std::process::exit(2); // Non-zero → Claude Code blocks the tool call
    }
    
    std::process::exit(0); // Single task or no parallel — OK
}
```

### 不确定性

Claude Code 的 `PreToolUse` hook 是否对 `Agent` 工具也触发？matcher 文档说支持工具名匹配，但 Agent 可能是内部工具。
需要测试：在 hooks.json 加一个 `"matcher": "Agent"` 的 PreToolUse hook，看是否触发。

### 备选方案（如果 hook 不对 Agent 触发）

改为在 `flowctl start` 时**硬拒绝**第二个并行任务，除非传 `--worktree-confirmed`：
```bash
flowctl start fn-1.2 --worktree-confirmed --json
```
不传 → 拒绝。这样 AI 必须在 start 之前确认 worktree。

---

## 2. Create Draft PR — flowctl epic completion 内置

### 方案

让 `flowctl epic completion` 自动运行 `gh pr create`：

```rust
// 在 epic completion ship 执行时：
fn auto_create_pr(epic_id: &str) {
    // 1. 检查是否在 feature branch（不是 main/master）
    let branch = Command::new("git").args(["branch", "--show-current"]).output();
    if branch == "main" || branch == "master" { return; } // 直接 push 到 main 不需要 PR
    
    // 2. Push branch
    Command::new("git").args(["push", "origin", "HEAD"]).status();
    
    // 3. Create draft PR
    let title = format!("feat: {}", epic_title);
    let body = format!("## Epic: {}\n\n{}", epic_id, epic_spec_summary);
    Command::new("gh").args([
        "pr", "create",
        "--title", &title,
        "--body", &body,
        "--draft"
    ]).status();
}
```

### 实现细节

- 只在 feature branch 上创建 PR（main/master 上不创建）
- `--no-pr` 标志跳过
- `gh` CLI 不可用时静默跳过（check `which gh`）
- PR body 包含 epic spec 摘要

---

## 3. Pre-launch 6 维度 — flowctl pre-launch 命令

### 方案

新增 `flowctl pre-launch --epic <id> --json` 命令，自动检查能自动化的维度：

```rust
pub fn cmd_pre_launch(epic_id: &str) -> PreLaunchResult {
    let mut checks = Vec::new();
    let changed_files = get_changed_files(); // git diff --name-only main...HEAD
    
    // 1. Code quality: guard 已跑过（检查 .state/ 标记）
    checks.push(check_guard_passed());
    
    // 2. Security: grep secrets in changed files
    let secrets = Command::new("grep")
        .args(["-rn", "password\\|secret\\|api_key\\|token\\|private_key"])
        .args(&changed_files)
        .output();
    checks.push(Check {
        name: "security",
        passed: secrets.stdout.is_empty(),
        detail: if secrets.stdout.is_empty() { "clean" } else { "SECRETS FOUND" },
    });
    
    // 3. Performance: check for N+1 patterns (简单启发)
    // 4. Accessibility: check if frontend files changed, if so flag for manual review
    let has_frontend = changed_files.iter().any(|f| 
        f.ends_with(".tsx") || f.ends_with(".jsx") || f.ends_with(".vue") || f.ends_with(".svelte")
    );
    checks.push(Check {
        name: "accessibility",
        passed: !has_frontend, // If frontend changed, needs manual check
        detail: if has_frontend { "frontend changed — verify a11y" } else { "no frontend changes" },
    });
    
    // 5. Infrastructure: check .env.example or env docs
    // 6. Documentation: check if README/CHANGELOG modified
    let has_doc_update = changed_files.iter().any(|f| 
        f.contains("README") || f.contains("CHANGELOG")
    );
    
    PreLaunchResult { checks, all_passed: checks.iter().all(|c| c.passed) }
}
```

然后在 `flowctl phase done --phase close` 门禁中加 `--pre-launch-ran` 标志（和 `--guard-ran` 一样）。

---

## 实施优先级

| # | 方案 | 代码量 | 效果 |
|---|------|--------|------|
| 1 | `flowctl pre-launch` 命令 + close 门禁 | ~200 行 Rust | 自动化 6 维度中 4 个 |
| 2 | `epic completion` 自动 PR | ~50 行 Rust | 解决 4 轮未创建 PR |
| 3 | PreToolUse hook 拦截 Agent | ~80 行 Rust + hooks.json | 解决 worktree（需验证 hook 是否对 Agent 触发） |

---

*Generated 2026-04-09*
