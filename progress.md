# rbrain 开发进展

## 项目概述

rbrain 是 gbrain（TypeScript 知识库）的 Rust 移植版本。2026-06-14 起，项目定位从“面向学术研究的个人 AI 知识库 CLI”升级为：

```text
rbrain = 学术研究过程账本 + 证据核查器 + 改进建议引擎
Swiftide / rbrain-agent = 研究流程执行与调度
Python / R / DuckDB / 浏览器 = 具体计算和外部操作
```

详细技术方案见 [plan.md](./plan.md)。

---

## 当前规划基线（2026-06-14）

### 已确认方向

| 决策 | 状态 | 说明 |
|------|------|------|
| rbrain-hub 是同源上游 | ✅ 确认 | M1-M4 以“回迁 + 适配”为主，不从零重写 |
| 放弃近期 LanceDB 集成 | ✅ 确认 | 保留 SQLite + Tantivy + usearch；拒绝 rbrain-hub 的 LanceDB 路线 |
| 新定位：过程记录 / 结果核查 / 改进建议 | ✅ 确认 | rbrain 不做通用 agent runtime，不执行任意 Python/R/SQL |
| M0 schema 治理强制前置 | ✅ 确认 | 先固定 PageType / EdgeType / JSON schema / timeline 语义 |
| dream 与 agent 边界重划 | ✅ 确认 | dream 只做 housekeeping；生成性任务走 PipelineRunner / agent |
| Swiftide 作为首选流程层 | ✅ 确认 | M6 先 spike Swiftide；失败则退回 `tokio-cron-scheduler + PipelineRunner` |
| 200+ 期刊引文库场景 | ✅ 纳入 | 走独立 `citations_query`，不走 chunk-level RRF |

### 下一阶段任务

| 里程碑 | 状态 | 目标 |
|--------|------|------|
| M0 Schema 治理 + Timeline 重定义 | ✅ 完成首版 | 已落地类型枚举、轻量 frontmatter 校验、`schema_version`、结构化 timeline 兼容写入/渲染；timeline 已排除出索引和图谱抽取 |
| M1 检索与记录基础优化 | 🟨 进行中 | 已回迁 PR-C1 标题前缀 embedding、PR-C2 page cap、PR-C3 JSON 精确 tag filter；PR-C4/C5/PR-D 待做 |
| M2 研究过程记录 API | ✅ 完成首版 | 已落地 `create_research_run`、`get_research_protocol`、`register_input`、`record_artifact`、`record_finding`、`record_validation_report`、`record_action_item`、`provenance_of` 及 MCP stdio/HTTP 工具；CLI 是否暴露待定 |
| M3 结果核查与改进建议 | 🟨 进行中 | 已回迁/适配 evidence 基础框架、内置 research_run validators、`brain_validate_research_run`；完整 rbrain-hub validators 待继续回迁 |
| M4 Pipeline profiles | ⬜ 待做 | 回迁 `pipeline.rs` / `PipelineRunner`，新增 `rbrain pipeline run`，dream 退为 housekeeping |
| M5 期刊引文库支持 | ⬜ 待做 | `citation_record`、corpus、dedupe、独立 `citations_query`、brief、hotspot report |
| M6 Swiftide 流程层 | ⬜ 待做 | `crates/rbrain-agent`；Swiftide spike，通过则编排简报/热点/推荐，失败则自实现兜底 |

---

## 当前实现状态（2026-05-24）

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
| --mock-embed global flag | ✅ 完成 |
| search/query --type/--tag 过滤 | ✅ 完成 |
| graph-query depth-1 context 显示 | ✅ 完成 |
| links 多段 evidence 展示（passages） | ✅ 完成 |
| think 多语言 prompt（CJK/EN） | ✅ 完成 |
| Embed 进度条（indicatif） | ✅ 完成 |
| Doctor 增强（embedding 覆盖率/indegree/存储大小） | ✅ 完成 |
| RBRAIN_HOME 环境变量 + --brain-dir global flag | ✅ 完成 |
| rbrain put --content 内联写入 | ✅ 完成 |
| rbrain config show / config get | ✅ 完成 |
| rbrain health 别名（doctor） | ✅ 完成 |
| rbrain graph 别名（graph-query） + --type 别名 | ✅ 完成 |
| rbrain --version | ✅ 完成 |
| rbrain-research-cli skill（parallel to gbrain skill） | ✅ 完成 |
| MCP setup guide（Claude Code / Codex CLI / OpenCode） | ✅ 完成 |
| Dream cycle 自动化知识整理 | ✅ 完成 |
| Salience / Anomalies | ⬜ 待做 |

