# rbrain Development Progress

## Phase Status

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Core schema + CRUD (put/get/delete/list) | ✅ Complete |
| 2 | Bulk import + file-system sync | ✅ Complete |
| 3 | Text chunking + embedding (Qwen API) | ✅ Complete |
| 4 | Hybrid search (usearch HNSW + tantivy BM25 + RRF) | ✅ Complete |
| 5 | CJK morphological tokenization (lindera) | ✅ Complete |
| 6 | Knowledge graph (links/backlinks/traverse) | ✅ Complete |
| 7 | Wiki generation (`rbrain generate` + `brain_generate`) | ✅ Complete |
| 8 | MCP server (stdio + HTTP, 9 tools) | ✅ Complete |
| 9 | Background job queue (`rbrain-worker`) | ✅ Complete |
| 10 | Project-local brain (`.rbrain/` git-style discovery) | ✅ Complete |

---

## Implemented Features (Detail)

### Data Layer
- [x] SQLite in WAL mode, single source of truth
- [x] Pages table: slug, title, page_type, tags, compiled_truth, timeline, language
- [x] Chunks table: page_slug, chunk_index, text, embedding (BLOB), lang
- [x] Links table: source_slug, target_slug, edge_type, context
- [x] Jobs table: type, payload, status, result, error, timestamps
- [x] Auto-migration on first open

### Search
- [x] Vector search via usearch HNSW (`rbrain-search/src/vector_store.rs`)
- [x] Keyword search via tantivy BM25 (`rbrain-search/src/keyword_index.rs`)
- [x] CJK pre-segmentation with lindera (IPADIC/CC-CEDICT/ko-dic)
- [x] Chinese Traditional→Simplified normalization (ferrous-opencc)
- [x] RRF fusion (`rbrain-engine/src/engine.rs`)
- [x] LLM query expansion via DeepSeek (`expand=true`)
- [x] Backlink boost in hybrid ranking
- [x] Search results with chunk text (`search_with_context`)
- [x] Grouped output in CLI (`print_grouped_results`)

### Embedding
- [x] Qwen text-embedding-v4 (1024-dim, strong CJK)
- [x] Batch embedding with retry
- [x] MockEmbedder for offline testing (`--mock-embed`)
- [x] `rbrain put` auto re-embeds after save

### Knowledge Graph
- [x] `[[wikilink]]` extraction from Markdown
- [x] `rbrain extract --all` builds graph edges
- [x] `rbrain graph-query <slug> --depth --direction`
- [x] `rbrain backlinks <slug>`
- [x] MCP `brain_graph` + `brain_backlinks` tools

### Wiki Generation
- [x] DeepSeek chat client (`from_config` reads API key)
- [x] `rbrain generate <topic> [--save] [--expand]`
- [x] MCP `brain_generate` tool

### MCP Server (9 tools)
- [x] `brain_query` — hybrid search with RRF
- [x] `brain_get` — fetch full page
- [x] `brain_put` — create/update page
- [x] `brain_delete` — delete page + chunks
- [x] `brain_list` — list with type/tag filter
- [x] `brain_graph` — knowledge graph traversal
- [x] `brain_backlinks` — find incoming links
- [x] `brain_stats` — knowledge base statistics
- [x] `brain_generate` — search + LLM → wiki

### Infrastructure
- [x] Project-local brain: git-style walk-up finds `.rbrain/`
- [x] `rbrain init` creates `.rbrain/` + updates `.gitignore`
- [x] Global fallback: `~/.rbrain/`
- [x] Figment config (flat TOML + env var override)
- [x] Background job queue (`rbrain-worker`, SQLite-backed)
- [x] `rbrain doctor --fix` health check
- [x] `rbrain stats` statistics

---

## Pending Items

### Item 6 — `rbrain get` truncated output (Easy)

**Problem**: `rbrain get <slug>` currently dumps the full page content, which is unwieldy for large pages (books, long notes).

**Design**:
- Default: truncate at 2000 chars, print `… (use --full to see complete content)`
- `--full` flag: print everything
- Optional `--lines N` for custom limit

**Files**: `crates/rbrain-cli/src/main.rs` (Get command, ~5 lines change)

**Effort**: ~30 min

---

### Item 7 — Embed progress bar (Medium)

**Problem**: `rbrain embed --all` shows `[1/N]` prefix but no percentage, ETA, or visual bar. For large corpora (1000+ chunks) it feels stuck.

