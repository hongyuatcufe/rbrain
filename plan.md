# rbrain 新定位技术方案

日期：2026-06-14（v3，research_run 统一容器、文献综述纳入三段式框架、收敛 page type）

## 结论

rbrain 不再优先定位为"搜索资料的个人知识库"，而是定位为：

```text
rbrain = 学术研究过程账本 + 证据核查器 + 改进建议引擎
Swiftide / rbrain-agent = 研究流程执行与调度
Python / R / DuckDB / 浏览器 = 具体计算和外部操作
```

在这个定位下，rbrain 的核心职责是：

1. 记录研究过程：保存研究问题、数据、文献、脚本、产物、发现、局限、备忘。
2. 核实研究结果：检查 finding、artifact、draft 是否有可追溯证据链。
3. 提出改进方向：把核查结果转化为结构化 next actions，供用户或 agent 执行。

## v3 核心修订：research_run 是所有研究流程的统一容器

此前方案把文献综述、期刊简报、热点报告、推荐、定量分析等能力拆成较多 page type，容易导致类型膨胀和 agent 写入时的选择困难。v3 改为：

```text
research_run = 统一研究容器
run_kind     = literature_review | journal_brief | hotspot_report
             | paper_drafting | quantitative_analysis | recommendation_push
             | mixed_methods | theory_building | research_design | custom
```

也就是说，**文献综述不是一个独立架构，期刊简报也不是一个独立架构；它们都是 `research_run` 的不同 `run_kind`**。

### 三段式职责与对象映射

| 职责 | 核心对象 | 说明 |
|---|---|---|
| 记录过程 | `research_run` + timeline | 记录目标、输入、阶段、状态、agent/validator/user 操作日志 |
| 记录输入 | `dataset` / `literature_corpus` / `citation_record` / `source` | 保存研究使用的数据、文献集合、单条引文和原始材料 |
| 记录产物 | `artifact` | 所有中间/最终产物用 `artifact_kind` 区分，不再扩散为大量 page type |
| 记录结论 | `finding` | 所有可核查主张、研究发现、趋势判断都沉淀为 finding |
| 核实结果 | `validation_report` | validator 输出的结构化报告，不只写 timeline |
| 提出建议 | `action_item` | 从 validator suggested_actions 或 agent 判断生成的可执行下一步 |
| 记录方法 | `method_note` / `research_memo` | 方法说明、人工决策、阶段备忘 |

### 收敛后的核心 page type

优先使用以下新 workflow 类型：

```text
research_run
dataset
literature_corpus
citation_record
source
artifact
finding
validation_report
action_item
method_note
research_memo
```

旧知识库类型继续兼容：

```text
note concept person org event wiki book paper figure synthesis question draft memo period
```

但新流程不再优先新建 `brief`、`hotspot_report`、`recommendation`、`result`、`analysis_plan`、`script`、`limitation` 等独立 page type，而是按如下方式收敛：

| 旧/候选类型 | v3 收敛方式 |
|---|---|
| `brief` | `artifact(artifact_kind=brief)` |
| `hotspot_report` | `artifact(artifact_kind=hotspot_report)` |
| `recommendation` | `action_item` 或 `artifact(artifact_kind=recommendation_report)` |
| `result` | `artifact(artifact_kind=result)` 或 `finding` |
| `analysis_plan` | `artifact(artifact_kind=analysis_plan)` 或 `research_run.frontmatter.analysis_plan` |
| `script` | `artifact(artifact_kind=script)`，必要时 `computed_by` 链接 |
| `limitation` | `validation_report.issue`、`finding(status=limitation)` 或 `action_item` |
| `gap_analysis` | `artifact(artifact_kind=gap_analysis)` |
| `contradiction_note` | `artifact(artifact_kind=contradiction_report)` |
| `citation_report` | `validation_report` 或 `artifact(artifact_kind=citation_report)` |

### 标准 research_run 结构

`research_run` frontmatter 至少包含：

```yaml
type: research_run
run_id: "uuid-or-stable-id"
run_kind: "literature_review"
status: "active | validating | needs_revision | completed | archived"
title: "..."
question: "..."
created_by: "user | agent"
profile: "literature_review"
```

统一 provenance 图：

```text
research_run --uses_dataset--> dataset
research_run --uses_corpus--> literature_corpus
research_run --uses_method--> method_note
research_run --produces--> artifact
research_run --produces--> finding
artifact     --derived_from--> dataset | literature_corpus | citation_record | source
artifact     --computed_by--> artifact(artifact_kind=script) | method_note
finding      --supports--> artifact | citation_record | source chunk
finding      --contradicts--> finding | citation_record | source chunk
validation_report --validates--> research_run | artifact | finding
action_item  --recommends--> research_run | artifact | finding
```

`timeline` 只记录事件，不承载事实结论：

```text
agent step completed
validator ran
user manually revised
pipeline produced artifact
```

正式结论必须进入 `finding`；核查结果必须进入 `validation_report`；下一步必须进入 `action_item`。

## 与 rbrain-hub 的关系（重要叙事修订）

rbrain-hub（位于 `/Users/hongyu/project/zeroclaw_with_rbrain/rbrain-hub`）**不是参考实现，而是同源演化中走在前面的版本**。本方案的 M1–M4 主要工作是**从 rbrain-hub 回迁 + 适配**，不是从零设计。

### 取舍清单

回迁时明确以下取舍，避免被上游设计裹挟：

