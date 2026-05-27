---
{}
---
# rbrain 开发进展

## 项目概述

rbrain 是 gbrain（TypeScript 知识库）的 Rust 移植版本，定位为面向学术研究的个人 AI 知识库 CLI。

---

## 2026-05-25 — Bug 修复 & Dream Cycle 完善

### Bug 修复（6 commits）

**语言检测**
- 修复 `Language::detect()` 误判日语/韩语含汉字文本为中文的问题（错误的 Jpn/Kor → ZhHant/ZhHans 强制覆盖）
- 修复 CLI `rbrain get` 用 `{:?}` debug 格式显示语言的问题，改为正确的 Display 输出

**字符串安全**
- 修复 `clean_json()` 在只有单个 ` ``` ` 标记时的 `s[3..0]` slice panic
- 修复 `dream_synthesize` 和 `dream_extract` 中对 CJK 文本的 1000-byte 截断 panic（改为 char-boundary 安全截断）

**Frontmatter 写入**
- 修复 `Page::new()` 创建页面时 frontmatter 为空 `{}` 的问题：`put_page_inner()` 现在在写文件前将 `page_type`、`title`、`tags` 合并进 frontmatter，消除 `---\n{}\n---` 写入
- `generate --save` 和 `think --save`：从内容提取 H1 作为 title，调用 `Language::detect()` 设置语言

**其他**
- 修复 `add_take()` DB 先于文件写入导致状态不一致的顺序问题
- 修复 HTTP `brain_link` 调用缺少 `chunk_id` 参数导致的编译错误
- 修复 MCP `brain_outlinks` 返回字段名从 `source_slug` 错误命名为 `page_slug`

### Dream Cycle 功能改进

**Dream Extract — Figure 描述质量**
- 更新 LLM system prompt，明确禁止"本文""本文作者""该文作者"等模糊引用
- 要求 LLM 描述人物的真实机构/职位/研究领域，并在 user message 中传入源文章 slug 和标题
- 对测试语料（57 篇教育学文献）重新跑 extract，生成 115 个 figure 页面，质量明显改善

**Dream Synthesize — 改为概念聚类合成（方案 B）**
- 原逻辑：按 tag 分组合成文献综述，依赖源文章手动打 tag，实际无法触发
- 新逻辑：遍历所有 concept 页面，通过 backlinks 找到引用该概念的源文章，对有 3+ 篇源文章的概念自动生成 synthesis 页面
- Synthesis 页面保存到 `synthesis/<concept-slug>`，类型为 `synthesis`，自动建立 `→ concept（develops）` 和 `→ 源文章（evidence）` 链接
- 测试结果：从 57 篇教育学文献中自动生成 **17 个 synthesis 页面**，核心概念"中国教育学自主知识体系"聚合了 28 篇源文章，内容有结构化分析和正确 wikilink 引用

### 测试数据状态（/Users/hongyu/project/rbrain-test）

| 指标 | 数值 |
|------|------|
| 总页面数 | 464 |
| 源文章（note） | 56 |
| 概念（concept） | 273 |
| 人物（figure） | 115 |
| 综合分析（synthesis） | 17 |
| 参考文献（wiki） | 2 |
| 嵌入覆盖率 | 100% |
| 语言检测（zh-hans） | 231 页 |

---

## 2026-05-26 — 引用质量体系 & 关键 Bug 修复

### 新功能：`rbrain audit`

**目标**：综述完成后自动检验引用规范性。

**CLI**：`rbrain audit <slug> [--fix]`

**检查项**：

| 级别 | 类型 | 描述 |
|------|------|------|
| ERROR | citation_type | 引用了 draft/synthesis/wiki 系统页而非 raw 原始文献 |
| WARN | bib_duplicate | 参考文献列表中存在重复条目（同一 slug 出现多次） |
| WARN | bib_orphan | 参考文献条目在正文中没有对应 `[[slug]]` 引用 |
| INFO | bib_missing | 正文引用了 raw/note 页但未出现在参考文献列表 |

**`--fix` 模式**：自动删除重复和游离条目，重新编号，调用 `put_page` 保存。**不**自动替换 draft/synthesis 引用（非确定性）。

**`--suggest`（内置默认）**：ERROR 每条自动附带候选替换来源：
1. 先查被引 synthesis 页的直接出链中 `page_type IN ('raw','note')` 的目标
2. 若空，走两跳：synthesis → concept → raw/note（修正了错误的查询方向：应取出链 target，不是 source）

---

### Bug 修复（今日）

**Bug 1：`to_canonical` 缺少 timeline 分隔符（根因）**

- **问题**：`to_canonical` 写 .md 文件时，compiled_truth 和 timeline 之间没有 `\n---\n`，只有 `\n\n`。`rbrain sync` 重读文件时 `split_body` 找不到分隔符，把 timeline 并入 compiled_truth，导致 timeline 内容被当作正文 chunk 进向量索引。
- **修复**：`crates/rbrain-core/src/markdown.rs` — `to_canonical` 有 timeline 时改为 `format!("---\n{}\n---\n{}\n\n---\n{}\n", ...)` ，无 timeline 时去掉多余的空行尾。
- **新增测试**：`test_to_canonical_round_trip_with_timeline`，`test_to_canonical_no_timeline`
- **数据修复**：5 个 note 页面的 timeline 已被 sync 合并进 compiled_truth（1056 → 5 页确认），Python 脚本拆分回 DB，重写 .md 文件，删旧 chunk 重嵌入（2615 → 2596 chunks）

**Bug 2：think/generate 未过滤 synthesis/wiki 类型**

- **问题**：检索上下文只过滤 `draft`，synthesis 和 wiki（均为 LLM 生成页）仍进入 think 的检索池，导致综述引用 synthesis 二次摘要而非 raw 原文。
- **修复**：`engine.rs` 两处（`dream_think`、`dream_generate_wiki`）改为 `.filter(|c| !matches!(c.page_type.as_str(), "draft" | "synthesis" | "wiki"))`。

**Bug 3：timeline chunk 污染向量索引**

- **问题**：`chunk_and_embed_page` 对 `page.timeline` 单独 chunk 并嵌入。dream extract 把 LLM 生成的 timeline 摘要追加到 raw 文章，被 think 检索并引用（"标志着…实证研究的开端"等虚假原文）。
- **修复**：删除 timeline_chunks 分支，只对 `page.compiled_truth` chunk。配合 Bug 1 修复，timeline 内容不再进入索引。

**Bug 4：`is_compiled_truth` 错误翻转**

- **问题**：chunker 内部遇到 `---` 分隔线会把 `is_compiled_truth` 翻转为 false。LLM 生成的 draft 页和 concept 页正文中包含 `---`（markdown HR），导致部分 chunk 标记为非正文。
- **修复**：`chunk_and_embed_page` 中对 compiled_truth_chunks 硬编码 `let is_compiled_truth: i32 = 1`。传入 chunker 的已是 compiled_truth，任何内部 `---` 均不应翻转标志。

**Bug 5（本次代码审查发现）：`duplicate_indices` 死代码**

- **问题**：Check 3 中有 `duplicate_indices.insert(_num.saturating_sub(0))`，`saturating_sub(0)` = no-op，且 `duplicate_indices` 此后从未读取（fix pass 独立重算 `orphan_slugs`）。
- **修复**：删除死代码行和 `duplicate_indices` 声明。

**Bug 6（本次代码审查发现）：两跳查询缺 ORDER BY**

- **问题**：audit Check 1 两跳候选替换查询无 `ORDER BY`，结果不确定。
- **修复**：补充 `ORDER BY l2.created_at DESC`。

---

### Dream Synthesize 改进

- system prompt 由「cite [[slug]]」改为「cite [[slug | chunk:N]]，N 是上下文中显示的 chunk DB id」
- 上下文由截断式 snippet 改为逐 chunk 传入（`SELECT id, text FROM chunks WHERE is_compiled_truth=1`），每块带 `[chunk:{id}] text` 标签，LLM 可准确引用
- `fetch_synthesis_sources` 添加 `source_chunk_id` 参数，通过 `source_chunk_idx` 字段过滤，只展示当前显示块的原始来源（旧数据 silent degradation）

---

### 测试数据状态（/Users/hongyu/project/rbrain-test，全量重建）

| 指标 | 数值 |
|------|------|
| 总页面数 | 405 |
| 源文章（note） | 57 |
| 概念（concept） | 220 |
| 人物（figure） | 111 |
| 综合分析（synthesis） | 13 |
| 总 chunk 数 | 2596 |
| is_compiled_truth=0 chunk 数 | 0 |
| 嵌入覆盖率 | 100% |

---

---

## 2026-05-27 — 安全加固、正确性修复与设计决策

### 已提交功能（commit c5f45d7）

**安全加固**
- `validated_slug`：拒绝路径穿越（`../`、绝对路径、反斜杠），所有 slug 输入统一走此函数
- `INSERT ON CONFLICT DO UPDATE` 替换 `INSERT OR REPLACE`，保留原始 `created_at`

**正确性修复**
- `chunk_and_embed_page`：先获取 embedding 再删除旧 chunk，API 失败时保留可搜索内容
- links 表加 `is_generated` 列（migration 0011）；re-extract 只删除 `is_generated=1` 的行，保留用户手动创建的边
- 向量搜索：改为 `score / boost` + ASC 排序（距离语义：越小越相关）；原来 `score * boost` + DESC 是错的
- `sync_file_to_db`：用 parse/re-serialize 替换 verbatim 覆盖，timeline 正确保留在 timeline 字段
- `is_derived_research_context`：将过滤范围从 `draft|synthesis|wiki` 扩展到 `concept|figure|evidence|memo`，think 只检索 note/raw 原始文献
- dream extract：无关联人物的事件路由到 `research/evidence/events/` 而非写入原始文章
- `extracted_event_rejection_reason`：将 `source_publication_event` 判断收窄为仅 `"期发表"`，避免误拒"顾明远发表重要文章…"类真实学术贡献事件

**新增集成测试**（17 个全部通过）
- slug 路径穿越拒绝
- 显式链接跨页面更新存活
- timeline 往返一致性
- chunk 失效与 timeline 保留
- evidence 页面路由（不污染原始文章）
- 向量搜索最近结果排名

### 系统性代码审查结论（2026-05-27）

对全部改动进行系统性审查，结论如下：

**无问题**：validated_slug 路径防护、embed-before-delete 顺序、ON CONFLICT DO UPDATE 保留 created_at、is_generated 链接删除、fetch_synthesis_sources 来源标注、event_rejection_reason 过滤、audit_citations 两跳查询

**已核实的误判**：两处 indegree boost 实现（apply_backlink_boost 用 `score * boost` + DESC，search_with_context 用 `score / boost` + ASC）因分数语义不同（RRF 分数 vs 向量距离）而设计不同，均正确

**设计观察（不修改）**：
- `sync_file_to_db` 从 `compiled_truth + timeline` 中提取 wikilink；由于先 DELETE `is_generated=1` 再 INSERT OR IGNORE，每次 sync 后结果与文件内容一致，不是真实 bug
- Evidence 页面被 `is_derived_research_context` 过滤出 think/generate 检索——设计有意为之，派生内容不参与 AI 检索

### 设计决策：Timeline 功能定位

**结论**：timeline 保留为展示层（人工浏览"学者年谱"），不让其参与 AI 检索。

原因：
1. 学术文献是静态语料，不是动态事件流；需要的是概念谱系，不是事件追踪
2. Timeline 是 LLM 跨文章摘要，天然与"引用必须可追溯到原文"的学术要求冲突
3. gbrain 的设计（时态查询才开放 timeline chunk）对动态个人知识库有意义，对静态文献库收益有限
4. 完整实现（chunk_source 字段 + 意图分类 + 条件检索）约需 3-4 天，边际收益低

### 设计决策：Citation Graph 方向

学术研究中真正有价值的图谱能力是 **citation graph**（引文关系），而非 timeline（事件关系）。

规划三个层次：
- **层次 1**（低成本）：改善 think 中 indegree boost 权重，优先计算来自 synthesis/draft 的 `evidence` 类型链接，让高被引基础文献在检索时排名更高。约 10 行 SQL 改动
- **层次 2**（核心）：在 `dream_extract` 中解析各 note 页面的参考文献节，将能匹配到知识库内页面的引用写入 links 表（`link_type: 'cites'`）。揭示语料库内部引文网络
- **层次 3**（后续）：think/generate 对 `cites` 入度高的 note 页面加权，让高被引文献自动浮出

---

## 待办

- [ ] Citation graph 层次 1：改善 think indegree boost（区分 evidence 链接 vs 其他链接）
- [ ] Citation graph 层次 2：dream_extract 解析参考文献节，写入 cites 类型链接
- [ ] MCP 新增工具（brain_think、brain_add_timeline_entry、brain_add_tag、brain_remove_tag、brain_outlinks）的端到端验证
- [ ] 考虑对已有 concept/figure 页面补充 language 字段（部分页面仍为 unknown）