---

## 近期开发记录

### 2026-06-14 — M1 检索增强第一切片

**已完成**:

1. 回迁 PR-C1 标题前缀 embedding：
   - `chunk_and_embed_page()` 仍保存原始 chunk 文本。
   - embedding 输入临时加上 `[page.title]` 或 `[slug]` 前缀，提升短 chunk / 代词密集 chunk 的语境质量。
2. 回迁 PR-C2 page-level max-pooling：
   - 新增 `MAX_POOL_DEFAULT = 1` 与 `apply_page_cap()`。
   - `search_with_context()` / `search_with_context_filtered()` 多取候选后按页面限流，避免同一页面多个 chunk 挤占上下文。
3. 回迁 PR-C3 JSON 精确 tag 过滤：
   - 将 `p.tags LIKE '%tag%'` 改为 `EXISTS (SELECT 1 FROM json_each(p.tags) WHERE value = ?)`。
   - 避免 `ai` 误匹配 `fairness-ai` 等部分字符串。
4. 补齐 timeline 隔离语义：
   - `put_page()` 只从 `compiled_truth` 抽 wikilink，不再从 timeline 写图谱边。
   - `chunk_and_embed_page()` 只索引 / embedding `compiled_truth`。
   - `import_dir()` 语言检测只看 `compiled_truth`，不受 timeline 审计日志干扰。
5. dream extraction 的 LLM 调用失败或 JSON 解析失败时回退到 mock extraction，避免 housekeeping 因临时网络/API 问题整体中断。

**验证**:

- `cargo fmt`
- `cargo test -p rbrain-core`
- `cargo test -p rbrain-engine`
- `cargo test -p rbrain-cli -p rbrain-mcp --no-run`
- DeepSeek API 健康检查：读取 `/Users/hongyu/project/rbrain-test/config.toml`，`deepseek-v4-flash` 返回 HTTP 200 / `ok`。

**待完成**:

- PR-C4：`ExplainedHit` / RRF 解释输出。
- PR-C5：intent-aware backlink boost。
- PR-D：research model + provenance edge 枚举和显式写入 API。

### 2026-06-14 — M2 研究过程记录 API 第一切片

**已完成**:

1. 新增 engine 级研究记录方法：
   - `Engine::create_research_run()`：创建 `research_run` 页面，写入 `run_id`、`status=active`、`title`、`question`。
   - `Engine::record_artifact()`：创建 `artifact` 页面，并写入 `research_run --produces--> artifact`。
   - `Engine::record_finding()`：创建 `finding` 页面，并写入 `research_run --produces--> finding` 与 `finding --supports--> evidence/artifact`。
2. 这些方法全部复用 `put_page()` 和 `add_link()`：
   - frontmatter 继续走 M0 schema 校验。
   - edge type 继续走固定 provenance label。
   - 不新增表结构，仍以 SQLite pages/links 为事实源。
3. MCP stdio 新增工具：
   - `brain_create_research_run`
   - `brain_record_artifact`
   - `brain_record_finding`
   - `brain_provenance_of`
4. MCP HTTP 同步补齐 dispatch 与 `tools/list` schema，避免 stdio/HTTP 能力不一致。
5. 新增 `Engine::provenance_of()`：围绕 finding / artifact / run 按 `produces`、`supports`、`derived_from`、`uses_*`、`computed_by`、`validates` 等研究证据边反查局部 provenance graph。
6. 新增集成测试 `test_research_recording_api_writes_pages_and_provenance`，验证页面类型、`produces` / `supports` 边和 provenance 反查。

**验证**:

- `cargo test -p rbrain-engine`
- `cargo test -p rbrain-cli -p rbrain-mcp --no-run`

**待完成**:

- CLI 子命令是否需要暴露这组研究记录 API，待根据实际使用方式决定。

### 2026-06-14 — M2.5 研究过程记录 API 补齐

**已完成**:

1. 新增 `Engine::register_input()`：
   - 支持 `dataset` / `literature_corpus` / `citation_record` / `source` / `method_note` / `research_memo`。
   - 按输入类型自动建立 `uses_dataset` / `uses_corpus` / `cites` / `uses_method` / `references` provenance 边。
2. 新增 `Engine::record_validation_report()`：
   - 创建 `validation_report` 页面。
   - 写入 `validator`、`status`、可选 `suggested_actions`。
   - 建立 `research_run --produces--> validation_report` 与 `validation_report --validates--> target`。