- **保留 usearch**，拒绝 LanceDB。rbrain-hub 已切到 LanceDB，但 Rust SDK v0.17 的 sparse ANN 缺失，且对私人助理场景过度。回迁 ExplainedHit 时需重接 usearch sparse 通路。
- **拒绝多租户 TenantContext**。rbrain-hub 的 MCP 层已引入 TenantContext，rbrain 不需要，简化掉。
- **保留现有 DeepSeek / Qwen 客户端**（`rbrain-llm`），不要被 rbrain-hub 的 LLM 抽象改写。
- **保留 Tantivy + Lindera 全文检索**。CJK 友好的 BM25 不会被任何 vector backend 替代。

### 回迁分包策略（PR 切分）

| PR | 内容 | 来源文件 | 估算行数 |
|---|---|---|---|
| PR-A | pipeline.rs 整体回迁 | rbrain-hub `crates/rbrain-engine/src/pipeline.rs` | ~680 |
| PR-B | validator framework 整 crate 回迁 | rbrain-hub `crates/rbrain-engine/src/evidence/*` | ~1900 |
| PR-C1 | 检索增强 — 标题前缀 embedding | engine.rs:571–579 | ~20 |
| PR-C2 | 检索增强 — page-level max-pooling | engine.rs:7146–7175 | ~30 |
| PR-C3 | 检索增强 — JSON 精确 tag 过滤 | engine.rs:929–934 | ~10 |
| PR-C4 | 检索增强 — ExplainedHit + RRF 解释 | engine.rs:1643–1722 | ~80 |
| PR-C5 | 检索增强 — intent-aware backlink boost | engine.rs:1847–1865 | ~120 |
| PR-D | research_run model + provenance edge 枚举 | rbrain-hub `research/model.rs` | ~150 |

每个 PR 独立验收，避免一次性大 merge。

## 明确放弃的方向

### 不集成 LanceDB 作为近期目标

保留现有本地检索栈：

```text
SQLite = 事实源
Tantivy + Lindera = CJK 友好的 BM25
usearch = 本地 dense vector index
```

放弃 LanceDB 集成的原因：

- rbrain 是私人学术助理，默认场景不需要团队/云端向量库、S3 或多写并发。
- 当前检索质量瓶颈主要在 chunk 语境、标题增强、结果解释、来源多样性、证据过滤，而不是 vector backend。
- CJK 全文检索仍应保留 Tantivy + Lindera；LanceDB 即使引入也不能替代现有 BM25 管线。
- 引入 LanceDB 会显著增加依赖、存储模型和故障排查复杂度。
- rbrain-hub 上游 LanceDB Rust SDK v0.17 sparse ANN 实际不可用，是临时技术债。

保留的扩展口：

- `VectorStore` trait 继续保持后端可替换。
- 如果未来出现百万级 chunks、多模态材料、远程同步或团队协作需求，再以可选 backend 方式重新评估。

## 关键架构决策

### A. Schema 治理（M0 强制前置）

当前 `Page.page_type: String` 与 `frontmatter: serde_json::Value` **无任何校验**，`edge_type` 在 `links.rs:69–100` 也是自由字符串（只硬编码了 5 种推断）。M2 一旦放开"agent / LLM 写入研究过程"的 API，数据会立刻退化为自由文本，validator 无从工作。

**M0 必须先落地**：

```rust
// rbrain-core/src/schema.rs (新增)
pub enum PageType {
    // 兼容现有知识库
    Concept, Person, Org, Event, Note, Source,
    Wiki, Book, Paper, Figure, Synthesis, Question, Draft, Memo, Period,
    // v3 新流程核心对象
    ResearchRun,
    Dataset, LiteratureCorpus, CitationRecord,
    Artifact, Finding,
    ValidationReport, ActionItem,
    MethodNote, ResearchMemo,
    // 兜底
    Unknown(String),
}

pub enum EdgeType {
    // 通用
    Mentions, Cites, DerivedFrom, Updates,
    // 研究过程与证据链
    UsesDataset, UsesCorpus, UsesMethod, ComputedBy,
    Produces, Supports, Contradicts, Validates,
    Limits, Recommends, Contains,
    Unknown(String),
}
```

- `put_page` 入口用 `jsonschema` crate 按 page_type 校验 frontmatter，schema 文件放 `schemas/{page_type}.json`。
- 新增 migration 记录 `schema_version`，旧数据以 `PageType::Unknown(raw)` 兼容。
- `infer_edge_type` 扩展或弃用：M2 之后所有边类型由 agent / 用户**显式指定**，禁止 LLM 自由生成。
- `artifact`、`finding`、`validation_report`、`action_item` 必须用 frontmatter 的 `*_kind` / `status` / `target` 字段进一步约束语义，避免继续扩散 page type。

无此前置，M2 / M3 / M5 全部"建在沙地上"。

### B. Timeline 语义重定义

**现状混乱**：timeline 字段当前由 `dream_extract` 用 LLM 写入"梦幻流水"，但 plan.md 新定位下要承担 validator 报告、agent 运行日志、用户备忘三种用途；同时又要排除出 embedding。

**重定义**：

```rust
pub struct TimelineEntry {
    pub ts: DateTime<Utc>,
    pub source: TimelineSource,   // user | validator | agent | script | llm
    pub kind: String,             // validation_result | agent_step | manual_note | extract_summary | ...
    pub payload: serde_json::Value,
}
// page.timeline: Vec<TimelineEntry> （取代当前的字符串）
```

约束：

