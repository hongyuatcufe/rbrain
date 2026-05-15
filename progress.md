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
| Embed 进度条（indicatif） | ⬜ 待做 |
| Salience / Anomalies / Dream cycle | ⬜ 待做 |

---

## 提交历史

### Commit 5（本次）— Phase 2 Easy batch + bug fixes
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

1. **links 唯一约束**：`UNIQUE(source_slug, target_slug, edge_type)` — 同一本书同类型 link 已改为 context 追加，但仍是一行记录。如需完全独立的多条 link，需迁移 schema 加 `chunk_id` 列。

2. **embed 进度条**：`embed --all` 只打印行文本，无 indicatif 进度条。

3. **sync --embed 不自动触发**：sync 发现改动后需要手动加 `--embed` 标志才会 re-embed。

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