**Design**:
- Use [`indicatif`](https://crates.io/crates/indicatif) crate
- Progress bar format: `[===>    ] 3/10 pages  42/137 chunks  ETA 00:23`
- Two-level progress: outer bar for pages, inner spinner for current-page chunks
- Suppress bar when stdout is not a TTY (e.g., CI / piped output)

**Files**:
- `Cargo.toml` workspace deps: add `indicatif = "0.17"`
- `crates/rbrain-cli/src/main.rs`: `embed` command wrapper
- `crates/rbrain-engine/src/engine.rs`: `embed_all()` yield progress via callback or channel

**Effort**: ~2 hrs

---

### Item 8 — `rbrain sync` auto re-embed changed pages (Medium)

**Problem**: `rbrain sync` detects new/changed/deleted files and updates SQLite, but does **not** re-embed the changed pages. The user must run `rbrain embed --all` manually afterward.

**Design**:
1. `sync` command: after DB update, collect `changed_slugs` and `new_slugs`
2. If `--embed` flag passed (or auto-detect): call `engine.chunk_and_embed_page()` for each changed slug
3. Log: `Synced 3 pages: 2 embedded, 1 skipped (no embedder)`

**Files**:
- `crates/rbrain-engine/src/engine.rs`: `sync_dir()` returns `SyncResult { added, changed, deleted }`
- `crates/rbrain-cli/src/main.rs`: `sync` command handles `--embed` flag

**Consideration**: Large repos may have many changed files. Add `--embed` as opt-in flag rather than default to avoid accidental API overuse.

**Effort**: ~3 hrs

---

### Item 9 — Dream Cycle (Complex)

**Problem**: gbrain has a `dream` / `synthesize` flow (see `gbrain/src/synthesize.ts`) that autonomously:
1. Picks underlinked wiki topics from the graph
2. Runs `brain_query` to gather evidence
3. Calls LLM to synthesise a wiki page
4. Saves the result and repeats with new topics discovered in the generated text

This creates a self-improving knowledge base over time.

**Design for rbrain**:

```
rbrain dream [--cycles N] [--topic <seed>] [--dry-run]
```

**Architecture** (multi-step agentic loop):

```
Dream loop iteration:
  1. Pick topic
     - If --topic given: use it for first cycle, then extract [[links]] from generated pages
     - Otherwise: pick least-connected page from graph (graph_density outlier)
  2. Gather evidence
     - engine.search_with_context(topic, lang, k=15, expand=true)
  3. Synthesise
     - deepseek.chat(DREAM_SYSTEM_PROMPT, context + topic)
     - Parse generated [[wikilinks]] to discover next topics
  4. Save + extract links
     - engine.put_page(wiki_page)
     - engine.extract_links(&wiki_page)  → queue newly mentioned topics
  5. Continue
     - Add newly discovered topics to a priority queue (BFS/BFS)
     - Repeat until N cycles done or queue empty
```

**System prompt** (`DREAM_SYSTEM_PROMPT`):
```
You are a scholarly wiki editor for an academic knowledge base on education history.
Given the retrieved source materials, write a concise Markdown wiki page.
- Use [[wikilink]] syntax to link related concepts
- Cite sources as (slug)
- Keep to 400-800 words
- Language: match the source materials
```

**Job queue integration**:
- Each cycle is a `DreamJob` submitted to `rbrain-worker`
- `rbrain dream --background` submits a batch and returns immediately
- `rbrain jobs` shows progress

**Files**:
- `crates/rbrain-engine/src/engine.rs`: `dream_cycle()` method
- `crates/rbrain-cli/src/main.rs`: `Dream` command
- `crates/rbrain-worker/src/jobs.rs`: `DreamJob` handler
- New: `crates/rbrain-engine/src/dream.rs` (dream loop logic)

**Reference**: `gbrain/src/synthesize.ts`, `gbrain/src/brain.ts` (`minion_dream` subagent)

**Effort**: ~2–3 days (design + implementation + testing)

---

## Known Limitations / Tech Debt

| Issue | Impact | Fix |
|-------|--------|-----|
| Tantivy index rebuilt on schema change | Lose keyword index on upgrade | Add schema version check + rebuild prompt |
| No authentication on HTTP MCP server | Local use only; not safe to expose | Add Bearer token or OAuth 2.1 |
| `rbrain serve supervisor` not fully tested | Background worker untested E2E | Add integration test |
| lindera dictionary size | Adds ~50 MB to binary | Feature-flag per language |
| No pagination on `rbrain list` | Large brains return all pages | Add `--limit` + `--offset` |

---

## Roadmap Summary

```
Done  ████████████████████  Phase 1-10
Next  ░░░░░░░░░░░░░░░░░░░░  Item 6 (get truncate) — 30 min
      ░░░░░░░░░░░░░░░░░░░░  Item 7 (progress bar) — 2 hrs
      ░░░░░░░░░░░░░░░░░░░░  Item 8 (sync embed)   — 3 hrs
      ░░░░░░░░░░░░░░░░░░░░  Item 9 (dream cycle)  — 3 days
```