3. 新增 `Engine::record_action_item()`：
   - 创建 `action_item` 页面。
   - 写入 `action_kind`、`status`。
   - 建立 `research_run --produces--> action_item` 与 `action_item --recommends--> target`。
4. MCP stdio / HTTP 同步新增工具：
   - `brain_register_input`
   - `brain_record_validation_report`
   - `brain_record_action_item`
   - `brain_get_research_protocol`
5. schema 同步：
   - `PageType` 增加 `validation_report` / `action_item`。
   - 新增 `schemas/validation_report.json` 与 `schemas/action_item.json`。
   - `schemas/research_run.json` 从 `task_type` 收敛为 `run_kind`，补入 `research_design`。
6. 扩展集成测试 `test_research_recording_api_writes_pages_and_provenance`，覆盖输入、核查报告、行动项和 provenance 反查。

**验证**:

- `cargo test -p rbrain-core`
- `cargo test -p rbrain-engine test_research_recording_api_writes_pages_and_provenance`
- `cargo test -p rbrain-mcp --no-run`

**待完成**:

- CLI 是否暴露这组研究记录 API，仍待根据实际使用方式决定。
- M3 validator framework 尚未回迁；当前只是具备保存 validator 输出的结构化接口。

### 2026-06-14 — M2.6 research protocol 读取入口

**已完成**:

1. 新增 `Engine::get_research_protocol()`：
   - 读取一个 `research_run`。
   - 按 provenance 边聚合 `inputs`、`artifacts`、`findings`、`validation_reports`、`action_items`、`related`。
   - 展开 `validation_report --validates--> target` 与 `action_item --recommends--> target` 二级边。
2. MCP stdio / HTTP 同步新增 `brain_get_research_protocol`。
3. 扩展集成测试，验证 protocol 视图能返回输入、产物、发现、核查报告、行动项和二级关系边。

**用途**:

- M3 validator 读取 run 上下文。
- M4 PipelineRunner 判断阶段输入/输出是否齐备。
- M6 Swiftide workflow 以 research_run 为恢复点和审计容器。

**验证**:

- `cargo test -p rbrain-engine test_research_recording_api_writes_pages_and_provenance`
- `cargo test -p rbrain-mcp --no-run`

### 2026-06-14 — M3 结果核查与改进建议第一切片

**已完成**:

1. 新增 `rbrain-engine::evidence` 模块：
   - `ValidatorStatus`
   - `ValidatorResult`
   - `SuggestedAction`
   - 基础 validators
2. 新增内置 research run validators：
   - `research_run_has_input`
   - `produced_artifact_exists`
   - `artifact_hash_present`
   - `finding_has_supporting_evidence`
3. 新增 `Engine::validate_research_run()`：
   - 读取并确认目标页面是 `research_run`。
   - 运行内置 validators。
   - 将每个 validator 结果写回 `validation_report`。
   - 将 `suggested_actions` 转化为 `action_item`。
4. MCP stdio / HTTP 同步新增：
   - `brain_validate_research_run`
5. 集成测试扩展：
   - 验证 validator 返回 pass/warn。
   - 验证自动生成的 `validation_report` 与 `action_item` 进入 research protocol。

**验证**:

- `cargo test -p rbrain-engine test_research_recording_api_writes_pages_and_provenance`
- `cargo test -p rbrain-mcp --no-run`

**待完成**:

- 继续回迁 rbrain-hub `evidence/*` 中更完整的 citation / literature-review validators。
- 将 validator 集合按 `run_kind` 分组，而不是所有 research_run 都跑同一组基础 validators。
- M4 PipelineRunner 后，validator 应成为每个 pipeline stage 的质量门。

### 2026-06-14 — M0 Schema 治理第一切片

**已完成**:

1. 新增 `rbrain-core::schema`：
   - `PageType`：覆盖现有页面类型、研究过程类型、文献综述类型、期刊引文产物类型，并保留 `Unknown(String)` 兼容旧数据。
   - `EdgeType`：覆盖通用边和研究 provenance 边，并保留 `Unknown(String)`。
   - `TimelineSource` / `TimelineEntry`：定义 `user | validator | agent | script | llm` 来源和结构化 payload。
   - `TimelineEntry::parse_compat()`：兼容旧 timeline 字符串，后续可逐步迁移到 JSON 数组。
2. 新增轻量 frontmatter schema 校验：
   - 旧类型（`note` / `wiki` / `book` / `concept` 等）保持宽松，避免破坏现有知识库。
   - 新结构化类型（如 `citation_record` / `research_run` / `dataset` / `artifact` / `brief`）检查必要字段。
