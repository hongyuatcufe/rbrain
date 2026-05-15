# rbrain 开发进展

## 项目概述

rbrain 是 gbrain（TypeScript 知识库）的 Rust 移植版本，定位为面向学术研究的个人 AI 知识库 CLI。

---

## 当前状态（2026-05-15）

| 功能领域 | 状态 |
|---------|------|
| 页面 CRUD（put/get/delete/list） | ✅ 完成 |
| Bulk import（import_dir） | ✅ 完成 |
| CJK 分词 keyword search（Tantivy + lindera） | ✅ 完成 |
| Hybrid search（向量 + BM25 + RRF） | ✅ 完成 |
| Graph links（link/backlinks/links/graph-query/orphans） | ✅ 完成 |
| Chunk 锚定 evidence link（--from-chunk） | ✅ 完成 |
| Timeline / Take / Think | ✅ 完成 |
| Tag 管理（tag/untag/tags） | ✅ 完成 |
| Lint / Export / embed --stale | ✅ 完成 |
| MCP server（brain_query/link/unlink/orphans） | ✅ 完成 |
| Claude Code slash command（/rbrain, /rbrain-edu） | ✅ 完成 |
| --mock-embed 全局标志 | ✅ 完成 |
| search/query --type/--tag 过滤 | ✅ 完成 |
| graph-query depth-1 context 显示 | ✅ 完成 |
| links 多段 evidence 展示（passages） | ✅ 完成 |
| think 多语言 prompt（CJK/EN） | ✅ 完成 |
| Embed 进度条（indicatif） | ⬜ 待做 |
| Salience / Anomalies / Dream cycle | ⬜ 待做 |

---

## 提交历史

### Commit 7 — query --type/--tag crowding-out bug fix
**日期**: 2026-05-15

**问题**: `query "因材施教" --type concept --limit 3` 返回空结果，`--limit 5` 才有结果。

**根因**: `search_with_context_filtered` 先跑全局 hybrid search 取 top-k 候选，再用 SQL 过滤类型。当 brain 里书籍 chunk 数量多、BM25 分值高时，concept chunk（分值 7.2）会被 45 个更高分的 book chunk 挤出 top-k 候选池，SQL 过滤阶段找不到它。

**修复**: `search_with_context_filtered` 额外跑一次 `keyword_search_filtered(page_type, tag)`，把过滤后命中的 chunk 合并进候选池，确保目标类型的页面始终参与 SQL 过滤。

---

### Commit 6 — 研究工作流核心缺口修复
**日期**: 2026-05-15

**Fix 1 — search/query --type/--tag 过滤**:
- `search "孔子" --type book` 只返回书籍，不污染自写的 concept/note 结果
- `query "因材施教" --type concept` 返回概念页（见 Commit 7 的 crowding-out 修复）

**Fix 2 — --from-chunk 存储带 chunk ID 前缀**:
- 新 evidence link 存储格式：`[chunk:59755] 孔子的因材施教原则...`
- `rbrain links` 多段展示：`Context (3 passages): [1] ... [2] ... [3] ...`

**Fix 3 — graph-query depth-1 显示 context**:
- `rbrain graph-query <slug> --depth 1` 每条 depth-1 边显示 evidence 摘要
- 修复 SQL 混用 `?1` 和 `?` 导致 sqlx 绑定错位的 bug

**Fix 4 — think 多语言 prompt**:
- CJK 语料：中文推理 prompt（核心观点 / 张力与矛盾 / 工作判断 / 开放问题）
- 英文语料：English prompt（Core Claims / Tensions & Gaps / Working Judgment / Open Questions）

---

### Commit 5 — Phase 2 Easy batch + bug fixes
**日期**: 2026-05-15

**新增命令**:
- `rbrain links <slug>` — 查出链（与 backlinks 对称），显示 edge type + evidence context
- `rbrain tag/untag/tags <slug> [tag]` — 标签管理
- `rbrain export --dir <path> --format md|json` — 导出所有页面
- `rbrain lint` — 质量检查：缺 title、未嵌入页、孤立页、broken links
- `rbrain embed --stale` — 只重嵌未嵌入/过时的页面
- `rbrain sync --embed` — sync + 自动 re-embed 改动页面

