# .flow 状态目录设计分析

> 基于行业最佳实践 + gstack 参考 | 2026-04-09

## 结论

**项目内 `.flow/` 是正确的选择**，符合行业共识（git/.git, cargo/target, nx/.nx）。
不应移到 `~/.flow/projects/`。

## 当前已有的 CWD 修复（足够）

1. `get_flow_dir()` 向上遍历 — 子目录执行时自动找到根目录的 .flow
2. `--project-dir` / `-C` — 显式指定项目目录
3. `FLOW_STATE_DIR` 环境变量 — CI/脚本使用

## 未来可选改进（不紧急）

### 1. 分层存储（gstack 模式）

```
.flow/              ← 运行时状态（gitignore）
.flow-config/       ← 团队配置（git 追踪）
~/.flow/projects/   ← 跨会话记忆（全局）
```

### 2. Slug 缓存（gstack 的 slug-cache）

用 `git remote get-url origin` 生成项目 slug，缓存到 `~/.flow/slug-cache/`。
下次找 .flow 时先查缓存，不用遍历。

### 3. Frecency 移到全局

文件频率数据是跨会话积累的，放到 `~/.flow/projects/{slug}/frecency.json`。
不影响当前功能，只是位置更合理。

## 不做的理由

把 .flow 拆成 3 个位置需要修改 flowctl 所有文件路径解析（~50 个文件引用 .flow）。
投入产出比不够。当前 3 个 CWD 修复已经解决了 5 轮审计的 #1 问题。

---

*Generated 2026-04-09*
