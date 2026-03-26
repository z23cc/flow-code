# Artifact 格式参考

> 本文件是 artifact-management 的详细格式参考。Hook DENY/HINT 消息和格式问题排查时查阅此处。

---

## task.md 完整格式

```markdown
### CHG-YYYYMMDD-NN: 变更标题

<!-- APPROVED -->
<!-- VERIFIED -->

- [/] T-001 任务描述
- [ ] T-002 任务描述
- [x] T-003 已完成任务
```

- **CHG 分组**：每个变更独立 `###` 标题
- **`<!-- APPROVED -->`**：C 阶段获批后添加，放在 CHG 标题下方、任务列表上方
- **`<!-- VERIFIED -->`**：V 阶段验证通过后添加，放在 APPROVED 下方
- **任务条目**：`- [状态] T-NNN 任务标题`
- **归档**：将 `<!-- ARCHIVE -->` 标记上移到待归档 CHG 块上方（两步 Edit：插入新标记 → 删除旧标记）

---

## implementation_plan.md 完整格式

**索引条目**（活跃区顶部）：
```markdown
- [/] CHG-20260308-01 功能名称 — 简要描述 #change [tasks:: T-001~T-003]
```

**索引字段说明**：
- **checkbox 状态**：编码变更进度，兼容 Obsidian Tasks 跨项目查询
- **CHG-ID**：变更标识符
- **标题**：变更简述
- **#change**：Obsidian 标签，用于 Tasks/Dataview 过滤
- **[tasks:: T-NNN~T-NNN]**：Dataview inline field，关联 task.md 任务编号

**详情段落**（活跃变更详情区）：
```markdown
### CHG-20260308-01 功能名称

**背景（Why）**：为什么做这个变更。
**范围（What）**：~N 行改动，M 个文件。
**技术决策（How）**：方案选择及理由（如有取舍）。

**T-001 任务标题**：
- `src/api.js:42` handleRequest — 缺少 null 检查 → 添加参数校验
- 验收：/api/test?param=null 返回 400 而非 500

**T-002 任务标题**：
- `tests/api.test.js` — 新建，覆盖 null 参数场景
- 验收：测试通过
```

- 每个索引条目**必须**有对应的 `### CHG-ID` 详情段落
- Hook 检测 E 阶段前提：`/^- \[\/\]/m`（行首 `- [/] `）
- **禁止**用"见 docs/plans/xxx"替代详情——详情必须自包含

---

## walkthrough.md 格式

**索引表**（活跃区顶部）：
```markdown
| 日期 | 完成内容 | 关联变更 |
|------|---------|---------|
| 2026-03-10 | CHG-20260310-02 描述（详细内容摘要） | CHG-20260310-02 |
```

**详情段落**（索引表下方）：
```markdown
## 2026-03-10 CHG-20260310-02 描述

> **追加时间**: 2026-03-10T18:26:00+08:00

- 执行 CHG-20260310-02：完成内容详述
  - **T-001**：具体改动
  - **T-002**：具体改动
```

---

## findings.md 格式

**索引条目**（索引区 + 详情区双区格式）：
```markdown
- [x] JWT 安全最佳实践 — 关键结论摘要 #finding [date:: 2026-01-17] [change:: CHG-20260117-01]
```

**详情段落**：
```markdown
### [2026-01-17] JWT 安全最佳实践

**背景**：发现过程和触发事件。
**问题**：当前行为 vs 应有行为（含代码位置、影响范围）。
**方案**：建议的解决方案（多方案对比+推荐）。
```

> 每条新 finding 必须**同时**写入索引条目和详情段落。

---

## spec.md 同步触发词

以下操作触发 `spec.md` 更新：
- 安装新依赖（pip install / npm install / pnpm install）
- 添加新配置项（config.py / .env）
- 创建新核心模块（api/*.py / services/*.py）
- 框架/库版本升级

---

## 模板文件

创建新 Artifact 时，hooks 自动使用以下模板（位于 `hooks/templates/`）：
- `spec.md` — 项目规格模板
- `task.md` — 任务清单模板
- `implementation_plan.md` — 实施计划模板
- `walkthrough.md` — 工作记录模板
- `findings.md` — 调研记录模板

变更管理模板（位于 `skills/artifact-management/templates/`）：
- `change-implementation_plan.md` — 完整 Implementation Plan 模板

---

## 常见错误速查

| 错误格式 | 正确格式 | 说明 |
|---------|---------|------|
| `\| [/] \| CHG-... \|` | `- [/] CHG-...` | 表格 → checkbox |
| `✅ CHG-...` | `- [x] CHG-...` | emoji → checkbox |
| 索引无详情段落 | `### CHG-ID` 段落 | hook 守门 DENY |
| 双 `<!-- ARCHIVE -->` | 最终保留 1 个 | 归档中间态可接受，完成后须删除旧标记 |
| `## ARCHIVE` | `<!-- ARCHIVE -->` | 必须是 HTML 注释 |
| 详情写"见 docs/plans/" | 详情自包含 | hook DENY |
| findings 只写索引无详情 | 同时写索引+详情 | 后续 session 无法还原上下文 |