- **所有 source 的 entry 都不参与 embedding / indexing**（沿用现状）。
- `source = llm` 的 entry **仅供人类阅读**，不喂回任何下游 LLM prompt，避免 hallucination 自我强化。
- validator 输出（M3）与 agent 步骤日志（M6）必须落到 timeline，以 source 区分。
- 检索时只看 `compiled_truth`；timeline 是只读审计层。

### C. dream cycle 与 agent 的边界

**现状重叠**：当前 `dream_extract` 在做"生成性"工作（LLM 抽 concept / event），plan.md M6 又让 agent 做 brief / hotspot / recommendation 等生成。

**重划职责**：

| 层 | 职责 | 不做 |
|---|---|---|
| dream（housekeeping） | lint、embed、extract（仅图谱实体抽取）、cleanup、stale 检查 | 不生成 brief / synthesis / report |
| PipelineRunner（同步） | 单次生成任务：synthesize / brief / hotspot / recommendation | 不定时、不多 agent |
| rbrain-agent（编排+定时） | cron 触发 + 多步骤 workflow + 调用 PipelineRunner + 写回 rbrain | 不存事实源、不替代 validator |

`rbrain dream --profile X` 长期作为 `rbrain pipeline run X` 的别名保留，新代码统一使用 `pipeline` 子命令；M4 之后 `dream` 关键字仅指 housekeeping。

### D. M5 期刊引文库的独立检索路径

200+ 期刊、万级 citation_record 的 brief / hotspot 不是 chunk-level RRF 能解决的：
- chunk 级 dedupe ≠ record 级 dedupe
- 时间窗 + 期刊 + 关键词聚合是关系型 + 全文检索的混合 query，不是 ANN
- RRF chunk pipeline 会被拉慢且语义错位

**独立查询路径**：

```rust
// rbrain-engine/src/citations.rs (新增)
pub struct CitationFilter {
    pub time_window: Option<(Date, Date)>,
    pub journals: Vec<String>,
    pub keywords: Vec<String>,
    pub authors: Vec<String>,
    pub language: Option<Language>,
    pub corpus: Option<String>,
}

pub struct CitationAgg {
    pub group_by: Vec<GroupKey>,   // journal | year | author | keyword
    pub dedupe_key: DedupeStrategy, // doi | title_author_year | hash
    pub topic_cluster: Option<ClusterCfg>,
}

impl Engine {
    pub fn citations_query(&self, f: CitationFilter, a: Option<CitationAgg>)
        -> Result<CitationQueryResult>;
}
```

- 底层走 SQL（用 `citation_record` 的 frontmatter JSON 列 + 索引）+ Tantivy 全文（title/abstract/keywords）。
- **绕过 chunk pipeline / RRF**。
- brief 生成时由 agent 调 `citations_query`，结果再交给 LLM 综合，而不是把 citation_record 当 page 走 hybrid_search。

### E. 文献综述作为 research_run 的核心流程

rbrain-hub 的文献综述效果更好，原因不是 page type 更多，而是采用了证据驱动的多阶段 pipeline：

```text
extract_pub_metadata_cnki
extract_pub_metadata_auto
extract
synthesize
compose
detect_gaps
detect_contradictions
verify_citations
```

v3 采用这条流程，但把它纳入统一容器：

```text
research_run(run_kind = literature_review, profile = literature_review)
```

#### 文献综述 run 的输入

```text
research_run --uses_corpus--> literature_corpus
literature_corpus --contains--> citation_record
research_run --uses_dataset--> dataset(artifact_kind/corpus snapshot 可选)
research_run --uses_method--> method_note(literature_review_profile)
```

输入材料分三层：

| 对象 | 作用 |
|---|---|
| `literature_corpus` | 本次综述使用的文献集合定义，例如主题、期刊范围、时间范围、筛选条件 |
| `citation_record` | 单篇文献元数据，标题/作者/年份/期刊/关键词/摘要/DOI/hash |
| `source` / `note` / `article_note` | 原文、阅读笔记、摘录、人工评论；可作为 citation_record 的补充材料 |

#### 文献综述 stage 到三段式职责的映射

| rbrain-hub stage | 过程记录 | 产物记录 | 核查/建议 |
|---|---|---|---|
| `extract_pub_metadata_cnki` | timeline: `pipeline_step` | `artifact(artifact_kind=pub_metadata_table)` 或 `citation_record` 批量写入 | 缺字段 → `validation_report` + `action_item(add_metadata)` |
| `extract_pub_metadata_auto` | timeline: `pipeline_step` | 同上，标记 `metadata_source=llm` / `confidence` | 低置信度 → `action_item(manual_metadata_review)` |
| `extract` | timeline: `pipeline_step` | `artifact(artifact_kind=concept_index)`；兼容生成 `concept` / `figure` / evidence note | 概念过多/重复 → `action_item(merge_concepts)` |
| `synthesize` | timeline: `pipeline_step` | `artifact(artifact_kind=concept_synthesis)`，每个核心概念一个产物 | `synthesis_sections_have_citations`、`primary_source_ratio` |
| `compose` | timeline: `pipeline_step` | `artifact(artifact_kind=literature_review_draft)` | `review_links_to_synthesis_pages`、citation audit |
| `detect_gaps` | timeline: `pipeline_step` | `artifact(artifact_kind=gap_analysis)` + 可转化为 `finding(status=gap)` | 缺失则 `action_item(run_gap_analysis)` |
| `detect_contradictions` | timeline: `pipeline_step` | `artifact(artifact_kind=contradiction_report)` + 可转化为 `finding(status=contradiction)` | 缺失则 `action_item(run_contradiction_detection)` |
| `verify_citations` | timeline: `validator_run` | `validation_report(validator=bibliography_consistency)` | 生成 `action_item(add_citation/fix_citation)` |

