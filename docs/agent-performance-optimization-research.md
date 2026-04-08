# AI Agent 性能瓶颈优化调研报告

> 基于 2026-04 最新开源生态 | flow-code 集成方案

---

## 问题定义

AI coding agent 的主要性能瓶颈分 4 层：

| 层 | 瓶颈 | 典型耗时 | 影响 |
|----|------|---------|------|
| **搜索** | 每次从零搜索，无索引无记忆 | 2-15s/次 | 研究阶段占任务时间 40%+ |
| **理解** | 读大量无关文件，token 浪费 | 浪费 50-80% token | 上下文窗口耗尽，质量下降 |
| **修改** | 全文件重写，精确匹配失败 | 3-12s/次编辑 | 编辑失败重试浪费时间 |
| **上下文** | 命令输出冗余，无跨会话记忆 | 浪费 60-90% token | Ralph 长循环 token 爆炸 |

---

## 第一层：搜索优化

### 当前状态
flow-code 侦察兵用 Grep/Glob 从零搜索。每次调用都是全量遍历，无索引、无 frecency、无模糊匹配。

### 最优方案：三层搜索栈

| 层 | 工具 | 用途 | 新增依赖 |
|----|------|------|---------|
| **L1 模糊文件名** | `nucleo-matcher` | 防错别字的文件查找 + frecency 排序 | 3 crate |
| **L2 结构化搜索** | `ast-grep` (CLI/MCP) | "找所有调用 X 的函数" — AST 级匹配 | 外部 CLI |
| **L3 索引化文本搜索** | Sparse N-gram 索引（Cursor 方案）| 重复搜索 100-1000x 加速 | 自建 |

### 推荐：先做 L1（nucleo + frecency + ignore）

**为什么 nucleo-matcher**：
- Helix / Nushell / Television 都用，社区最成熟
- fzf 兼容评分（用户直觉一致）
- 仅 3 个依赖（memchr + unicode-segmentation + unicode-width）
- 比 skim 快 6x，比 fuzzy-matcher 快 6-10x
- 正确的 Unicode 处理

**为什么不用 frizbee**（fff.nvim 用的）：
- frizbee 更快（比 nucleo 快 1.8x）但更年轻
- 无 Unicode 支持（操作字节不处理 grapheme）
- AI agent 不会打错字，typo tolerance 价值有限
- nucleo 接近 1.0，API 更稳定

**Frecency 自建**（~50 行）：
```rust
// 指数衰减，参考 Mozilla Firefox 的 frecency 算法
new_score = old_score * 0.5^(days_elapsed / 14.0) + weight
// weight: git-modified=3.0, recently-opened=2.0, normal=1.0
```

**评分公式**：
```
final_score = nucleo_fuzzy_score × (1.0 + git_boost + frecency_boost)
git_boost:     staged=0.5, modified=0.3, untracked=0.1  
frecency_boost: min(frecency_score / 10.0, 0.5)
```

### L2 推荐：ast-grep 作为外部工具

ast-grep 已有 MCP server（`ast-grep-mcp` crate），可以直接注册：
```bash
claude mcp add ast-grep -- ast-grep-mcp
```
提供结构化搜索能力："找所有 async 函数"、"找所有 match 表达式缺少 _ arm 的"。

### L3 长期：Sparse N-gram 索引

Cursor 的方案：启动时构建 n-gram 倒排索引，后续 regex 查询走索引而非全量扫描。
- Cursor 实测：16.8s → 13ms（1300x 加速）
- AyGrep 开源实现：声称比 ripgrep 快 400x（索引化后）
- 适合 monorepo（100K+ 文件），flow-code 当前规模暂不需要

---

## 第二层：语义理解优化

### 当前状态
侦察兵逐文件读取，无法"按符号导航"。RP context_builder 有帮助但依赖外部工具。

### 最优方案：tree-sitter Repo Map（Aider 模式）

| 方案 | 工具 | 效果 | 复杂度 |
|------|------|------|--------|
| **Repo Map** | tree-sitter + PageRank | 1K token 概览全项目关键符号 | 中（需实现） |
| **符号级检索** | SCIP (via rust-analyzer) | 精确的跨文件定义/引用 | 高（需索引器） |
| **LSP 实时导航** | Serena MCP (包装 LSP) | 50ms 找到所有引用（vs 秒级 grep） | 低（注册 MCP） |
| **本地语义搜索** | nomic-embed-code + Qdrant | "找处理认证的代码" 自然语言查询 | 高（需模型+向量库） |

### 推荐：先用 Serena MCP + tree-sitter 代码结构

**短期（零开发）**：注册 Serena MCP
```bash
# Serena 包装 LSP，给 agent 精确的 go-to-definition / find-references
claude mcp add serena -- serena-mcp
```
效果：找函数所有调用点从秒级 grep 降到 50ms LSP 调用。

**中期（嵌入 flowctl）**：tree-sitter 代码结构提取
```bash
flowctl code-structure --path src/ --json
# → 提取所有函数/类型签名，不读实现体
# → 类似 RP 的 get_code_structure，但不依赖外部工具
```

用 `tree-sitter` Rust crate + 语言 grammar crate。依赖增加 ~5 个，但价值极高：
- Worker re-anchor 时看到"这个文件有哪些函数"而不是读全文
- Plan 阶段生成 repo map（1K token 概览全项目）
- 比读全文件省 80%+ token

---

## 第三层：代码修改优化

