# Rust Crate 优化调研报告

> 基于 2026-04 Rust 生态最新状态 | flowctl 性能优化选型

---

## 当前 flowctl 状态

| 指标 | 当前值 |
|------|--------|
| 二进制大小 | ~3.5MB |
| 直接依赖 | ~20 个 |
| 传递依赖 | ~259 个 |
| 测试 | 359 pass |

---

## 一、搜索/索引层：当前 trigram 够用吗？

### 结论：够用，但需要 3 个工程优化

**不用换 tantivy。** tantivy 会加 150+ 依赖、+2-4MB 二进制，而它的优势（BM25 评分、词干、分词）对代码搜索无用。代码搜索的核心是精确/regex 子串匹配，不是自然语言相关性排序。

Cursor、GitHub Code Search、Sourcegraph 的生产系统都用 trigram 索引 — 和我们的 ngram_index.rs 是同一个算法。差距是工程打磨：

| 优化 | 影响 | 新增 crate | 工作量 |
|------|------|-----------|--------|
| **二进制序列化**（替换 JSON） | 索引加载快 10-100x | `bincode`（3 deps） | 低 |
| **memchr 验证**（替换字符串搜索） | 候选验证快 2-5x | 0（已在依赖树中） | 低 |
| **regex→trigram 分解** | 支持索引化 regex 搜索 | 0（`regex-syntax` 已在树中） | 中 |

可选（大仓库才需要）：
- rayon 并行索引构建（~5 deps）
- mmap 加载（memmap2 已有）

---

## 二、模糊匹配层：nucleo 还是最优吗？

### 结论：nucleo 仍是最佳选择

| 维度 | nucleo-matcher 0.3.1 | frizbee 0.9.0 |
|------|---------------------|---------------|
| 速度 | 基准 | 1.8x 更快 |
| Unicode | ✅ 完整 grapheme | ❌ 只处理字节 |
| 错别字容忍 | ❌ | ✅ 可配置 max_typos |
| 生态 | Helix/Nushell/Television/Atuin | blink.cmp/skim/fff.nvim |
| 稳定性 | 接近 1.0，API 稳定 | 0.9.x，有破坏性更新 |
| Frecency | 无（外部组合） | 无（外部组合） |

**nucleo 的优势**：Unicode 正确性 + 生态最广 + API 稳定。frizbee 更快但无 Unicode、API 不稳定。AI agent 不打错字所以 typo tolerance 价值有限。

**Frecency 最佳实践**：Atuin 模式 — matcher 分数 × frecency 权重，外部组合。当前 flowctl 已经这样做了。

---

## 三、代码编辑/补丁层：发现更好的 crate

### 关键发现：两个 crate 比 fudiff 更好

**1. diff-match-patch-rs** — Google DMP 的最快 Rust 实现
- diff + **fuzzy match** + fuzzy patch 三合一
- `match_main()` 函数：给定模式和大致位置，用 bitap 算法找最佳匹配 — **这正是"在文件中找到近似文本并替换"的最优原语**
- 零依赖
- 比其他 DMP 实现更快，WASM 就绪

**2. flickzeug** — 最佳模糊 patch 应用（prefix-dev 出品）
- diffy 的活跃 fork
- 用 Levenshtein 距离做行级模糊匹配（≥80% 相似即接受）
- GNU patch 风格 fuzz level（精确→减少上下文→模糊行匹配）
- 保留原文件上下文行（只应用实际的增删）
- 支持三路合并
- conda-forge 生产验证

**3. imara-diff** — 最快 diff 算法
- 比 `similar` 快 30x（极端情况）
- Myers + Histogram 算法，gnu-diff/git 启发式
- 零依赖

### 建议：替换 fudiff → diff-match-patch-rs + flickzeug

