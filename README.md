# rbrain

**rbrain** is a Rust port of [gbrain](../gbrain) — a personal AI knowledge base designed for academic research. Where gbrain runs on TypeScript + PostgreSQL/PGLite, rbrain is a single self-contained binary backed by SQLite, tantivy, and usearch.

**rbrain** 是 [gbrain](../gbrain) 的 Rust 移植版本——一个面向学术研究的个人 AI 知识库。gbrain 基于 TypeScript + PostgreSQL/PGLite，rbrain 是单一自包含二进制文件，后端使用 SQLite + tantivy + usearch，无任何系统依赖。

---

## Why Rust? / 为什么用 Rust？

| | gbrain | rbrain |
|---|---|---|
| Runtime | Node.js | Single static binary |
| Database | PostgreSQL / PGLite | SQLite (WAL mode) |
| Deployment | `npm install` + DB setup | Copy one binary |
| CJK search | `ILIKE` fallback | lindera morphology (IPADIC / CC-CEDICT / ko-dic) |
| Vector index | pgvector (HNSW) | usearch (HNSW, pure Rust) |
| Embedding model | OpenAI / Voyage | Qwen text-embedding-v4 (strong CJK) |
| Memory footprint | ~200 MB (Node heap) | ~15 MB resident |

---

## Architecture / 架构

```
  Research project folder /
  ├── notes/          ← Markdown files (git-tracked)
  ├── books/          ← Imported PDFs / text
  └── .rbrain/        ← Project-local brain (auto-discovered)
       ├── brain.db         SQLite — pages, chunks, links, jobs
       ├── vectors.usearch  HNSW vector index
       └── tantivy/         BM25 keyword index (per language)

  Global fallback: ~/.rbrain/   (used when no .rbrain/ found in CWD or ancestors)
  Global config:   ~/.rbrain/config.toml  (API keys, model settings)
```

SQLite is the **single source of truth**. Tantivy and usearch are derived indices — wipe and rebuild at any time without re-calling any API.

```
  ┌──────────┐  put/sync  ┌─────────────┐  embed   ┌──────────────────┐
  │ Markdown │──────────▶│   SQLite    │─────────▶│ usearch + tantivy│
  │  files   │           │  (truth)    │          │  (derived index) │
  └──────────┘           └─────────────┘          └──────────────────┘
                                │
                    ┌───────────┼───────────────┐
                    ▼           ▼               ▼
                  CLI         MCP           Worker
              (rbrain)   (stdio/HTTP)    (job queue)
```

### Crate layout / Crate 结构

| Crate | Role |
|---|---|
| `rbrain-core` | Shared types, traits (`Embedder`, `VectorStore`, `KeywordIndex`), config |
| `rbrain-db` | SQLite connection pool + migrations |
| `rbrain-search` | tantivy keyword index, usearch vector store, text chunker |
| `rbrain-llm` | Qwen embedder, DeepSeek chat client, mock embedder |
| `rbrain-engine` | Orchestration layer — hybrid search, RRF fusion, wiki generation |
| `rbrain-worker` | Async job queue backed by SQLite |
| `rbrain-mcp` | MCP server (stdio + HTTP), 8 tools |
| `rbrain-cli` | `rbrain` binary, all subcommands |

---

## Feature Comparison with gbrain / 与 gbrain 功能对照

### ✅ Implemented / 已实现

| Feature | gbrain | rbrain |
|---|---|---|
| Page CRUD (put / get / delete / list) | ✅ | ✅ |
| Markdown import + frontmatter | ✅ | ✅ |
| File-system sync (detect new/changed/deleted) | ✅ | ✅ |
| Text chunking (recursive, markdown-aware) | ✅ | ✅ |
| Embedding (batch, retry) | ✅ | ✅ Qwen text-embedding-v4 |
| Vector search (HNSW) | ✅ pgvector | ✅ usearch |
| Keyword search (BM25) | ✅ FTS | ✅ tantivy |
| CJK tokenization | ✅ ILIKE fallback | ✅ lindera (IPADIC / CC-CEDICT / ko-dic) |
| Chinese Traditional→Simplified normalization | ❌ | ✅ ferrous-opencc |
| Hybrid search (RRF fusion) | ✅ | ✅ |
| Query expansion (LLM) | ✅ | ✅ DeepSeek |
| Backlink boost | ✅ | ✅ |
| Knowledge graph (links / backlinks / traverse) | ✅ | ✅ |
| Wiki generation (`generate`) | ✅ Dream cycle | ✅ `rbrain generate` |
| MCP server (stdio) | ✅ | ✅ |
| MCP server (HTTP) | ✅ OAuth 2.1 | ✅ (no auth yet) |
| Background job queue | ✅ minions | ✅ rbrain-worker |
| Health check + auto-fix | ✅ | ✅ `rbrain doctor` |
| Brain statistics | ✅ | ✅ `rbrain stats` |
| Project-local brain (`.rbrain/`) | ❌ | ✅ git-style auto-discovery |
| Offline testing (mock embedder) | ❌ | ✅ `--mock-embed` |

### MCP Tools / MCP 工具