### 当前状态
Worker 用 Write/Edit 工具修改文件。Edit 要求精确匹配 old_string，匹配失败就报错。

### 行业最佳实践：Sketch + Apply 分离

| 方案 | 工具 | 效果 | 集成方式 |
|------|------|------|---------|
| **模糊匹配回退** | fudiff / flickzeug | 上下文匹配代替行号，容忍漂移 | Rust crate |
| **专用 Apply 模型** | Morph Fast Apply | 10,500 tok/s，98% 准确率，省 50-60% token | MCP/API |
| **AST 级重写** | ast-grep rewrite | 结构化替换，保证语法正确 | CLI |
| **语义 diff** | difftastic | tree-sitter AST diff，忽略格式变化 | CLI |

### 推荐：fudiff + Morph MCP

**fudiff**（嵌入 flowctl）：
```rust
// 上下文匹配的 patch 应用，不依赖行号
use fudiff::{diff, patch};
let d = diff(original, modified);
let result = patch(slightly_changed_original, &d); // 容忍文件已变化
```
解决 Edit 工具精确匹配失败的问题。零依赖 Rust crate。

**Morph Fast Apply**（MCP 注册）：
```bash
claude mcp add morph -- morph-mcp
```
Worker 用 Morph 的 edit_file 替代原生 Edit：
- 10,500 tok/s（比原生快 10x+）
- 98% 合并准确率
- 自动处理空白/缩进差异

---

## 第四层：上下文优化

### 当前状态
- flowctl 命令输出已有 `--compact` 过滤（TOML 8 阶段管道）
- RTK 已集成（hook 自动重写命令，省 60-90%）
- 无跨会话结构化记忆

### 行业最佳实践

| 方案 | 工具 | 效果 | 复杂度 |
|------|------|------|--------|
| **Repo 打包** | Repomix --compress | tree-sitter 结构提取，省 70% token | 外部 CLI |
| **符号级检索** | jCodeMunch MCP | ~90% context 缩减 | 注册 MCP |
| **代码结构** | code2prompt | 结构化 prompt + token 计数 | Rust CLI |
| **增量索引** | CocoIndex | tree-sitter 语义分块，增量更新 | Rust crate |

### 推荐：tree-sitter 代码结构 + Repomix 集成

**嵌入 flowctl**：`flowctl code-structure`（同第二层）
**外部工具**：侦察兵可调用 Repomix 压缩大文件

---

## 实施路线图

### Phase 1：立即可做（零开发/低开发，注册外部工具）

| 行动 | 工具 | 效果 | 工作量 |
|------|------|------|--------|
| 注册 ast-grep MCP | `ast-grep-mcp` | 结构化代码搜索 | 1 行命令 |
| 注册 Serena MCP | `serena-mcp` | LSP 精确导航（50ms） | 1 行命令 |
| 注册 Morph MCP | `morph-mcp` | 10x 快 edit + 语义搜索 | 1 行命令 |
| 更新侦察兵技能 | 修改 scout 技能 | 优先用 MCP 工具搜索 | 改几个 .md |

### Phase 2：核心嵌入（中等开发，进 flowctl Rust 代码）

| 行动 | 新增 crate | 效果 | 代码量 |
|------|-----------|------|--------|
| nucleo 模糊搜索 | `nucleo-matcher` + `ignore` | 防错别字 + gitignore + frecency | ~300 行 |
| Frecency 记录 | 自建 | 越用越准的文件排序 | ~100 行 |
| fudiff 模糊 patch | `fudiff` | 容忍文件漂移的 edit | ~50 行 |
| `flowctl search` 命令 | — | 统一搜索入口 | ~200 行 |
| `flowctl doctor` 增强 | — | 检测 MCP 工具可用性 | ~100 行 |

新增依赖预算：**~10 个 crate**（nucleo-matcher 3 + ignore 5 + fudiff 1 + similar 1）

### Phase 3：深度集成（大开发，长期）

| 行动 | 新增 crate | 效果 | 代码量 |
|------|-----------|------|--------|
| tree-sitter 代码结构 | `tree-sitter` + grammar crates | repo map + 符号提取 | ~500 行 |
| `flowctl code-structure` | — | 不读全文的代码概览 | ~300 行 |
| Sparse N-gram 索引 | 自建 | 重复搜索 100x+ 加速 | ~1000 行 |
| MCP server 模式 | `flowctl mcp-serve` | flowctl 作为 MCP server | ~500 行 |

---

## 依赖预算总览

| Phase | 新增 crate | 编译时间影响 | 二进制体积影响 |
|-------|-----------|-------------|---------------|
| 现状 | 0 | 基准 | 基准（~2.8MB） |
| Phase 2 | ~10 | +15-20% | +200-400KB |
| Phase 3 | ~15-20 | +30-40% | +500KB-1MB |
| fff-core 直接集成（对比） | ~67 | +60-80% | +2-3MB |

Phase 2 的 10 个 crate 比 fff-core 的 67 个精简 85%，但覆盖了 fff 90% 的核心价值。

---

## 一句话总结

> **不要造轮子，也不要搬运巨石。组合最佳 crate + 注册最佳 MCP = 最优性价比。**
> 
> 短期靠 MCP 生态（ast-grep、Serena、Morph），中期嵌入 nucleo + frecency + fudiff，长期做 tree-sitter repo map。

---

*Generated 2026-04-09 by flow-code research pipeline*
