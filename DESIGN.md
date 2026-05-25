---
{}
---
# rbrain Design Document

Cross-cutting principles and locked technical decisions.

## Architecture Overview

Single static binary with three interfaces: CLI, MCP server, background worker. All share a common Engine that orchestrates storage, search, and LLM calls.

```
         ┌──────────────────────────────┐
         │   Brain repo (~/brain/)      │
         │   Markdown, git-tracked      │
         └────────────┬─────────────────┘
                      │ sync
                      ▼
┌─────────────────────────────────────────────┐
│            SQLite (~/.rbrain/brain.db)       │
│   Pages, chunks, links, jobs, embeddings    │
└────────────┬────────────────────────────────┘
             │ rebuild
             ▼
┌────────────────────┐    ┌─────────────────┐
│  usearch (HNSW)    │    │  tantivy (BM25) │
│  ~/.rbrain/vectors │    │  ~/.rbrain/tantivy │
└────────────────────┘    └─────────────────┘
```

SQLite owns truth. Tantivy and usearch are derived indices, rebuildable from SQLite without API calls.

## Locked Decisions

| Component | Choice | Rationale |
|-----------|--------|-----------|
| SQLite mode | WAL | Single writer, many readers. Normal sync. |
| Embeddings | Qwen text-embedding-v4, dim=1024 | Strong CJK, cheaper than OpenAI. Locked dimension. |
| LLM chat | DeepSeek V4 Flash | Cheap, strong, OpenAI-compatible endpoint. |
| RRF fusion | k=60 | Default from gbrain, configurable. |
| CJK tokenizer | lindera-tantivy | Proper morphology over trigram. IPADIC/CC-CEDICT/ko-dic. |
| Chinese normalization | ferrous-opencc | Pure Rust. Traditional→simplified at index and query time. |
| Embedding storage | SQLite BLOB + usearch | Both. SQLite for durability, usearch for fast HNSW. |
| Rust edition | 2024 | Workspace standard. |

Pin exact versions for: tantivy, lindera-tantivy, usearch, rmcp, sqlx, ferrous-opencc. These break across minor versions.