| 场景 | 当前 | 优化后 |
|------|------|--------|
| 精确替换 | fudiff | `str::replace`（直接） |
| 近似文本定位 | fudiff 上下文匹配 | `diff-match-patch-rs::match_main()`（bitap 算法） |
| 统一 diff 应用 | fudiff | `flickzeug`（Levenshtein 行级模糊） |
| 快速 diff 生成 | fudiff | `imara-diff`（30x 快于 similar） |

---

## 四、语义理解层：可以做本地语义搜索吗？

### 结论：可以，但代价大

**本地语义搜索最轻栈**：
```
tree-sitter（AST 分块） + fastembed（ONNX 嵌入） + usearch（ANN 搜索）
```

| 组件 | crate | 大小增加 |
|------|-------|---------|
| AST 分块 | tree-sitter + 5 grammar | +3MB |
| 嵌入生成 | fastembed (ort 后端) | +15MB 运行时 + 22MB 模型 |
| 向量搜索 | usearch | +1MB |
| **合计** | | **+41MB** |

flowctl 从 3.5MB → ~45MB。**12x 膨胀。**

### 更轻的替代：BM25 关键词搜索
```
tree-sitter（分块） + bm25 crate（关键词评分）
```
+3MB（tree-sitter）+ <1MB（bm25）= **+4MB**。无 ML 模型，无 ONNX 运行时。

损失语义理解但保持轻量。对"找到处理认证的代码"这类查询，BM25 + tree-sitter 分块的效果已经不错。

---

## 五、AST 解析层：tree-sitter-language-pack 的真相

### 关键发现：该 crate 不是我们想的那样

调研发现 **没有一个成熟的 tree-sitter 全语言包 crate**。实际做法：

| 方案 | 谁在用 | 优缺点 |
|------|--------|--------|
| 静态链接 10-15 个 grammar crate | Helix, Difftastic | 可控，但 +2-5 分钟编译 |
| WASM 动态加载 | Zed | 最灵活，但 +15MB (wasmtime) |
| **Helix tree-house** | Helix | **新方案**：可复用的 tree-sitter 库 |
| 当前 regex 提取 | flowctl | 零依赖但不精确 |

**Helix 的 tree-house** 是最有前景的：从 Helix 编辑器提取的可复用 tree-sitter 集成库，包含高亮、query 迭代、injection 处理。

### ADR-006 验证：regex 提取暂时是对的

当前用 regex 提取符号（ADR-006）避免了 tree-sitter 的编译时间和二进制膨胀。未来升级路径：
1. **Phase 1（当前）**：regex 提取，零依赖
2. **Phase 2**：静态链接 5 个 grammar（Rust/TS/Python/Go/JS），+3MB
3. **Phase 3**：用 Helix tree-house 做完整代码智能

---

## 实施优先级

### 立即可做（零/极少新依赖）

| 改进 | crate | 依赖 | 效果 |
|------|-------|------|------|
| N-gram 索引二进制序列化 | bincode | 3 | 索引加载快 100x |
| memchr 候选验证 | 0（已有） | 0 | 子串验证快 2-5x |
| regex→trigram 分解 | 0（已有 regex-syntax） | 0 | 支持索引化 regex 搜索 |

### 下一版本（值得加的新 crate）

| 改进 | crate | 依赖 | 效果 |
|------|-------|------|------|
| 替换 fudiff → diff-match-patch-rs | diff-match-patch-rs | 0 | 更好的模糊定位（bitap 算法） |
| 加 flickzeug 模糊 patch | flickzeug | strsim | 行级 Levenshtein 模糊 |
| 加 imara-diff 快速 diff | imara-diff | 0 | 30x 快于 similar |

### 长期（需要评估 ROI）

| 改进 | crate | 大小增加 | 效果 |
|------|-------|---------|------|
| tree-sitter 精确 AST | tree-sitter + grammars | +3MB | 精确符号提取 |
| BM25 关键词搜索 | bm25 | <1MB | "找处理X的代码" |
| 本地语义搜索 | fastembed + usearch | +41MB | "找类似功能的代码" |

---

*Generated 2026-04-09 by flow-code research*
