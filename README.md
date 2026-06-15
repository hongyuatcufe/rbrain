# rbrain

**rbrain** is a Rust-based personal AI knowledge base CLI for academic research. A self-contained binary backed by SQLite, [tantivy](https://github.com/quickwit-oss/tantivy) (BM25 full-text search), and [usearch](https://github.com/unum-cloud/usearch) (vector search) — no system dependencies required.

**rbrain** 是面向学术研究的个人 AI 知识库命令行工具。单一自包含二进制，后端使用 SQLite + tantivy（BM25 全文检索）+ usearch（向量检索），无任何系统依赖。

---

## Features / 功能特性

- **Hybrid search / 混合检索** — BM25 keyword search + semantic vector search, optional LLM query expansion
  BM25 关键词检索 + 语义向量检索，支持 LLM 查询扩展
- **Knowledge graph / 知识图谱** — typed directed links anchored to specific passages (`evidence`, `related`, `supports`, `contrasts`, `develops`)
  带类型的有向链接，可锚定到具体段落
- **Dream Cycle / 自动知识提取流水线** — lint → embed → extract concepts/figures → synthesize concept clusters
  自动化多阶段流水线：检查 → 嵌入 → 提取概念/人物 → 概念聚类综合
- **Think / 深度推理** — structured reasoning over retrieved context: tensions, judgments, open questions
  基于检索上下文的结构化推理：张力、工作判断、开放问题
- **Timeline / 时间线** — dated evidence log attached to any page
  为任意页面附加带日期的证据条目
- **Takes / 诠释片段** — interpretive fragments (`judgment`, `question`, `hypothesis`, `interpretation`) without overwriting page content
  为页面附加诠释性笔记，不覆盖原文
- **MCP server / MCP 服务端** — expose all brain operations to Claude and other MCP-compatible AI assistants
  将所有知识库操作暴露给 Claude 等 MCP 兼容客户端
- **CJK support / 中日韩支持** — language detection (zh-hans, zh-hant, ja, ko, en), CJK-safe chunking, mixed-script search
  语言检测、CJK 安全分块、多语言混合检索

---

## Install / 安装

```bash
git clone https://github.com/hongyuatcufe/rbrain
cd rbrain
cargo build --release -p rbrain-cli

# add to PATH
ln -sf "$PWD/target/release/rbrain" ~/.local/bin/rbrain
```

Requires Rust 1.78+. / 需要 Rust 1.78+。

---

## Quick Start / 快速上手

```bash
# Initialize a project brain / 初始化项目知识库
cd my-research-project
rbrain init

# Configure API keys (.rbrain/config.toml is gitignored)
# 配置 API 密钥（.rbrain/config.toml 默认已加入 .gitignore）
cat > .rbrain/config.toml <<EOF
[qwen]
api_key = "sk-..."
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "text-embedding-v4"

[deepseek]
api_key = "sk-..."
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"

embedding_dim = 1024
EOF

# Import source articles / 导入源文献
rbrain import ./papers/

# Run full dream cycle / 运行完整自动化流水线
# embed → extract concepts/figures → synthesize
rbrain dream

# Search and retrieve / 检索
rbrain search "自主知识体系"
rbrain query "中国教育学的本土化路径" --expand
rbrain get concepts/自主知识体系
```

---

## CLI Reference / 命令参考

### Brain Management / 知识库管理

```bash
rbrain init                    # create .rbrain/ in CWD / 在当前目录初始化
rbrain stats                   # page/chunk/link/embedding counts / 统计信息
rbrain doctor                  # health check / 健康检查
rbrain lint                    # issues: broken links, orphans, unembedded pages
```

### Reading & Writing Pages / 读写页面

```bash
rbrain put <slug> --file <path>          # write page from file / 从文件写入页面
rbrain get <slug>                        # read page + timeline / 读取页面及时间线
rbrain list [--type <type>] [--tag <t>]  # list pages / 列出页面
rbrain import <dir>                      # import all .md files / 批量导入
rbrain export --dir <out> --format md    # export as Markdown / 导出为 Markdown
rbrain export --dir <out> --format json  # export as JSON / 导出为 JSON
```

Page types / 页面类型: `note` | `concept` | `figure` | `synthesis` | `wiki` | `question` | `evidence` | `draft` | `memo` | `period` | `book`

### Search & Retrieval / 检索

```bash
rbrain search "query"                         # BM25 keyword search / BM25 关键词检索
rbrain search "query" --tag t --type concept  # filtered / 过滤检索
rbrain query "question" --expand              # hybrid + query expansion / 混合检索
```

### Knowledge Graph / 知识图谱

```bash
rbrain link <from> <to> --type evidence --from-chunk <id>   # 链接到具体段落
rbrain link <from> <to> --type related
rbrain unlink <from> <to> [--type <t>]
rbrain backlinks <slug>          # incoming links / 反向链接
rbrain links <slug>              # outgoing links / 出向链接
rbrain graph-query <slug> --depth 2 --direction both
rbrain orphans                   # pages with no incoming links / 孤立页面
rbrain extract --all             # extract [[wikilinks]] from content / 提取双链
```

Link types / 链接类型: `evidence` | `related` | `supports` | `contrasts` | `develops`

### Timeline / 时间线

```bash
rbrain timeline <slug> --text "..." --date "2026-05-15" --source "<slug> chunk:<id>"
# --date defaults to today / --date 默认为今天
```

### Takes / 诠释片段

```bash
rbrain take <slug> "judgment text" --kind judgment
rbrain takes <slug>              # list all takes / 列出所有诠释
```

`--kind`: `judgment` | `question` | `hypothesis` | `interpretation`

### Tags / 标签

```bash
rbrain tag <slug> <tag>
rbrain untag <slug> <tag>
rbrain tags <slug>
```

### Synthesis / 综合分析

```bash
rbrain think "topic" --limit 12 --expand   # deep reasoning / 深度推理
rbrain think "topic" --save                # saves as synthesis/<slug>
rbrain generate "topic" --limit 12 --expand  # wiki-style summary / 文献综述草稿
rbrain generate "topic" --save             # saves as wiki/<slug>
```

### Embeddings / 向量嵌入

```bash
rbrain embed --stale     # re-embed stale pages / 重新嵌入过期页面
rbrain embed --all       # re-embed everything / 重新嵌入全部
rbrain sync --embed      # sync filesystem → DB then re-embed / 同步后重新嵌入
```

### Dream Cycle / 自动化流水线

```bash
rbrain dream                        # full pipeline / 完整流水线
rbrain dream --stage lint           # Phase 1: lint
rbrain dream --stage embed          # Phase 2: embed stale pages / 嵌入过期页面
rbrain dream --stage extract        # Phase 3: extract concepts & figures / 提取概念和人物
rbrain dream --stage synthesize     # Phase 4: synthesize concept clusters / 概念聚类综合
```

**Phase 3 — Extract / 提取阶段**：对每篇未处理的 `note`，调用 DeepSeek 提取概念、学者/人物和时间线事件。自动创建 `concepts/<slug>` 和 `figures/<slug>` 页面；关联人物的事件写入人物页，其他事件写入 `research/evidence/events/<source-slug>` 派生页。`raw/` 来源文献不会被 dream 改写。处理记录写入 `dream_metadata`（幂等，不重复处理）。

For each unprocessed `note`, calls DeepSeek to extract concepts, figures, and timeline events. Events associated with people are written to figure pages; other events are written to derived `research/evidence/events/<source-slug>` pages. Dream never rewrites `raw/` source documents. Idempotent via `dream_metadata`.

**Phase 4 — Synthesize / 综合阶段**：对有 3 篇以上源文章反向链接的概念，自动生成结构化文献综合页面，保存至 `synthesis/<concept-slug>`，并建立 `develops`（→ 概念）和 `evidence`（→ 源文章）链接。源文章更新后自动重新综合。

For each concept with 3+ source note backlinks, generates a structured literature synthesis at `synthesis/<concept-slug>`. Auto-links synthesis → concept (`develops`) and synthesis → source notes (`evidence`). Re-synthesizes when sources are updated.

---

## MCP Server / MCP 服务端

rbrain 通过 Model Context Protocol 将所有操作暴露给 Claude Code 等 MCP 客户端。

```bash
rbrain serve mcp                  # stdio mode (for Claude Code) / stdio 模式
rbrain serve mcp --http 127.0.0.1:3456  # local HTTP mode / 本地 HTTP 模式
rbrain serve mcp --http 0.0.0.0:3456 --allow-remote  # explicit remote bind / 显式远程绑定
```

HTTP mode exposes mutation tools without authentication and therefore only accepts loopback
addresses (`127.0.0.1`, `[::1]`, or `localhost`) by default. Use `--allow-remote` only behind
a trusted boundary such as a local tunnel, firewall, or reverse proxy with authentication.

### MCP Tools / MCP 工具列表

| Tool | Description |
|------|-------------|
| `brain_put` | Write or update a page / 写入或更新页面 |
| `brain_get` | Read a page / 读取页面 |
| `brain_delete` | Delete a page / 删除页面 |
| `brain_list` | List pages with optional type/tag filter / 列出页面 |
| `brain_query` | Hybrid semantic search / 混合语义检索 |
| `brain_think` | Deep reasoning synthesis on a topic / 深度推理 |
| `brain_generate` | Search + LLM wiki synthesis / 文献综述生成 |
| `brain_link` | Create a typed graph link / 建立知识图谱链接 |
| `brain_unlink` | Remove a link / 删除链接 |
| `brain_backlinks` | Get incoming links / 反向链接 |
| `brain_outlinks` | Get outgoing links / 出向链接 |
| `brain_graph` | Traverse graph neighborhood / 图谱邻域遍历 |
| `brain_orphans` | List pages with no incoming links / 孤立页面 |
| `brain_add_timeline_entry` | Add a dated entry to a page / 添加时间线条目 |
| `brain_add_tag` | Add a tag / 添加标签 |
| `brain_remove_tag` | Remove a tag / 删除标签 |
| `brain_stats` | Brain statistics / 统计信息 |

### Claude Code Setup / Claude Code 配置

Add to `.claude/settings.json` / 添加到 `.claude/settings.json`：

```json
{
  "mcpServers": {
    "rbrain": {
      "type": "stdio",
      "command": "rbrain",
      "args": ["serve", "mcp"]
    }
  }
}
```

---

## Architecture / 架构

```
rbrain/
├── crates/
│   ├── rbrain-cli/      # CLI entry point (clap) / 命令行入口
│   ├── rbrain-engine/   # Core logic: dream, search, linking, synthesis / 核心逻辑
│   ├── rbrain-core/     # Page model, language detection, markdown parsing / 页面模型
│   ├── rbrain-db/       # SQLite schema and queries (sqlx) / 数据库层
│   ├── rbrain-search/   # Tantivy BM25 + usearch vector index / 检索层
│   ├── rbrain-llm/      # DeepSeek chat + Qwen embedding clients / LLM 客户端
│   ├── rbrain-mcp/      # MCP server (stdio + HTTP) / MCP 服务端
│   └── rbrain-worker/   # Background job queue / 后台任务队列
└── rbrain-research-cli/ # Claude Code skill for research workflows / 研究工作流技能
```

**Embedding / 嵌入模型**: Qwen `text-embedding-v4` (1024-dim) via DashScope  
**LLM**: DeepSeek `deepseek-chat` — extraction, synthesis, think, generate  
**Storage / 存储**: SQLite (pages, chunks, links, dream_metadata) + usearch (vectors) + tantivy (BM25)

---

## Brain Auto-Discovery / 知识库自动发现

rbrain 从当前目录向上遍历自动定位知识库：
1. 找到 `.rbrain/` → 使用该项目本地知识库
2. 未找到 → 回退到 `~/.rbrain/`

rbrain walks up from CWD to discover the active brain:
1. finds `.rbrain/` → uses project-local brain
2. falls back to `~/.rbrain/`

这意味着 `cd my-project && rbrain stats` 无需任何参数即可使用项目知识库。

---

## Skill / Claude Code 技能

研究工作流技能文件位于 [`rbrain-research-cli/SKILL.md`](rbrain-research-cli/SKILL.md)，涵盖完整工作流：知识库发现、检索、写入、图谱链接、Dream Cycle、Signal Detection 和 MCP 使用模式。

A Claude Code skill for research workflows is at [`rbrain-research-cli/SKILL.md`](rbrain-research-cli/SKILL.md). Covers brain discovery, retrieval, writing, graph linking, dream cycle, signal detection, and MCP usage patterns.