#### 文献综述中的 finding

最终综述 draft 不应是唯一结果。rbrain 需要从 synthesis / compose / gap / contradiction 产物中沉淀可核查 finding：

```text
finding(status = draft | claim | validated)
finding_kind = theme | trend | gap | contradiction | method_pattern | theoretical_position
```

典型证据链：

```text
research_run --produces--> finding(theme)
finding --supports--> artifact(concept_synthesis)
finding --cites--> citation_record
finding --supports--> source chunk
validation_report --validates--> finding
action_item --recommends--> finding
```

这解决两个问题：

1. 文献综述不是只生成一篇文章，而是生成一组可核查的研究判断。
2. 后续写论文、简报、推荐时可以复用 finding，而不是反复从长文里抽取结论。

#### 文献综述 validators

直接吸收 rbrain-hub 的文献综述 validator 思路，但输出落到 `validation_report`：

```text
source_count_minimum
citation_chunks_exist
citation_chunk_matches_slug
primary_source_ratio
synthesis_sections_have_citations
gap_analysis_present
contradictions_recorded
review_links_to_synthesis_pages
bibliography_consistency
```

validator 输出统一：

```text
validation_report
  target_slug = research_run | artifact | finding
  validator = "citation_chunks_exist"
  status = pass | warn | fail
  affected_slugs = [...]
  message = "..."
  suggested_actions = [...]
```

其中 `suggested_actions` 必须可以转化为 `action_item`：

```text
add_citation
fix_citation
link_evidence
merge_concepts
rerun_stage
record_gap
record_contradiction
manual_review
```

#### 文献综述的验收标准

一个 `literature_review` research_run 不能只看是否生成了 draft，而要同时满足：

1. 有注册 corpus 和 source/citation 输入。
2. 每个 synthesis 有可追溯 chunk/citation。
3. 最终 review draft 链接到 synthesis 和 primary source。
4. gap_analysis 和 contradiction_report 至少运行过一次，哪怕结论是“未发现明显空白/矛盾”。
5. citation/bibliography validator 通过或生成明确 action_item。
6. 关键论断被沉淀为 finding，并能通过 `brain_provenance_of` 找到证据链。

## 应从 rbrain-hub 回迁的模块

### 1. 检索质量增强（PR-C1 ~ C5）

- 标题前缀 embedding（rbrain-hub engine.rs:571–579）
- timeline 不参与索引（rbrain-hub engine.rs:623–626，rbrain 已部分符合）
- `query --explain` / ExplainedHit（rbrain-hub engine.rs:1643–1722），删去 LanceDB sparse 分支，接 usearch
- page-level max-pooling `apply_page_cap`（rbrain-hub engine.rs:7146–7175）
- JSON 精确 tag 过滤（`json_each()`，rbrain-hub engine.rs:929–934）
- intent-aware backlink boost（rbrain-hub engine.rs:1847–1865，三档 0.15 / 0.05 / 0）

### 2. PipelineStep 抽象（PR-A）

直接回迁 rbrain-hub `crates/rbrain-engine/src/pipeline.rs`（679 行），含：

```text
InputSpec        = SelfContent | LinkedSources | AggregateContent
PromptSpec       = File | Inline
ResponseFormat   = Json | Markdown
OutputMode       = Return | SaveAs | SaveMulti | UpdateFrontmatter
PipelineStep     = input + prompt + response + output + execution control
PipelineProfile  = TOML 反序列化为多 step
RetryParser      = JSON 解析失败回灌错误重试
```

CJK JSON 引号修复、RetryParser、batch_size / incremental / max_inputs / inject_existing_titles 等细粒度控制随 PR-A 一并到位。

把当前硬编码 dream cycle（engine.rs:2069–2295）改成 TOML profile：

```bash
rbrain pipeline run lint            # 原 dream lint
rbrain pipeline run embed           # 原 dream embed
rbrain pipeline run extract         # 原 dream extract
rbrain pipeline run literature_review
rbrain pipeline run journal_brief
rbrain pipeline run hotspot_report
```

### 3. Research memory（PR-D）

新增或规范 v3 核心 page types（详见 §A 与 v3 核心修订）：

```text
research_run
dataset                literature_corpus
citation_record        source
artifact               finding
validation_report      action_item
method_note            research_memo
```

原则：流程差异进入 `research_run.run_kind`；产物差异进入 `artifact.artifact_kind`；核查差异进入 `validation_report.validator`；改进方向进入 `action_item.action_kind`。

### 4. Provenance graph（PR-D）

固定研究边类型（详见 §A）：

```text
uses_dataset    uses_corpus    uses_method    computed_by
derived_from    produces       supports       contradicts
cites           limits         validates      recommends    updates
```

典型证据链：

```text
research_run --uses_dataset--> dataset
research_run --uses_corpus--> literature_corpus
research_run --produces--> finding
finding --supports--> artifact/result/source chunk
artifact --derived_from--> dataset/citation_record
artifact --computed_by--> script
artifact(artifact_kind=brief) --cites--> citation_record/article_note/source chunk
artifact(artifact_kind=hotspot_report) --derived_from--> literature_corpus
validation_report --validates--> artifact/finding/research_run
action_item --recommends--> artifact/finding/research_run
```

### 5. Validators（PR-B）

直接回迁 rbrain-hub `crates/rbrain-engine/src/evidence/*` 整 crate（~1900 行）。validator 不执行分析代码，只检查状态、元数据和证据链。