3. 新增 `schemas/*.json` 作为外部 schema 约定：
   - `citation_record.json`
   - `research_run.json`
   - `dataset.json`
   - `brief.json`
   - `artifact.json`
4. 新增 migration `0010_schema_version.sql`：
   - `pages.schema_version INTEGER NOT NULL DEFAULT 1`
   - `idx_pages_schema_version`
5. 写入边界接入校验：
   - `Engine::put_page()` 写入前校验 page_type/frontmatter。
   - `Engine::add_link()` 拒绝空 `edge_type`。
   - `put_page()` 显式写入 `schema_version = 1`。
6. timeline 兼容写入与渲染：
   - `Engine::add_timeline_entry()` 写入结构化 `TimelineEntry { source=user, kind=dated_event }`。
   - `Engine::add_take()` 写入结构化 `TimelineEntry { source=user, kind=take }`。
   - CLI `get` / `takes` 使用 `TimelineEntry::render_compat()` / `take_lines_compat()` 保持旧式文本展示。
   - MCP stdio / HTTP 的 `brain_get` 返回渲染后的 timeline，避免暴露 JSON 审计数组。

**验证**:

- `cargo test -p rbrain-core`
- `cargo test -p rbrain-engine test_structured_frontmatter_validation`
- `cargo test -p rbrain-engine`
- `cargo test -p rbrain-engine --test integration test_timeline_entries_are_structured_but_render_compatibly`
- `cargo test -p rbrain-cli -p rbrain-mcp --no-run`

**待完成**:

- 仍未把 `Page.timeline: String` 全面迁移为 `Vec<TimelineEntry>`；当前是 String 存储 JSON 数组 + 兼容解析/渲染层。
- 尚未引入完整 `jsonschema` crate runtime validator；当前使用内置轻量校验，JSON schema 文件先作为约定来源。
- 尚未冻结 `infer_edge_type` 的自由推断路径；M2 显式记录 API 落地后再收紧。

---

## 提交历史

### Commit 11 — 语言检测优化、多证据片段支持与梦境循环简化版实现

**日期**: 2026-05-24

**新增功能与改进**:
1. **语言检测机制优化**：
   - 将散落在各模块的语言检测逻辑统一抽离到最底层的 `rbrain-core` 中的 `Language::detect`。
   - 引入了高精度的 CJK 启发式算法（韩文/日文假名/繁简分流），修复了简体中文有时会被误判为繁体中文或日语的问题。
2. **中文停用词过滤**：
   - 在 `rbrain-search` 的 `keyword_index.rs` 中增加了对高频中文停用词（如“的”、“了”、“在”、“是”、“我”等）的过滤拦截，分词后的关键词匹配更加纯粹，提升了中文检索精度。
3. **`links` 数据库 Schema 扩展 (多证据片段支持)**：
   - 编写了数据库迁移 `0008_extend_links_chunk_id.sql`，在 SQLite 数据库中为 `links` 表增加了 `chunk_id` 列，支持在同一源页面的不同段落中多次链接至同一目标页面，且各自保留独立的上下文证据。
4. **Dream Cycle (梦境循环) 简化版实现**：
   - 编写了数据库迁移 `0009_dream_metadata.sql`，在 SQLite 数据库中创建 `dream_metadata` 表，记录每个页面的提取时间戳和内容 hash，实现增量知识提取。
   - 在 `rbrain-engine` 中实现了 Dream Cycle 完整流水线（Linting、Embedding 缺失页、Extracting 实体与事件、Synthesizing 标签文献综述），在 `rbrain-cli` 中新增了 `dream` 子命令并支持 `--stage` 选项。
   - 适配了 Mock 提取和 Synthesis 合成逻辑，使无 API 密钥的本地测试/集成测试能够 100% 确定性离线运行。
5. **Markdown Frontmatter 解析修复**：
   - 修复了 `sync` 和 `import` 命令在解析 Markdown 文件 Frontmatter 后丢失 `tags` 与 `title` 的问题。现在它们能够正确载入 Page 结构并写入数据库，使标签维度的 Dream Synthesis 可以完美识别并执行文献聚合。

---

### Commit 10 — gbrain-research-cli 功能对齐

**日期**: 2026-05-21

**背景**: 对照 `research-project-template/skills/gbrain-research-cli/SKILL.md` 进行功能审查，发现 rbrain CLI 在以下方面与 gbrain 存在差距，全部在本次 commit 补齐。