**Bug 修复**:
1. `add_timeline_entry` / `add_take` 调用 `put_page` 会清空 explicit links → 改为直接 SQL UPDATE，links 不受影响
2. `put` 命令不解析 frontmatter，导致 type/title/tags 丢失 → 先调 `MarkdownParser::parse()` 再建 page
3. `add_link` 对同一 (source, target, type) 的第二次调用会覆盖 context → 改为追加（多段原文用 `---` 分隔）
4. `rbrain backlinks` 空结果静默 → 现在明确提示 `(none)`

**新增 engine 方法**:
- `outlinks(slug)` — 查出链
- `add_tag(slug, tag)` / `remove_tag(slug, tag)` / `list_stale_pages()` — 标签和 stale 管理
- `lint()` — 返回 (level, slug, message) 警告列表
- `export_pages(dir, json)` — 批量导出

**Skill 重构**:
- `~/.claude/commands/rbrain.md` → 通用版（领域无关示例，标准路径 notes/concepts/questions/）
- `~/.claude/commands/rbrain-edu.md` → 新建，教育史专用（中文示例，research/figures/periods/ 路径）

---

### Commit 4 — Timeline, Take, Think + chunk 锚定 evidence
**日期**: 2026-05-14/15

- `rbrain timeline/take/takes/think` CLI 命令
- `rbrain link --from-chunk <id>` — 自动从 chunk 读取 context 作为 evidence
- search/query 输出显示 `[chunk:ID]`
- MCP: brain_link（含 chunk_id）、brain_unlink、brain_orphans
- `~/.claude/commands/rbrain.md` slash command 创建

---

### Commit 3 — Bug fixes（集成测试发现）
**日期**: 2026-05-14

- `import_dir` 只调 put_page，忘了调 chunk_and_embed → 修复
- `add_link` INSERT 缺 created_at → 修复
- indegree trigger COUNT(*) 无 GROUP BY 返回 NULL → 修复（migration 0007）
- wikilink 提取把图片（.jpeg/.png）当页面链接 → 过滤掉
- import slug 有尾随空格 → `.trim()`

---

### Commit 2 — Link, Unlink, Orphans
**日期**: 2026-05-14

- `rbrain link/unlink/orphans` CLI
- Graph query 支持 depth + direction
- Backlinks 显示 context

---

### Commit 1 — Initial release v0.1.0
**日期**: 2026-05-14

- 8 个 crate 架构
- 核心 CRUD、import、embed、keyword search、hybrid search
- Tantivy lazy writer（Option<IndexWriter>，按需创建，commit 后释放锁）
- lindera CJK 预分词（zh/ja/ko）
- MockEmbedder（--mock-embed 全局标志）
- MCP server（stdio + HTTP）
- Job queue + Worker

---

## 已知限制

1. **links 唯一约束**：`UNIQUE(source_slug, target_slug, edge_type)` — 同一本书同类型 link 已改为 context 追加（`---` 分隔），仍是一行记录。如需完全独立的多条 link，需迁移 schema 加 `chunk_id` 列。

2. **旧 evidence context 无 chunk ID 前缀**：Commit 6 之前通过 `--from-chunk` 存储的 context 没有 `[chunk:ID]` 前缀，新增的才有。

3. **embed 进度条**：`embed --all` 只打印行文本，无 indicatif 进度条。

4. **Dream cycle**：自动维护（lint→embed→extract→synthesize）尚未实现。

---

## API Keys

- DeepSeek: `deepseek.api_key` in `~/.rbrain/config.toml`（用于 generate/think/query expand）
- DashScope Qwen: `qwen.api_key`（用于 embedding，中国区 endpoint）
- `--mock-embed` 全局标志：完全离线测试，无需任何 key

---

## 下一步（按优先级）

1. embed 进度条（indicatif，~1h）
2. `rbrain doctor` 增强（embedding 覆盖率、最大 indegree、tantivy index 大小）
3. Salience 排序（情感权重 × 时间衰减）
4. Dream cycle 简化版（4 阶段：lint→embed--stale→extract--all→generate stale synthesis）