数据分析类（rbrain-hub 已有）：

- `dataset_registered`
- `dataset_hash_present`
- `analysis_plan_exists`
- `artifact_hash_present`
- `result_has_script`
- `finding_has_support`
- `finding_has_dataset_lineage`
- `limitation_recorded`

文献综述类（部分回迁 + 补齐）：

- `source_count_minimum`
- `citation_chunks_exist`
- `citation_chunk_matches_slug`
- `primary_source_ratio`
- `synthesis_sections_have_citations`
- `gap_analysis_present`
- `contradictions_recorded`
- `review_links_to_synthesis_pages`
- `bibliography_consistency`

期刊引文简报类（M5 新增）：

- `citation_corpus_registered`
- `citation_records_have_required_fields`
- `deduplication_done`
- `artifact_brief_has_time_window`
- `artifact_brief_cites_records`
- `artifact_hotspot_report_has_method_note`
- `recommendation_actions_match_user_profile`

validator 输出统一结构（沿用 rbrain-hub）：

```json
{
  "validator": "finding_has_support",
  "status": "pass | warn | fail",
  "message": "...",
  "affected_slugs": ["..."],
  "suggested_actions": [
    {
      "action": "link_evidence",
      "payload": { "from": "...", "to": "...", "link_type": "supports" }
    }
  ]
}
```

`SuggestedAction` 枚举需在 M3 收尾时一次性扩展到 M5 所需动作（`RegisterCorpus / AddCitationRecord / DedupeCorpus / RegenerateBrief` 等），避免 M5 再改一遍 + migration。

所有 validator 输出必须有两种落点：

1. append 到对应 page 的 `timeline`（source=validator），作为审计日志；
2. 写入或更新 `validation_report`，作为可检索、可追踪、可转化为 action_item 的正式核查产物。

## 重点应用场景：200+ 中外文期刊引文库

### 场景描述

系统需要持续管理 200 多个中外文期刊的引文信息，包括：

- 标题
- 作者
- 年份
- 期刊
- 关键词
- 摘要
- DOI / URL / CNKI 等来源标识
- 学科分类
- 主题标签
- 更新时间

目标输出：

- 定期综合简报
- 用户定制推送
- 学术研究热点分析报告
- 研究主题的文献推荐
- 某一领域的趋势、争议、空白和代表性作者/机构分析

### 数据模型

推荐使用 v3 核心 page types：

```text
citation_record     单篇文献元数据记录
literature_corpus   一个期刊集合、主题集合或时间窗口语料库
artifact            定期简报、热点分析、聚类结果、趋势表等产物
finding             从简报/热点分析中沉淀出的趋势、主题、空白、争议
validation_report   简报/热点/引用核查结果
action_item         用户推送、后续阅读、补证据、重跑分析等建议
method_note         生成简报/热点报告的方法说明
```

`citation_record` frontmatter 最小字段：

```yaml
type: citation_record
title: "..."
authors: ["..."]
year: 2026
journal: "..."
keywords: ["..."]
abstract: "..."
doi: null
url: null
source: "cnki | crossref | manual | csv"
language: "zh | en | ja | ko | other"
corpus: "..."
ingested_at: "YYYY-MM-DDTHH:MM:SSZ"
record_hash: "sha256:..."
```

`artifact(artifact_kind=brief)` frontmatter 最小字段：

```yaml
type: artifact
artifact_kind: brief
title: "..."
time_window:
  from: "YYYY-MM-DD"
  to: "YYYY-MM-DD"
corpus: "..."
method: "research/methods/journal-brief"
source_count: 0
created_by: "rbrain | agent | user"
```

`artifact(artifact_kind=hotspot_report)` 复用 artifact schema，并额外要求 `time_window`、`corpus`、`method`、`source_count`、`comparison_window`（如适用）。

`action_item` 用于用户推送与改进建议：

```yaml
type: action_item
action_kind: "read_article | track_topic | fix_citation | rerun_stage | enrich_corpus"
status: "open | in_progress | done | dismissed"
target: "citation_record/artifact/finding/research_run slug"
reason: "..."
priority: "low | medium | high"
```

以上均由 M0 的 `jsonschema` 校验。

### 检索与聚合需求（独立查询路径，见 §D）

期刊简报不同于普通综述，重点不是只找几篇强相关文献，而是对一段时间窗口内的大量记录做聚合：

- 按时间窗口过滤。
- 按期刊、关键词、作者、语种、学科分类过滤。
- 对标题、关键词、摘要做 hybrid retrieval。
- 对结果做主题聚类和去重。
- 对热点做趋势比较，而不是只做静态总结。
- 输出必须能回链到具体 `citation_record`。

**全部走 `citations_query`，不走 chunk-level RRF**。

需要新增的能力：

- `rbrain import-citations --format csv|json|ris|bib`
- `rbrain citations dedupe`
- `rbrain citations query`
- `rbrain pipeline run journal_brief --corpus ... --from ... --to ...`
- `rbrain pipeline run hotspot_report --corpus ... --window monthly|quarterly`
- MCP tools：`brain_register_citation_corpus`、`brain_import_citation_records`、`brain_citations_query`、`brain_record_artifact`、`brain_record_finding`、`brain_provenance_of`

### Agent 层（rbrain-agent + Swiftide）的角色

rbrain-agent 不负责保存事实源，也不替代 rbrain 检索。它适合做：

