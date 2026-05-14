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

---

## P1: Source-of-Truth Ladder

```
markdown repo (~/brain/)   ← user-editable, git-tracked, durable
        │
        │  rbrain sync / import
        ▼
SQLite (~/.rbrain/brain.db) ← canonical truth, including raw embeddings
        │
        │  rbrain rebuild
        ▼
usearch + tantivy           ← derived indices. Wipe and rebuild any time.
```

Embeddings live in SQLite as `BLOB` in `chunks.embedding`. If usearch is corrupted, rebuild from SQLite without re-calling the API. Trades ~400MB per 100k chunks for resilience.

---

## P2: Write Protocol

`put_page` touches multiple stores. Order matters:

1. **Validate** input (slug, content, frontmatter)
2. **Compute hashes** for change detection
3. **Acquire file lock** on `<slug>.md.lock` (P8)
4. **Write markdown atomically** — write to `.tmp`, fsync, rename. Abort if on-disk hash differs from DB row (external edit in flight)
5. **SQLite transaction**:
   - Upsert `pages` row
   - Delete old `chunks`, insert new with `has_embedding=0`
   - Delete old `links`, insert new
   - Commit
6. **Release file lock**
7. **Spawn background tasks**: embed + keyword index in parallel via `tokio::try_join!`
8. **Job handlers** update SQLite embedding BLOB, insert to usearch/tantivy, flip flags

If step 8 fails for any chunk, `has_embedding` stays 0. `rbrain doctor --fix` finds and re-runs.

---

## P3: Config Management

Single source: `~/.rbrain/config.toml` → env vars → defaults. Priority order:

```rust
Config::load() // toml -> env -> defaults
```

CLI constructs `Config`, passes to `Engine::new(config)`. Nothing else reads env directly.

---

## P4: Error Taxonomy

- Each crate: typed error enum via `thiserror::Error` for public API
- `anyhow::Result` only in CLI binary boundary and tests
- Retry decisions inspect variants, not parse strings:
  ```rust
  if matches!(e, EmbedError::RateLimited(_) | EmbedError::Api { status: 5.., .. })
  ```

---

## P5: Logging Conventions

Every I/O method wrapped in `#[tracing::instrument]` span. Required fields:

| Field | When |
|-------|------|
| `slug` | Page operations |
| `chunk_id` | Chunk operations |
| `lang` | Language-routed operations |
| `provider` | API calls (`qwen` | `deepseek`) |
| `attempt` | Retried operations |

Example:
```rust
#[tracing::instrument(skip(self, content), fields(slug = %slug))]
```

All log events include `event` field for structured output:
```json
{"event":"page.put","slug":"concepts/capitalism","duration_ms":45}
```

---

## P6: Transaction Rule

Any write touching multiple SQLite tables: single `sqlx::Transaction`. No exceptions.

---

## P7: Frontmatter Preservation

- `rbrain sync` reads files as-is. Never rewrites.
- `rbrain put` writes canonical form (sorted keys, normalized whitespace)
- `content_hash` stores hash of canonical form — whitespace-only changes don't trigger re-import

---

## P8: Concurrent Write Handling

Before writing markdown:

1. Acquire advisory lock on `<slug>.md.lock` via `flock` (Unix) or named mutex (Windows)
2. If lock not acquired within 5s: return `ConflictError("another process is writing")`
3. Release lock after commit

Prevents two `put_page` calls corrupting the file. Reads don't acquire locks.

---

## P9: Scope Boundaries (V1 Exclusions)

| ID | Exclusion | Rationale |
|----|-----------|-----------|
| G1 | Real-time sync (inotify) | Explicit `rbrain sync` only |
| G2 | Multi-brain support | Single `~/brain/` directory |
| G3 | Collaborative editing | Single user, single machine |
| G4 | Mobile/secondary clients | HTTP for remote MCP only |
| G5 | Automatic embedding model upgrade | `dim=1024` locked. Manual rebuild. |
| G6 | Page versioning | Git provides history |
| G7 | PDF/web import | Markdown files only |
| G8 | Image/OCR support | Text-only embeddings |

---

## Trait Boundaries

Define traits only where implementations swap:

```rust
trait Embedder: Send + Sync { dimension(), embed_one(), embed_batch(), verify_deterministic() }
trait VectorStore: Send + Sync { upsert(), delete(), search(), save(), load() }
trait KeywordIndex: Send + Sync { upsert(), delete(), search(), commit() }
```

Initial impls: QwenEmbedder, UsearchStore, TantivyIndex. Engine is concrete, not trait. Use `Arc<dyn Trait>` for shared ownership.

---

## Performance Budget

| Operation | Target |
|-----------|--------|
| `rbrain get <slug>` | <50ms |
| `rbrain query` (non-LLM) | p95 <500ms |
| `rbrain import 100 pages` | <5 min |
| `rbrain doctor` (10k pages) | <2s |
| MCP tool call | <500ms |

---

## LLM Failure Fallback

Every LLM call in query path has 2s timeout. On timeout/error: log degradation, proceed with vector+keyword on raw query. User gets slightly worse result, not no result.

---

## Dependencies (Pin Exact)

```
tantivy = "=0.22.0"
lindera-tantivy = "=0.38.0"
usearch = "=3.x.y"
rmcp = "=x.y.z"
sqlx = "=0.8.x"
ferrous-opencc = "=x.y.z"
```

Check current stable versions before pinning. These crates change APIs across minors.