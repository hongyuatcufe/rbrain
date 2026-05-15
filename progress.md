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
| MCP server（17 个工具，stdio + HTTP） | ✅ 完成 |
| Claude Code slash command（/rbrain, /rbrain-edu） | ✅ 完成 |
| Signal-detector 行为（slash command 内自动捕捉概念） | ✅ 完成 |
| --mock-embed 全局标志 | ✅ 完成 |
| search/query --type/--tag 过滤 | ✅ 完成 |
| graph-query depth-1 context 显示 | ✅ 完成 |
| links 多段 evidence 展示（passages） | ✅ 完成 |
| think 多语言 prompt（CJK/EN） | ✅ 完成 |
| Embed 进度条（indicatif） | ✅ 完成 |
| Doctor 增强（embedding 覆盖率/indegree/存储大小） | ✅ 完成 |
| Salience / Anomalies / Dream cycle | ⬜ 待做 |

---

## 提交历史

### Commit 9 — MCP output schema 修复

**日期**: 2026-05-15

**问题**: rmcp 1.7.0 要求所有 tool output schema 根类型必须为 `object`，返回 `Vec<T>`、`Option<T>`、`String` 的工具导致服务器启动时 panic，MCP 完全不可用。

**修复**: 为所有受影响的工具定义 wrapper struct：
- `ChunkList { results: Vec<ChunkResult> }` → brain_query
- `PageList { results: Vec<PageSummary> }` → brain_list
- `GraphList { results: Vec<GraphEdge> }` → brain_graph
- `LinkList { results: Vec<LinkRef> }` → brain_backlinks + brain_outlinks
- `SlugList { slugs: Vec<String> }` → brain_orphans
- `GetResult { found: bool, page: Option<PageResult> }` → brain_get
- `ThinkResult { reasoning: String }` → brain_think

---

### Commit 8 — MCP 工具扩展 + Signal-Detector 行为

**日期**: 2026-05-15

**MCP 新增工具（lib.rs + http.rs）**:

| 工具 | 对应 engine 方法 | 说明 |
|------|----------------|------|
| `brain_think` | `engine.think()` | 深度推理，返回核心观点/张力/工作判断/开放问题 |
| `brain_add_timeline_entry` | `engine.add_timeline_entry()` | 给任意页面追加带日期的事件记录 |
| `brain_add_tag` | `engine.add_tag()` | 通过 MCP 给页面加标签 |
| `brain_remove_tag` | `engine.remove_tag()` | 通过 MCP 移除标签 |
| `brain_outlinks` | `engine.outlinks()` | 查出链（与 brain_backlinks 对称） |

**Signal-Detector 行为层**:
- `/rbrain` slash command 追加 Signal Detection section：检测到原创思考 → `brain_put` 到 `concepts/`，检测到人名 → `brain_add_timeline_entry` 到 `figures/`，建立后 → `brain_link` 关联来源 chunk
- `/rbrain-edu` 同样追加，路径改为 `research/concepts/`、`research/figures/`、`research/periods/`
- 每次会话结束输出 signal log

**背景**: gbrain 的核心工作流是 signal-detector skill 在每条消息后台自动捕捉原创观点和概念引用并写入 brain。rbrain 现在通过 slash command 行为指令实现相同效果。

---

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

### Commit 5 — Embed 进度条 + Doctor 增强 + import_dir 修复
**日期**: 2026-05-15

**embed 进度条**:
- `embed --all` 和 `embed --stale` 使用 indicatif 显示 `[===>] 42/150 pages + 当前 slug`

**doctor 增强**:
- 各类型 embedding 覆盖率（带 bar chart）
- top-5 高 indegree 页面
- 存储大小：SQLite / Vector store / Tantivy

**engine 新方法**:
- `link_count()` / `top_pages_by_indegree(n)` / `embedding_coverage_by_type()`

**import_dir 修复**:
- 传入单个文件路径时，`strip_prefix` 用父目录而非文件本身，避免 slug 为空

---

### Commit 4 — Phase 2 Easy batch + bug fixes
**日期**: 2026-05-15

**新增命令**:
- `rbrain links <slug>` — 查出链，显示 edge type + evidence context
- `rbrain tag/untag/tags <slug> [tag]` — 标签管理
- `rbrain export --dir <path> --format md|json` — 导出所有页面
- `rbrain lint` — 质量检查：缺 title、未嵌入页、孤立页、broken links
- `rbrain embed --stale` — 只重嵌未嵌入/过时的页面
- `rbrain sync --embed` — sync + 自动 re-embed 改动页面

**Bug 修复**:
1. `add_timeline_entry` / `add_take` 调用 `put_page` 会清空 explicit links → 改为直接 SQL UPDATE
2. `put` 命令不解析 frontmatter，导致 type/title/tags 丢失 → 先调 `MarkdownParser::parse()`
3. `add_link` 对同一 (source, target, type) 的第二次调用会覆盖 context → 改为追加（`---` 分隔）
4. `rbrain backlinks` 空结果静默 → 现在明确提示 `(none)`

---

### Commit 3 — Timeline, Take, Think + chunk 锚定 evidence
**日期**: 2026-05-14/15

- `rbrain timeline/take/takes/think` CLI 命令
- `rbrain link --from-chunk <id>` — 自动从 chunk 读取 context 作为 evidence
- search/query 输出显示 `[chunk:ID]`
- MCP: brain_link（含 chunk_id）、brain_unlink、brain_orphans
- `~/.claude/commands/rbrain.md` slash command 创建

---

### Commit 2 — Bug fixes（集成测试发现）
**日期**: 2026-05-14

- `import_dir` 只调 put_page，忘了调 chunk_and_embed → 修复
- `add_link` INSERT 缺 created_at → 修复
- indegree trigger COUNT(*) 无 GROUP BY 返回 NULL → 修复（migration 0007）
- wikilink 提取把图片（.jpeg/.png）当页面链接 → 过滤掉
- import slug 有尾随空格 → `.trim()`

---

### Commit 1 — Link, Unlink, Orphans + Graph query
**日期**: 2026-05-14

- `rbrain link/unlink/orphans` CLI
- Graph query 支持 depth + direction
- Backlinks 显示 context

---

### Commit 0 — Initial release v0.1.0
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

1. **links 唯一约束**：`UNIQUE(source_slug, target_slug, edge_type)` — 同一来源同类型 link 改为 context 追加（`---` 分隔），仍是一行记录。如需完全独立的多条 link，需迁移 schema 加 `chunk_id` 列。

2. **旧 evidence context 无 chunk ID 前缀**：Commit 6 之前通过 `--from-chunk` 存储的 context 没有 `[chunk:ID]` 前缀，新增的才有。

3. **Dream cycle**：自动维护（lint→embed→extract→synthesize）尚未实现。

4. **language detection 不准**：简体中文文档有时被检测为 `zh-hant`，影响分词器选择。非阻塞性问题。

---

## API Keys

- DeepSeek: `deepseek.api_key` in `~/.rbrain/config.toml`（用于 generate/think/query expand）
- DashScope Qwen: `qwen.api_key`（用于 embedding，中国区 endpoint）
- `--mock-embed` 全局标志：完全离线测试，无需任何 key

---

## 下一步（按优先级）

1. `rbrain doctor` 增强（embedding 覆盖率、最大 indegree、tantivy index 大小）→ ✅ 已完成
2. Salience 排序（情感权重 × 时间衰减）
3. Dream cycle 简化版（4 阶段：lint→embed--stale→extract--all→generate stale synthesis）
4. links schema 扩展（加 `chunk_id` 列支持真正独立的多条 evidence link）