- 定时任务编排：每周/月生成简报。
- 多步骤推送流程：识别用户兴趣 → `citations_query` → 聚类 → 生成推荐 → 回写 rbrain。
- 多 agent 协作：Retriever、TrendAnalyzer、BriefWriter、CitationAuditor。
- 用户交互：根据用户反馈调整主题、过滤条件和推送风格。

## 其他研究流程如何进入 research_run

所有流程都遵守同一模板：

```text
create research_run(run_kind=...)
register inputs
run pipeline / agent workflow
record artifacts
extract or record findings
validate artifacts/findings/run
convert suggested_actions to action_item
```

### 1. 期刊定期简报：`run_kind = journal_brief`

适用场景：每周/月/季度对 200+ 中外文期刊引文库做综合汇总。

```text
research_run(run_kind=journal_brief)
  --uses_corpus--> literature_corpus
  --uses_dataset--> dataset(citation_snapshot)
  --uses_method--> method_note(journal_brief_method)
  --produces--> artifact(artifact_kind=citation_query_result)
  --produces--> artifact(artifact_kind=brief)
  --produces--> finding(trend | theme | emerging_topic)
  --produces--> validation_report
  --recommends--> action_item
```

流程：

1. `citations_query` 按时间窗、期刊、关键词、语种过滤。
2. dedupe 生成 `artifact(artifact_kind=dedupe_report)`。
3. 聚合主题/关键词/期刊分布，生成 `artifact(artifact_kind=citation_aggregation)`。
4. LLM 生成 `artifact(artifact_kind=brief)`。
5. 从简报中抽取 finding，例如“本月 AI 教育评价论文显著增加”。
6. validator 检查：
   - 时间窗是否明确。
   - 引用是否回链到 `citation_record`。
   - source_count 是否足够。
   - 是否完成去重。
   - 是否记录方法说明。
7. suggested_actions 转化为 action_item，例如补充缺失期刊、重跑 dedupe、修正引用。

简报本身不再是独立 `brief` page type，而是：

```text
artifact(artifact_kind=brief)
```

### 2. 热点分析报告：`run_kind = hotspot_report`

适用场景：发现某一领域近期研究热点、主题演化、争议和空白。

```text
research_run(run_kind=hotspot_report)
  --uses_corpus--> literature_corpus
  --uses_dataset--> dataset(citation_snapshot)
  --produces--> artifact(artifact_kind=topic_cluster)
  --produces--> artifact(artifact_kind=trend_table)
  --produces--> artifact(artifact_kind=hotspot_report)
  --produces--> finding(hotspot | trend | gap | contradiction)
```

流程：

1. `citations_query` 取目标时间窗和对照时间窗。
2. 聚类或关键词共现，生成 `topic_cluster`。
3. 对比历史窗口，生成 `trend_table`。
4. 生成 `hotspot_report` artifact。
5. 抽取 finding：
   - emerging hotspot
   - declining topic
   - cross-disciplinary topic
   - research gap
   - contradictory trend
6. validator 检查：
   - 是否有方法说明。
   - 是否有对照时间窗。
   - 热点 finding 是否有 citation_record 支持。
   - 聚类结果是否可追溯到输入记录。
7. action_item 示例：
   - 调整聚类粒度。
   - 增加英文期刊样本。
   - 人工合并相近主题。
   - 补充方法说明。

### 3. 用户定制推送：`run_kind = recommendation_push`

适用场景：根据用户研究兴趣，定期推送相关文献、主题和观点。

```text
research_run(run_kind=recommendation_push)
  --uses_corpus--> literature_corpus
  --uses_method--> method_note(user_profile_matching)
  --produces--> artifact(artifact_kind=recommendation_report)
  --produces--> action_item(read_article | track_topic | update_profile)
```

流程：

1. 读取用户 profile：研究主题、关注期刊、排除关键词、偏好语种。
2. `citations_query` 找候选文献。
3. rerank/cluster，生成 recommendation_report。
4. 每条推荐生成 action_item，而不是独立 recommendation page：
   - `read_article`
   - `track_author`
   - `track_topic`
   - `add_to_literature_review`
   - `dismiss_recommendation`
5. validator 检查：
   - 推荐是否有明确理由。
   - 是否回链到 citation_record。
   - 是否符合 user profile。
   - 是否避免重复推送。

### 4. 论文写作辅助：`run_kind = paper_drafting`

适用场景：从已有 finding、artifact、literature_review run 中组织论文草稿。

```text
research_run(run_kind=paper_drafting)
  --uses_method--> method_note(writing_plan)
  --uses_corpus--> literature_corpus
  --produces--> artifact(artifact_kind=outline)
  --produces--> artifact(artifact_kind=draft_section)
  --produces--> artifact(artifact_kind=paper_draft)
  --produces--> validation_report(citation/style/argument check)
```

流程：

1. 注册写作目标、期刊/会议要求、字数、结构。
2. 引入已有 finding 和 literature_review artifacts。
3. 生成 outline artifact。
4. 分章节生成 draft_section。
5. 汇总为 paper_draft。
6. validator 检查：
   - 每个实质性段落是否有 citation。
   - finding 是否有证据链。
   - 是否存在未展开的概念。
   - 是否存在引用格式/年份/作者错误。
7. action_item 示例：
   - 补 citation。
   - 拆分过大的 finding。
   - 增加反方文献。
   - 改写证据不足段落。

论文写作不应绕过 finding。草稿中的核心论断应尽可能链接到已有 finding，或新建 finding 后再进入核查。

### 5. 定量分析：`run_kind = quantitative_analysis`

适用场景：统计分析、回归、聚类、可视化、实验数据分析。

