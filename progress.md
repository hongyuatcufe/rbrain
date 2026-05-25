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

## 待办

- [ ] 探讨 dream link 阶段的完善（自动从 synthesis → concept → figure 建立更丰富的图谱）
- [ ] 考虑对已有 concept/figure 页面补充 language 字段（318 页仍为 unknown）
- [ ] MCP 新增工具（brain_think、brain_add_timeline_entry、brain_add_tag、brain_remove_tag、brain_outlinks）的端到端验证