**新增 CLI 功能**:

| 功能 | 说明 |
|------|------|
| `RBRAIN_HOME=/path rbrain stats` | 环境变量设置 brain 路径，等价于 gbrain 的 `GBRAIN_HOME=` |
| `rbrain --brain-dir /path stats` | 全局 CLI 标志，优先级高于 RBRAIN_HOME |
| `rbrain put <slug> --content "..."` | 内联写入 Markdown，无需 stdin 或文件 |
| `rbrain config show` | 显示当前配置（API key 脱敏） |
| `rbrain config get <key>` | 获取单个配置项（支持 deepseek.model、qwen.model 等） |
| `rbrain health [--fix]` | doctor 别名 |
| `rbrain graph <slug> [--type <edge>] [--depth N]` | graph-query 别名，`--type` 是 `--edge-type` 的别名 |
| `rbrain --version` / `rbrain -V` | 版本号 |

**新增 skill 文件**:
- `research-project-template/skills/rbrain-research-cli/SKILL.md` — 完整工作流文档
- `research-project-template/skills/rbrain-research-cli/agents/openai.yaml` — Codex 接口定义
- `research-project-template/skills/rbrain-research-cli/scripts/bootstrap-rbrain-cli.sh` — 安装脚本
- `research-project-template/skills/rbrain-research-cli/references/project-profile-example.md` — 项目配置示例

**核心文件改动**:
- `crates/rbrain-core/src/config.rs` — 增加 `RBRAIN_HOME` 环境变量优先级处理；新增 `load_with_brain_dir()` 方法
- `crates/rbrain-cli/src/main.rs` — 全局 `--brain-dir` 标志；`load_config!()` 宏统一配置加载；新增 `Put.content`、`Graph`、`Health`、`Config` subcommand 及 `ConfigAction`

---

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

3. **Dream cycle 仍是 hardcoded 流程**：自动维护（lint→embed→extract→synthesize）已经实现，但还不是 `plan.md` 中规划的 TOML-configurable Pipeline profile。

4. **缺少研究过程账本模型**：尚未实现 `research_run`、`dataset`、`artifact`、`finding`、`limitation` 等结构化记录 API。

5. **缺少结果核查层**：尚未实现 validator framework、provenance validator、citation/brief validator 和结构化 `suggested_actions`。

6. **缺少期刊引文库能力**：尚未实现 `citation_record`、`literature_corpus`、引文去重、定期简报、热点分析报告和用户推送流程。

7. **缺少 schema 治理层**：尚未实现 `PageType` / `EdgeType`、frontmatter JSON schema、`schema_version` 和显式边类型约束。

8. **timeline 仍是字符串字段**：尚未重构为 `Vec<TimelineEntry>`，也还没有按 `source=user|validator|agent|script|llm` 区分审计日志来源。

9. **缺少 Swiftide 流程层**：尚未实现可选 `rbrain-agent` crate；当前 rbrain 只提供 CLI/MCP/Engine 能力。

---

## API Keys

- DeepSeek: `deepseek.api_key` in `~/.rbrain/config.toml`（用于 generate/think/query expand）
- DashScope Qwen: `qwen.api_key`（用于 embedding，中国区 endpoint）
- `--mock-embed` 全局标志：完全离线测试，无需任何 key

---

## 下一步（按优先级）

1. M0 schema 治理：新增 `rbrain-core/src/schema.rs`，定义 `PageType` / `EdgeType`；建立 `schemas/*.json`；在写入入口做校验。
2. M0 timeline 重定义：设计 `TimelineEntry` 并规划兼容迁移，确保 timeline 永不进入 embedding / indexing / 下游 LLM prompt。
3. M1 回迁检索增强：按 PR-C1~C5 小步回迁标题前缀 embedding、ExplainedHit、page cap、JSON tag filter、intent-aware boost。
4. M1/PR-D 回迁 research model 与 provenance edge 枚举，去除 rbrain-hub 的 TenantContext。
5. M2 实现研究过程记录 API，所有输入必须走 M0 schema 和显式 EdgeType。
6. M3 回迁 validator framework，validator 输出 append 到 timeline（source=validator）。
7. M4 回迁 PipelineRunner，新增 `rbrain pipeline run`，`dream --profile` 保留为别名。
8. M5 建立期刊引文库独立路径：`citations_query`、CSV/JSON 导入、dedupe、brief/hotspot 生成。
9. M6 做 Swiftide 0.5-1 天 spike，再决定 `rbrain-agent` 是否基于 Swiftide；失败则走自实现兜底。
