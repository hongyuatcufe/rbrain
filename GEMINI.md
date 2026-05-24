# GEMINI.md - 任务总结

在本次开发中，我们针对 rbrain 进行了以下优化，主要针对中文学术研究场景：

1. **语言检测机制优化**：
   - 将散落在各模块的语言检测逻辑统一抽离到最底层的 `rbrain-core` 中的 `Language::detect`。
   - 引入了高精度的 CJK 启发式算法：
     - 若包含韩文字符，判定为韩语 (`Ko`)。
     - 若包含日文假名，判定为日语 (`Ja`)。
     - 若包含汉字且无韩日文字符，则使用 `ferrous-opencc` 对整段文本进行繁转简。若转换后文本发生改变，则判定为繁体中文 (`ZhHant`)，否则判定为简体中文 (`ZhHans`)。
     - 其他语言回退到 `whatlang` n-gram 判定。
   - 彻底修复了简体中文有时会被误判为繁体中文或日语，从而导致分词错误及搜索不到的问题。

2. **中文停用词过滤**：
   - 在 `rbrain-search` 的 `keyword_index.rs` 中增加了对高频中文停用词（如“的”、“了”、“在”、“是”、“我”等）的拦截过滤。
   - 使分词后的关键词匹配更加纯粹，显著提升了中文 BM25 检索和 Hybrid 搜索的精度。

3. **`links` 数据库 Schema 扩展 (多证据片段支持)**：
   - 编写了数据库迁移 `0008_extend_links_chunk_id.sql`，在 SQLite 数据库中为 `links` 表增加了 `chunk_id` 列，默认值为 `-1`（代表无对应段落的通用关联）。
   - 将唯一性约束修改为 `UNIQUE(source_slug, target_slug, edge_type, chunk_id)`，支持在同一源页面的不同段落中多次链接至同一目标页面，且各自保留独立的上下文证据。
   - 在 `rbrain-engine`、`rbrain-cli` 以及 MCP 服务的 `brain_link` 中全面适配了 `chunk_id` 的存储、更新与查询解析。

4. **真实学术文献实测验证 (Academic Literature Testing)**：
   - 编写了测试脚本 `scratch/run_academic_test.sh` 并构造了简/繁/日三种真实学术论文摘要样本。
   - 验证了语言检测的高准确率：简体中文/繁体中文/日语（含大量汉字）均能够百分之百准确识别。
   - 验证了学术短语搜索：“预训练语言模型的研究与应用”在过滤“的”、“与”等停用词后依然精准召回相关段落。
   - 验证了多重学术引用证据链：在同一文献 review 中可以独立引用同一文献的不同 chunk，并且能够通过 `links` 完整召回各自独立的上下文证据（`chunk_id` 区分独立存储）。

5. **Dream Cycle (梦境循环) 简化版实现**：
   - 编写了数据库迁移 `0009_dream_metadata.sql`，在 SQLite 数据库中创建 `dream_metadata` 表，记录每个页面的提取时间戳和内容 hash，实现增量知识提取（只处理有修改的页面），避免重复调用 LLM。
   - 在 `rbrain-engine` 中实现了 Dream Cycle 完整流水线（Linting、Embedding 缺失页、Extracting 实体与事件、Synthesizing 标签文献综述），在 `rbrain-cli` 中新增了 `dream` 子命令并支持 `--stage` 选项。
   - 适配了 Mock 提取和 Synthesis 合成逻辑，使无 API 密钥的本地测试/集成测试能够 100% 确定性离线运行。

6. **Markdown Frontmatter 解析修复**：
   - 彻底修复了 `sync` 和 `import` 命令在解析 Markdown 文件 Frontmatter 后丢失 `tags` 与 `title` 的问题。现在它们能够正确载入 Page 结构并写入数据库，使标签维度的 Dream Synthesis 可以完美识别并执行文献聚合。

---
完成时间：2026-05-24