```text
research_run(run_kind=quantitative_analysis)
  --uses_dataset--> dataset
  --uses_method--> method_note(analysis_plan)
  --produces--> artifact(artifact_kind=script)
  --produces--> artifact(artifact_kind=result_table)
  --produces--> artifact(artifact_kind=chart)
  --produces--> finding(numeric_claim)
```

流程边界：

- rbrain 记录数据、脚本、结果、finding 和核查。
- Python/R/DuckDB/Notebook 执行具体计算。
- agent/Swiftide 可以编排执行，但计算事实源必须回写 rbrain。

validator 检查：

```text
dataset_registered
dataset_hash_present
analysis_plan_exists
artifact_hash_present
script_registered_for_result
finding_has_supporting_artifact
finding_has_dataset_lineage
limitations_recorded
```

action_item 示例：

```text
register_dataset
record_analysis_plan
link_result_table
record_limitation
rerun_analysis
add_codebook
```

### 6. 质性/混合研究：`run_kind = mixed_methods`

适用场景：访谈、文本编码、案例研究、量化结果与文献/访谈材料结合。

```text
research_run(run_kind=mixed_methods)
  --uses_dataset--> dataset(interview_transcripts | survey)
  --uses_corpus--> literature_corpus
  --uses_method--> method_note(codebook)
  --produces--> artifact(artifact_kind=codebook)
  --produces--> artifact(artifact_kind=coded_segments)
  --produces--> artifact(artifact_kind=joint_display)
  --produces--> finding
```

validator 检查：

- 是否有 codebook。
- finding 是否同时标明文献证据和经验材料证据。
- coded_segments 是否能回链到原始材料。
- 量化 finding 是否有 dataset lineage。
- 是否记录局限。

### 7. 理论建构：`run_kind = theory_building`

适用场景：围绕概念、理论张力、学术观点形成原创框架。

```text
research_run(run_kind=theory_building)
  --uses_corpus--> literature_corpus
  --produces--> artifact(artifact_kind=concept_map)
  --produces--> artifact(artifact_kind=theory_memo)
  --produces--> finding(theoretical_position)
  --produces--> action_item(test_theory | collect_counterexample)
```

validator 检查：

- 理论 finding 是否有文献支持。
- 是否记录反例/矛盾。
- 是否区分原创判断和文献观点。
- 是否给出后续验证路径。

这类流程特别需要 `research_memo`，因为理论建构中很多关键内容是人的判断过程，而不是可自动生成的结果。

## Agent 框架选型：Swiftide（首选） + 自实现兜底

经横向评估（Rig / AutoAgents / graph-flow / Kowalski / pi_agent_rust / ADK-Rust 等），选定：

### 首选：Swiftide

- **MCP toolboxes 原生支持**：`rbrain-mcp` 暴露的 17+ tools 可被 Swiftide agent runtime-load，无桥接代码
- **typed task graph + pause/resume**：完美对应 plan.md 的"validator 输出 suggested_actions → 人确认 / agent 执行"模式
- **同一框架既能做 agent 又能做 RAG indexing**：未来如需把 chunker / embedding 重构为 streaming pipeline 可复用
- **Dashscope provider** = 阿里云 Qwen 直接可用；DeepSeek 走 OpenAI-compatible
- v0.32（94 个 release，2026-06 仍活跃），bosun-ai 维护

### 兜底方案：tokio-cron-scheduler + PipelineRunner

若 M6 启动前 0.5–1 天 Swiftide spike 发现 MCP toolbox 集成成本超预期，立即退回：

- `tokio-cron-scheduler` 做定时
- 直接顺序调用回迁版 `PipelineRunner`
- 通过 Engine API（同进程）或 rbrain-mcp（跨进程）读写

私人助理场景下串行 pipeline 已足够，纯自实现总代码量 <800 行可跑通 journal-brief。

### 明确放弃

- **ADK-Rust**：v1.0 真实性存疑，文档与训练数据有出入
- **pi_agent_rust**：是 end-user CLI（Claude Code 类），不是库
- **Rig + graph-flow**：MCP 非原生，需缝合两个框架，graph-flow 仍 v0.2.x
- **AutoAgents**：v0.3.x 不稳，无显式图编排
- **Kowalski / OpenFANG**：前者过小，后者过重

新增独立 crate：

```text
crates/rbrain-agent
```

第一阶段只暴露可选功能，不进入核心路径：

```bash
rbrain-agent run journal-brief --corpus education --window monthly
rbrain-agent run hotspot-report --topic "教育数字化"
rbrain-agent recommend --user-profile profiles/hongyu.toml
```

rbrain-agent 通过 MCP 或 Engine API 调用 rbrain：

```text
Swiftide workflow -> rbrain query/list/get/link/record/validate -> rbrain DB
```

## 技术路线

### M0：上游回迁前置（Schema 治理 + Timeline 重定义）

- `enum PageType` / `enum EdgeType`（含 `Unknown(String)` 兜底）
- v3 核心对象 schema：`research_run`、`dataset`、`literature_corpus`、`citation_record`、`artifact`、`finding`、`validation_report`、`action_item`
- `put_page` 入口 `jsonschema` 校验，`schemas/{page_type}.json`
- migration：`schema_version` 字段，存量数据迁移为 `Unknown`
- `Vec<TimelineEntry>` 取代 timeline 字符串字段，含 source / kind / payload
- 弃用或冻结 `infer_edge_type` 自由文本路径

**这是其他所有 milestone 的硬前置**。