| Tool | Description | gbrain | rbrain |
|---|---|---|---|
| `brain_query` | Hybrid search → ranked chunks with text | `search` / `query` | ✅ |
| `brain_get` | Fetch full page content | `get_page` | ✅ |
| `brain_put` | Create / update page | `put_page` | ✅ |
| `brain_delete` | Delete page + chunks | `delete_page` | ✅ |
| `brain_list` | List pages (type/tag filter) | `list_pages` | ✅ |
| `brain_graph` | Traverse knowledge graph | `traverse_graph` | ✅ |
| `brain_backlinks` | Find pages linking here | `get_backlinks` | ✅ |
| `brain_stats` | Knowledge base statistics | `get_stats` | ✅ |
| `brain_generate` | Search + LLM → wiki page | Dream cycle (subagent) | ✅ |
| `submit_job` / `get_job` / etc. | Job queue management | ✅ | ✅ `jobs` subcommand |
| `think` | Deep reasoning (Opus) | ✅ | ❌ planned |
| `takes_*` | Idea management | ✅ | ❌ planned |
| `find_contradictions` | Anomaly detection | ✅ | ❌ planned |

### ⏳ In Progress / 开发中

| Feature | Notes |
|---|---|
| `rbrain get` truncated output | Long pages currently dump full content |
| Embed progress bar | Shows `[1/N]` but no ETA or percentage |
| `rbrain sync` → auto re-embed | Sync detects changes but doesn't trigger re-embedding |
| Dream cycle | Autonomous multi-step wiki synthesis via subagent loop |

---

## Installation / 安装

### Build from source / 从源码构建

```bash
# Requires Rust 1.85+
git clone <this-repo>
cd rbrain
cargo build --release
# Binary at: target/release/rbrain
```

### Quick test (no API key needed) / 快速测试（无需 API key）

```bash
mkdir my-project && cd my-project
rbrain init
echo "# My first note\nThis is a test." | rbrain put "first-note" --mock-embed
rbrain search "test" --mock-embed
```

---

## Configuration / 配置

Global config at `~/.rbrain/config.toml`:

```toml
[qwen]
api_key = "sk-..."          # DashScope API key (for embedding)
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model    = "text-embedding-v4"

[deepseek]
api_key  = "sk-..."          # DeepSeek API key (for chat / generate)
base_url = "https://api.deepseek.com/v1"
model    = "deepseek-chat"

embedding_dim = 1024
```

Environment variables override config (prefix `RBRAIN_`):

```bash
export RBRAIN_DB_PATH=/custom/path/brain.db
export RBRAIN_VECTORS_PATH=/custom/path/vectors.usearch
export RBRAIN_TANTIVY_DIR=/custom/path/tantivy
```

---

## Usage / 使用

### Project-local brain / 项目本地知识库

```bash
cd /path/to/my-research
rbrain init                    # Creates .rbrain/ + adds to .gitignore

rbrain import .                # Import all .md files in current dir
rbrain embed --all             # Embed everything (calls Qwen API)

rbrain query "孔子教育思想"    # Hybrid search, results grouped by page
rbrain search "礼治"           # Keyword-only search

rbrain generate "孔子教育思想" --save   # LLM wiki → saved to brain
```

### Knowledge graph / 知识图谱

```bash
rbrain extract --all           # Parse [[wikilinks]] → graph edges
rbrain backlinks "孔子"        # Who links to this page?
rbrain graph-query "孔子" --depth 3 --direction out
```

### MCP server (for Claude) / MCP 服务（配合 Claude）

```bash
# Add to Claude Desktop config:
rbrain serve mcp

# HTTP mode:
rbrain serve mcp --http 0.0.0.0:8080
```

Claude can then use `brain_query`, `brain_put`, `brain_generate`, etc. as tools.

### CLI reference / 命令参考

```
rbrain init          Initialise project-local brain (.rbrain/)
rbrain put <slug>    Create/update page (auto re-embeds)
rbrain get <slug>    Retrieve page content
rbrain delete <slug> Delete page + embeddings
rbrain list          List pages (--type, --tag, --limit)
rbrain import <dir>  Import markdown directory
rbrain sync          Sync file system → database
rbrain embed         Embed pages (--all or single slug)
rbrain extract       Extract links into graph (--all)
rbrain query         Hybrid search (grouped by page)
rbrain search        Keyword search (grouped by page)
rbrain generate      Search + LLM → wiki page (--save)
rbrain graph-query   Traverse knowledge graph
rbrain backlinks     Find incoming links
rbrain doctor        Health check (--fix)
rbrain stats         Brain statistics
rbrain serve mcp     Start MCP server
rbrain jobs          Job queue management
```

---

## Development / 开发状态

See [progress.md](./progress.md) for detailed task tracking.

**Current phase:** Core features complete (Phase 1–4), MCP server operational, project-local brain working. Next: polish items and dream cycle.

---

## License

MIT

---

## Relation to gbrain / 与 gbrain 的关系

rbrain is developed alongside gbrain as a Rust-native alternative for environments where Node.js is unavailable or where a single self-contained binary is preferred (e.g., server deployment, embedded research workstations). The two systems share the same conceptual model (pages → chunks → embeddings + graph) and are designed to be interoperable at the data level (SQLite schema is compatible, MCP tools mirror gbrain's operation names).

rbrain 作为 gbrain 的 Rust 原生替代方案并行开发，适用于无法运行 Node.js 的环境，或需要单一二进制文件部署的场景（如服务器端部署、离线研究工作站）。两个系统共享相同的概念模型（pages → chunks → embeddings + graph），并在数据层面保持互操作性（SQLite schema 兼容，MCP 工具命名与 gbrain 操作名称对应）。
