# rbrain research_run 八类研究流程设计

日期：2026-06-14

本文档用于补充 `plan.md`。这里讨论的是文献综述之外的 8 类研究流程，目标是把它们统一纳入 rbrain 的新功能定位：

1. 记录过程：保存输入、步骤、产物、判断依据和责任链。
2. 核实结果：检查证据、引用、方法、逻辑、可复现性和适用边界。
3. 提出改进：把缺口、风险和下一步动作转化为可追踪的 `action_item`。

文献综述仍然是核心流程，已经在 `plan.md` 中作为 `literature_review` 类型的 `research_run` 详细设计。本文件聚焦其他 8 类流程。

## 0. 统一抽象

所有流程都进入 `research_run`，由 `run_kind` 区分类型：

```text
research_run
  run_kind: journal_brief | hotspot_report | recommendation_push | paper_drafting |
            quantitative_analysis | mixed_methods | theory_building | research_design
```

流程内部尽量使用少量稳定 page type：

```text
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

不同流程的差异主要放在 frontmatter 字段中：

```text
artifact.artifact_kind
validation_report.validator
action_item.action_kind
finding.finding_kind
```

这样做的好处是：不同研究流程不需要各自发明一套 page type，而是共享过程记录、结果核查、改进建议、溯源图谱和 MCP/API。

## 1. 当前完成度判断

截至目前，代码层面已经完成的是通用底座的一部分：

```text
create_research_run
record_artifact
record_finding
provenance_of
```

这意味着 8 个流程现在都可以被“粗粒度记录”为 research run、artifact、finding，并能做基础 provenance 查询。

但 8 个流程没有一个已经完成端到端工作流。原因是每个流程都还缺少专门的输入登记、校验报告、行动项、流程 profile、模板化产物、自动触发逻辑和领域校验器。

建议判断如下：

| 流程 | 当前状态 | 是否需要补开发 |
| --- | --- | --- |
| 期刊定期简报 `journal_brief` | 可用通用 run 记录，但没有简报专用流水线 | 需要 |
| 热点分析报告 `hotspot_report` | 可记录结果，但没有聚类、趋势和热点核查 | 需要 |
| 用户定制推送 `recommendation_push` | 可记录推送产物，但没有用户画像和匹配解释 | 需要 |
| 论文写作辅助 `paper_drafting` | 可记录草稿和发现，但没有论证、引用和章节核查 | 需要 |
| 定量分析 `quantitative_analysis` | 可记录数据和结果，但没有可复现链路校验 | 需要 |
| 质性/混合研究 `mixed_methods` | 可记录 memo，但没有编码本和三角互证结构 | 需要 |
| 理论建构 `theory_building` | 可记录概念产物，但没有概念关系和反例核查 | 需要 |
| 方法设计/研究方案 `research_design` | 可记录方案，但没有设计审查和风险清单 | 需要 |

结论：不是 8 个流程分别从零补开发，而是先补一次通用开发，把 `validation_report`、`action_item`、输入登记、profile 和 provenance 约束做稳，然后再按优先级实现流程模板。

## 2. 期刊定期简报

`run_kind: journal_brief`

### 目标

面向 200 多个中外文期刊，按周、月或季度汇总新文献，生成稳定格式的研究简报。简报不是简单摘要，而是要回答：

1. 最近新增了什么文献。
2. 哪些主题、方法、对象或地区正在增加。
3. 哪些文献值得重点阅读。
4. 哪些信息需要人工复核。

### 过程记录

核心输入：

```text
dataset: citation_snapshot
literature_corpus: journal_issue_batch
citation_record: article metadata
source: journal website / database export / manual import
```

核心过程：

```text
collect_metadata
deduplicate_records
normalize_fields
classify_topics
summarize_batches
compose_brief
verify_brief
```

核心产物：

```text
artifact_kind: metadata_import_report
artifact_kind: topic_distribution_table
artifact_kind: key_paper_list
artifact_kind: journal_brief
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: metadata_completeness
validator: duplicate_detection
validator: journal_scope_check
validator: citation_field_consistency
validator: brief_evidence_check
```

重点核查：

1. 标题、作者、年份、期刊、摘要、关键词是否缺失。
2. 中文和英文记录是否存在重复。
3. 简报中的趋势判断是否能回溯到 citation_record。
4. 是否把少量高频关键词误判为真实热点。
5. 是否遗漏用户指定期刊或时间窗口。

### 改进建议

典型 `action_item`：

```text
action_kind: fix_metadata
action_kind: merge_duplicates
action_kind: expand_source
action_kind: review_key_papers
action_kind: refine_topic_taxonomy
```

### 最小 MVP

第一版只需要支持：

1. 导入一批 CSV/JSON 引文记录。
2. 创建 `journal_brief` research run。
3. 记录元数据统计 artifact。
4. 生成简报 artifact。
5. 生成缺失字段和重复记录 validation report。
6. 生成待人工复核 action item。

### 当前开发缺口

```text
brain_register_input
brain_record_validation_report
brain_record_action_item
citation import / dedupe helper
journal_brief profile
brief template
```

## 3. 热点分析报告

`run_kind: hotspot_report`

### 目标

识别某一研究领域、期刊集合或时间窗口内的研究热点、上升主题、衰退主题、方法转向和潜在空白。

它和期刊简报的区别是：简报偏周期性信息服务，热点报告偏分析性判断。

### 过程记录

核心输入：

```text
dataset: citation_snapshot
literature_corpus: topic_window
citation_record: article metadata
artifact: previous_hotspot_report
```

核心过程：

```text
build_topic_terms
cluster_records
compare_time_windows
rank_hotspots
inspect_representative_papers
compose_hotspot_report
verify_hotspot_claims
```

核心产物：

```text
artifact_kind: topic_cluster_table
artifact_kind: trend_change_table
artifact_kind: representative_paper_list
artifact_kind: hotspot_report
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: time_window_consistency
validator: cluster_quality_check
validator: representative_paper_check
validator: trend_claim_evidence_check
validator: bias_and_coverage_check
```

重点核查：

1. 热点是否只是关键词频率上升，而没有实质主题聚合。
2. 代表性论文是否真的支撑热点名称。
3. 时间窗口是否一致。
4. 中英文关键词是否合并不当。
5. 样本来源是否偏向某些期刊、地区或学科。

### 改进建议

典型 `action_item`：

```text
action_kind: rename_topic_cluster
action_kind: split_or_merge_cluster
action_kind: add_representative_sources
action_kind: rerun_with_new_window
action_kind: compare_against_baseline
```

### 最小 MVP

第一版不必直接做复杂机器学习，可以先支持：

1. 按关键词、年份、期刊聚合。
2. 生成 topic cluster artifact。
3. 生成热点分析 artifact。
4. 校验每个热点至少有若干 citation_record 支撑。
5. 对证据不足的热点生成 action item。

### 当前开发缺口

```text
hotspot_report profile
topic aggregation helper
trend comparison helper
claim-to-citation validator
```

## 4. 用户定制推送

`run_kind: recommendation_push`

### 目标

根据用户的研究兴趣、正在进行的 research run、已有文献集合或明确问题，推送相关文献、观点、方法和趋势。

推送不是简单搜索结果。rbrain 应当记录为什么推送、依据是什么、用户是否采纳，以及下一轮如何改进。

### 过程记录

核心输入：

```text
artifact: user_interest_profile
research_run: related_active_run
literature_corpus: candidate_pool
citation_record: candidate_article
finding: prior_user_feedback
```

核心过程：

```text
build_interest_profile
retrieve_candidates
rank_candidates
explain_matches
compose_recommendation
collect_feedback
update_profile
```

核心产物：

```text
artifact_kind: interest_profile
artifact_kind: candidate_ranking
artifact_kind: recommendation_push
artifact_kind: feedback_summary
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: profile_match_check
validator: duplicate_recommendation_check
validator: novelty_check
validator: explanation_quality_check
validator: source_reliability_check
```

重点核查：

1. 推送是否和用户当前研究问题相关。
2. 是否重复推送用户已经看过或明确排除的文献。
3. 推荐理由是否可解释。
4. 是否把弱相关文献包装成强相关。
5. 是否平衡经典文献、新近文献和边缘但有启发的文献。

### 改进建议

典型 `action_item`：

```text
action_kind: update_interest_profile
action_kind: suppress_duplicate_source
action_kind: add_negative_preference
action_kind: request_user_feedback
action_kind: expand_candidate_pool
```

### 最小 MVP

第一版可以从“可解释推送”开始：

1. 用户给出兴趣描述。
2. 系统检索候选 citation_record。
3. 记录 ranking artifact。
4. 每条推荐必须附推荐理由和来源。
5. 对低置信度推荐生成 action item。

### 当前开发缺口

```text
user interest profile schema
recommendation_push profile
candidate ranking artifact
feedback recording API
duplicate recommendation validator
```

## 5. 论文写作辅助

`run_kind: paper_drafting`

### 目标

支持论文从选题、提纲、章节草稿、引用整合到修改建议的过程。rbrain 的重点不是替用户直接写完论文，而是保存写作依据、检查论证质量、指出修改方向。

### 过程记录

核心输入：

```text
research_memo: writing_intent
artifact: outline
artifact: section_draft
finding: literature_claim
citation_record: cited_source
validation_report: prior_review
```

核心过程：

```text
define_argument
build_outline
draft_section
attach_citations
check_argument_flow
check_citation_support
revise_draft
```

核心产物：

```text
artifact_kind: paper_outline
artifact_kind: section_draft
artifact_kind: argument_map
artifact_kind: citation_plan
artifact_kind: revision_plan
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: argument_consistency
validator: citation_support_check
validator: paragraph_coherence
validator: claim_scope_check
validator: originality_and_overlap_check
```

重点核查：

1. 核心论点是否稳定。
2. 每个关键判断是否有文献、数据或方法依据。
3. 引用是否真的支持句子中的主张。
4. 章节之间是否重复或跳跃。
5. 结论是否超出证据边界。

### 改进建议

典型 `action_item`：

```text
action_kind: strengthen_claim
action_kind: add_citation
action_kind: narrow_scope
action_kind: reorganize_section
action_kind: remove_unsupported_claim
```

### 最小 MVP

第一版可以支持：

1. 创建 paper_drafting run。
2. 记录提纲 artifact。
3. 记录章节草稿 artifact。
4. 把草稿中的关键 claim 记录为 finding。
5. 对 claim 和 citation_record 做基础 provenance。
6. 输出 revision action items。

### 当前开发缺口

```text
paper_drafting profile
section draft schema
claim extraction helper
claim-to-citation validator
revision action template
```

## 6. 定量分析

`run_kind: quantitative_analysis`

### 目标

支持统计、计量、文本挖掘、网络分析、实验分析等定量研究流程。rbrain 在这里的角色不是替代统计软件，而是记录分析过程、核查结果可复现性、指出方法和解释风险。

Swiftide / rbrain-agent 可以承担流程编排侧：读取数据登记、调用外部脚本、调用统计工具、生成图表并把结果写回。rbrain 承担记录、核查和改进建议。

### 过程记录

核心输入：

```text
dataset: raw_data
dataset: cleaned_data
artifact: analysis_plan
artifact: script
artifact: model_output
artifact: figure
finding: statistical_result
method_note: variable_definition
```

核心过程：

```text
register_dataset
define_variables
write_analysis_plan
run_script
record_outputs
interpret_results
verify_reproducibility
check_method_limits
```

核心产物：

```text
artifact_kind: analysis_plan
artifact_kind: data_dictionary
artifact_kind: cleaning_log
artifact_kind: script
artifact_kind: statistical_table
artifact_kind: figure
artifact_kind: quantitative_report
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: dataset_fingerprint_check
validator: script_reproducibility_check
validator: variable_definition_check
validator: model_assumption_check
validator: result_interpretation_check
```

重点核查：

1. 数据文件是否有 hash 或版本标识。
2. 清洗数据能否追溯到原始数据。
3. 脚本、参数、模型版本是否记录。
4. 表格和图形是否来自同一批输出。
5. 统计显著性是否被过度解释为实质重要性。
6. 因果语言是否超出研究设计。

### 改进建议

典型 `action_item`：

```text
action_kind: add_dataset_hash
action_kind: document_variable
action_kind: rerun_analysis
action_kind: test_model_assumption
action_kind: revise_interpretation
```

### 最小 MVP

第一版建议优先实现：

1. `brain_register_input` 登记数据集和 hash。
2. `record_artifact` 记录脚本、表格、图形。
3. `record_finding` 记录定量发现。
4. `validation_report` 检查数据、脚本、结果是否形成闭环。
5. `provenance_of` 能追溯一个结论来自哪个数据、脚本和输出。

### 当前开发缺口

```text
input registration API
dataset hash/fingerprint support
script artifact profile
result-to-script-to-dataset provenance convention
reproducibility validator
```

## 7. 质性/混合研究

`run_kind: mixed_methods`

### 目标

支持访谈、观察、文本分析、案例研究，以及定量和质性结合的研究流程。rbrain 的重点是记录编码过程、保存解释链条、核查材料与结论之间的关系。

### 过程记录

核心输入：

```text
dataset: interview_transcripts
dataset: field_notes
source: document_source
artifact: codebook
artifact: coded_segments
finding: theme
finding: case_observation
method_note: sampling_strategy
```

核心过程：

```text
register_materials
define_codebook
code_segments
compare_cases
derive_themes
triangulate_evidence
compose_findings
verify_interpretation
```

核心产物：

```text
artifact_kind: codebook
artifact_kind: coded_segment_table
artifact_kind: case_matrix
artifact_kind: theme_map
artifact_kind: mixed_methods_report
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: codebook_consistency
validator: evidence_segment_check
validator: negative_case_check
validator: triangulation_check
validator: interpretation_scope_check
```

重点核查：

1. 主题是否有足够原始材料支撑。
2. 编码是否前后一致。
3. 是否记录反例或负案例。
4. 质性发现和定量发现是否互相支持、冲突或只是并列。
5. 解释是否超出样本和材料边界。

### 改进建议

典型 `action_item`：

```text
action_kind: refine_code_definition
action_kind: add_evidence_segment
action_kind: inspect_negative_case
action_kind: reconcile_mixed_evidence
action_kind: narrow_interpretation
```

### 最小 MVP

第一版可以支持：

1. 记录材料 dataset。
2. 记录 codebook artifact。
3. 记录 theme finding。
4. 每个 theme 至少关联若干 source 或 evidence artifact。
5. 对缺少证据的 theme 生成 action item。

### 当前开发缺口

```text
mixed_methods profile
codebook artifact schema
evidence segment convention
theme-to-evidence validator
negative case validator
```

## 8. 理论建构

`run_kind: theory_building`

### 目标

支持概念提炼、命题生成、理论模型构造、机制解释和理论贡献论证。rbrain 的重点是记录概念从哪里来、理论关系如何形成、反例和边界条件是什么。

### 过程记录

核心输入：

```text
research_memo: concept_note
finding: empirical_pattern
artifact: concept_map
artifact: proposition_list
artifact: mechanism_model
source: theoretical_source
```

核心过程：

```text
collect_concepts
compare_definitions
derive_propositions
build_mechanism_model
check_against_cases
identify_boundary_conditions
compose_theory_note
```

核心产物：

```text
artifact_kind: concept_glossary
artifact_kind: concept_map
artifact_kind: proposition_list
artifact_kind: mechanism_model
artifact_kind: theory_note
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: concept_definition_check
validator: proposition_support_check
validator: mechanism_coherence_check
validator: boundary_condition_check
validator: alternative_explanation_check
```

重点核查：

1. 概念定义是否清晰，是否和既有理论混淆。
2. 命题是否能从材料或文献中获得支持。
3. 理论模型内部关系是否自洽。
4. 是否记录边界条件。
5. 是否比较替代理论解释。

### 改进建议

典型 `action_item`：

```text
action_kind: clarify_concept
action_kind: add_theoretical_source
action_kind: test_proposition
action_kind: specify_boundary_condition
action_kind: compare_alternative_explanation
```

### 最小 MVP

第一版可以支持：

1. 记录 concept glossary artifact。
2. 记录 proposition finding。
3. 记录 mechanism model artifact。
4. 对每个 proposition 追溯到 finding/source。
5. 生成边界条件和替代解释 action item。

### 当前开发缺口

```text
theory_building profile
concept/proposition artifact schema
concept relation convention
alternative explanation validator
boundary condition validator
```

## 9. 方法设计/研究方案

`run_kind: research_design`

### 目标

支持用户从研究问题出发，形成研究方案、数据方案、方法路线、风险预案和实施计划。它应当成为许多后续流程的前置 run。

例如，一个 `research_design` run 可以派生出：

```text
literature_review
quantitative_analysis
mixed_methods
paper_drafting
```

### 过程记录

核心输入：

```text
research_memo: initial_question
finding: known_gap
artifact: preliminary_outline
source: prior_study
method_note: methodological_preference
```

核心过程：

```text
define_research_question
scope_literature
choose_method
define_data_strategy
identify_risks
compose_design
review_design
create_followup_runs
```

核心产物：

```text
artifact_kind: research_question_set
artifact_kind: design_matrix
artifact_kind: data_strategy
artifact_kind: method_plan
artifact_kind: risk_register
artifact_kind: research_design_proposal
```

### 结果核查

建议生成这些 `validation_report`：

```text
validator: question_clarity_check
validator: method_fit_check
validator: data_feasibility_check
validator: scope_and_timeline_check
validator: ethics_and_risk_check
```

重点核查：

1. 研究问题是否可回答。
2. 方法是否适合问题。
3. 数据是否可获得。
4. 时间、样本、工具和权限是否可行。
5. 预期贡献是否和已有研究区分开。
6. 是否需要伦理、隐私或版权审查。

### 改进建议

典型 `action_item`：

```text
action_kind: narrow_research_question
action_kind: revise_method_plan
action_kind: verify_data_access
action_kind: add_risk_mitigation
action_kind: create_followup_run
```

### 最小 MVP

第一版可以支持：

1. 创建 research_design run。
2. 记录研究问题 artifact。
3. 记录方法方案 artifact。
4. 记录风险清单 artifact。
5. 生成设计审查 validation report。
6. 将 follow-up run 作为 action item 或 linked research_run。

### 当前开发缺口

```text
research_design profile
design matrix template
risk register artifact
follow-up run linkage convention
method-fit validator
```

## 10. 建议的补开发顺序

不要先分别实现 8 条孤立流水线。建议先补通用能力：

### 第一轮：统一流程底座

```text
brain_register_input
brain_record_validation_report
brain_record_action_item
run_kind / artifact_kind / validator / action_kind validation
research_run profile loader
```

完成后，8 个流程都能稳定表达“输入-过程-产物-核查-改进”。

### 第二轮：优先支持 200 多期刊场景

```text
journal_brief
hotspot_report
recommendation_push
```

理由：这三类共享 citation_record、literature_corpus、metadata import、topic aggregation、dedupe、brief/report artifact，能复用最多代码。

### 第三轮：支持研究生产流程

```text
research_design
paper_drafting
quantitative_analysis
mixed_methods
theory_building
```

理由：这些流程更依赖用户交互、方法判断和结果核查，适合在通用底座稳定后逐步扩展。

## 11. 与 Swiftide / rbrain-agent 的关系

这些流程可以分成两个层次：

```text
Swiftide / rbrain-agent: 执行研究流程编排
rbrain: 记录、核查、改进和长期记忆
```

适合交给 Swiftide / rbrain-agent 的动作：

```text
检索
批处理
调用模型
执行脚本
生成草稿
生成图表
运行统计
调度多步骤任务
```

rbrain 必须保留的动作：

```text
登记输入
保存产物
记录发现
建立 provenance
生成 validation_report
生成 action_item
维护用户长期研究上下文
```

因此，Swiftide / rbrain-agent 不应替代 rbrain。更合适的关系是：Swiftide 作为流程编排层，通过 MCP 或 Engine API 调用 rbrain；rbrain 作为研究过程账本、核查器和改进建议系统，保存长期事实源和审计链。

ADK-Rust 不再作为当前方案的目标框架。`plan.md` 已将它列为放弃方向，原因是版本真实性和文档稳定性存疑；当前首选路径是 Swiftide，若 Swiftide spike 失败，则退回 `tokio-cron-scheduler + PipelineRunner` 的自实现兜底方案。