### M1：检索与记录基础优化（回迁 PR-C1 ~ C5 + PR-D）

- 标题前缀 embedding
- timeline 不参与检索索引（已部分符合，M0 后彻底）
- `query --explain` / ExplainedHit（去 LanceDB 分支，接 usearch）
- page-level max-pooling
- JSON 精确 tag 过滤
- intent-aware backlink boost
- research_run 模型 + provenance edge 枚举

### M2：研究过程记录 API

- `brain_create_research_run`
- `brain_get_research_protocol`
- `brain_register_input`
- `brain_record_artifact`
- `brain_record_finding`
- `brain_record_validation_report`
- `brain_record_action_item`
- `brain_validate_research_run`
- `brain_provenance_of`

所有 API 严格按 M0 schema 校验，禁止自由 page_type / edge_type。

### M3：结果核查与改进建议（回迁 PR-B）

- validator framework（直接回迁 rbrain-hub `evidence/*`）
- data-analysis validators（直接回迁）
- literature-review validators（重点回迁：citation chunks、primary source ratio、synthesis quality、gap/contradiction、bibliography consistency）
- citation-brief validators（M5 新增，框架先就位）
- 结构化 `suggested_actions`，`SuggestedAction` 枚举一次扩展到 M5 所需动作
- validator 输出落为 `validation_report`
- `suggested_actions` 可转化为 `action_item`
- `brain_validate_research_run` 作为统一入口，便于 pipeline / agent 调用后写回 rbrain

### M4：Pipeline profiles（回迁 PR-A）

- 回迁 `PipelineStep` / `PipelineRunner` / `PipelineProfile`
- 改造 dream cycle 为 housekeeping profiles：`lint` / `embed` / `extract`
- 新增生成性 profiles：`literature_review` / `journal_brief` / `hotspot_report`
- 所有 profile 必须绑定 `research_run.run_kind`
- 每个 stage 必须记录 timeline，并把输出保存为 `artifact` / `finding` / `validation_report` / `action_item`
- PromptLoader 与外置 prompts（`prompts/*.md`）
- `rbrain dream --profile X` 保留为 `rbrain pipeline run X` 别名

### M5：期刊引文库支持

- 引文导入格式：CSV / JSON，后续再考虑 RIS / BibTeX
- `citation_record` schema（M0 已定义，M5 落地导入流程）
- dedupe：title + author + year + DOI / hash
- **独立检索路径 `citations_query`**（绕过 chunk pipeline，SQL + Tantivy 混合）
- 简报生成（走 `journal_brief` profile，输出 `artifact(artifact_kind=brief)`）
- 热点分析报告（走 `hotspot_report` profile，输出 `artifact(artifact_kind=hotspot_report)`）
- 用户兴趣 profile 与推荐结果记录（输出 `artifact(artifact_kind=recommendation_report)` 与 `action_item`）
- 期刊引文类 validator 落地（M3 框架已就位）

### M6：Swiftide 流程层

- 启动前 0.5–1 天 Swiftide spike：验证 MCP toolbox 调用 rbrain-mcp 可用
- 通过 → 新增 `rbrain-agent` crate，基于 Swiftide
- 不通过 → 退回 `tokio-cron-scheduler + PipelineRunner` 兜底方案
- 第一个 workflow：`journal-brief`（retrieve → cluster → generate → audit → save）
- 后续：`hotspot-report`、`recommendation`、`paper-writing helper`
- Swiftide 只做流程编排，不成为 rbrain 事实源
- 所有产物、证据链和核查结果必须回写 rbrain
- 所有 workflow 以 `research_run` 为恢复点和审计容器
- agent 运行日志 append 到对应 page 的 timeline（source=agent）

## 非目标

- 不让 rbrain 执行任意 Python/R/SQL。
- 不把 rbrain 改成通用 agent runtime。
- 不在近期引入 LanceDB。
- 不引入多租户/团队权限模型。
- 不把 Swiftide / agent session 当作长期研究状态。
- 不保留 `page_type: String` / `edge_type: String` 的自由文本路径。
- 不让 LLM 写入的 timeline entry 进入下游 prompt。
- 不让期刊 brief / hotspot 走 chunk-level RRF。

## 验收标准

新定位完成后，rbrain 应能回答：

1. 这个结论来自哪些文献/数据/脚本/结果？
2. 这个研究 run 现在卡在哪一步？
3. 哪些 finding 缺证据？
4. 哪些简报段落没有可追溯 citation_record？
5. 最近一个时间窗口内有哪些研究热点、争议和空白？
6. 对某个用户，哪些新文献或观点值得推送，为什么？
7. 下一步应该补什么证据、重跑什么分析、记录什么局限？

## 工作量估算

| 阶段 | 内容 | 新增代码 | 改动现有 |
|---|---|---|---|
| M0 | schema 治理 + timeline 重构 | ~400 | ~200 |
| M1 | 回迁检索增强 + research model | ~400 | ~100 |
| M2 | 研究过程 API（engine + MCP） | ~700 | ~50 |
| M3 | 回迁 validator framework | ~1900 | ~50 |
| M4 | 回迁 pipeline + dream 重构 | ~700 | ~200 |
| M5 | 期刊引文库（含独立检索路径） | ~900 | ~50 |
| M6 | rbrain-agent（Swiftide 路径） | ~1200 | ~50 |
| **合计** | | **~6200** | **~700** |

其中 **PR-A / PR-B / PR-C* / PR-D 合计约 3000 行可直接从 rbrain-hub 移植**，真正"新写"集中在 M0、M5、M6（约 3200 行）。
