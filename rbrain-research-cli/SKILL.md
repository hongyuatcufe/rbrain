# RBrain Research CLI

## Core Idea

Use the **rbrain** Rust CLI as the operational layer for research knowledge bases. rbrain auto-discovers the active brain by walking up from CWD — no explicit `BRAIN_HOME` needed. Use CLI for repeatable, auditable actions; use MCP tools for lightweight conversational retrieval.

This skill is project-neutral. First discover the active project root and brain state, then apply the workflow.

Source:
```text
https://github.com/hongyuatcufe/rbrain
```

When behavior is uncertain, inspect the local source at `~/project/rbrain/rbrain` before assuming default CLI behavior.

---

## Bootstrap

Verify the CLI is available:

```bash
command -v rbrain
rbrain --version
```

If missing, rebuild from source:

```bash
cd ~/project/rbrain/rbrain
cargo build --release -p rbrain-cli
ln -sf "$PWD/target/release/rbrain" ~/.local/bin/rbrain
```

---

## Brain Discovery (run first)

```bash
rbrain stats          # active brain path, page/chunk/link/embedding counts
rbrain doctor         # health check: broken links, unembedded pages, orphans
pwd
```

rbrain auto-discovers the active brain:
- finds `.rbrain/` walking up from CWD → project-local brain
- falls back to `~/.rbrain/` if none found

Initialize a new project brain:

```bash
rbrain init           # creates .rbrain/ and updates .gitignore
```

Config lives at `.rbrain/config.toml`:

```toml
[qwen]
api_key = "sk-..."
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model = "text-embedding-v4"

[deepseek]
api_key = "sk-..."
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"

embedding_dim = 1024
```

---

## Standard Path Layout

```
notes/<slug>          Imported source articles and reading notes
concepts/<slug>       Core concepts (auto-created by dream extract)
figures/<slug>        Scholars and historical figures (auto-created by dream extract)
synthesis/<slug>      Concept-level synthesis (auto-created by dream synthesize)
wiki/<slug>           Reference wiki pages (rbrain generate --save)
questions/<slug>      Guiding research questions
evidence/<slug>       Source-grounded evidence cards
drafts/<slug>         Chapter and section drafts
```

---

## Retrieval

```bash
rbrain search "exact term or phrase"              # keyword BM25 (no API needed)
rbrain query "thematic question" --expand         # hybrid search + LLM query expansion
rbrain get <slug>                                 # full page content + timeline
rbrain backlinks <slug>                           # pages that link here
rbrain links <slug>                               # outgoing links from this page
rbrain graph-query <slug> --depth 2 --direction both   # graph neighborhood
rbrain orphans                                    # pages with no incoming links
rbrain list --type concept                        # list by type
rbrain list --tag <tag>                           # list by tag
```

For CJK corpora: start with `search` on exact names/terms before `query` — sparse classical text may miss semantic search.

**Via MCP:**
```
brain_search(query="...", limit=10)
brain_query(query="...", limit=12, expand=true)
brain_get(slug="...")
brain_backlinks(slug="...")
brain_outlinks(slug="...")
```

---

## Writing Pages

```bash
# From file
rbrain put my/page/slug --file path/to/file.md

# From stdin
printf '---\ntype: concept\ntitle: Title\ntags: []\n---\n\n# Title\n\n## Problem Or Definition\n\n## Working Judgment\n\n## Open Questions\n' \
  | rbrain put concepts/my-concept

# Re-embed after save happens automatically when API keys are configured
# Offline drafting:
rbrain put <slug> --file draft.md --mock-embed
```

Page types: `note` | `wiki` | `concept` | `question` | `figure` | `evidence` | `synthesis` | `draft` | `memo` | `period` | `book`

**Via MCP:**
```
brain_put(slug="concepts/<slug>", content="...", page_type="concept")
```

---

## Graph Links — Evidence Workflow

Link to **specific passages** using chunk IDs from search output:

```bash
# Step 1: find relevant passages (chunk IDs shown in output)
rbrain search "自主知识体系" --limit 5
# → [1] raw/articles/论中国教育学... — [chunk:42] "..."

# Step 2: link concept to passage as evidence
rbrain link concepts/自主知识体系 raw/articles/论中国教育学 \
  --type evidence --from-chunk 42

# Step 3: verify
rbrain backlinks raw/articles/论中国教育学
```

Link types: `evidence` | `related` | `supports` | `contrasts` | `develops`

```bash
rbrain unlink <from> <to> [--type <t>]
rbrain extract --all    # auto-extract [[wikilinks]] from page content
```

**Via MCP:**
```
brain_link(from="...", to="...", link_type="evidence", chunk_id=42)
brain_unlink(from="...", to="...")
```

---

## Timeline — Dated Evidence Log

Append dated entries to any page (visible in `rbrain get` below the `---` divider):

```bash
rbrain timeline figures/孔子 \
  --date "2026-05-15" \
  --text "提出有教无类，强调教育机会平等" \
  --source "中国教育思想通史 chunk:150"
```

No `--date` → defaults to today.

**Via MCP:**
```
brain_add_timeline_entry(
  slug="figures/<slug>",
  text="...",
  date="2026-05-15",          # optional, defaults to today
  source="<page-slug> chunk:<id>"
)
```

---

## Takes — Interpretive Fragments

Attach short interpretive notes to any page without rewriting it:

```bash
rbrain take concepts/自主知识体系 \
  "自主不等于排外，而是建立平等对话的主体地位" \
  --kind judgment

rbrain take concepts/自主知识体系 \
  "这一框架在基础教育领域是否同样适用？" \
  --kind question

rbrain takes concepts/自主知识体系   # list all takes
```

`--kind`: `judgment` | `question` | `hypothesis` | `interpretation` (default)

---

## Tags

```bash
rbrain tag <slug> <tag>       # add tag
rbrain untag <slug> <tag>     # remove tag
rbrain tags <slug>            # list tags on a page
rbrain list --tag <tag>       # list pages with tag
```

**Via MCP:**
```
brain_add_tag(slug="...", tag="...")
brain_remove_tag(slug="...", tag="...")
```

---

## Think — Deep Reasoning Artifact

Reasons through contradictions, tensions, and working judgments. The process is the artifact:

```bash
rbrain think "中国教育学自主知识体系的逻辑起点之争" --limit 12 --expand
rbrain think "标准化与地方自主权的张力" --save   # saves as synthesis/<slug>
```

Output sections: `## 核心观点 / ## 张力与矛盾 / ## 工作判断 / ## 开放问题`

**Via MCP:**
```
brain_think(topic="...", limit=12, expand=true)
```

---

## Generate — Wiki Output

Search + LLM → structured wiki draft:

```bash
rbrain generate "topic" --limit 12 --expand          # print draft
rbrain generate "topic" --limit 12 --expand --save   # saves as wiki/<slug>
```

---

## Dream Cycle — Automated Knowledge Extraction

`rbrain dream` runs a multi-stage pipeline over all source notes:

```bash
rbrain dream                       # full pipeline (all stages)
rbrain dream --stage lint          # Phase 1: lint issues
rbrain dream --stage embed         # Phase 2: embed stale pages
rbrain dream --stage extract       # Phase 3: extract concepts/figures
rbrain dream --stage synthesize    # Phase 4: synthesize concept clusters
```

### Phase 3 — Extract

Reads every `note` page not yet processed, calls DeepSeek to extract:
- **Concepts** → written to `concepts/<slug>` (type: `concept`)
- **Figures** (scholars/people) → written to `figures/<slug>` (type: `figure`)
- **Timeline events** → appended to the relevant figure page
- Links source note → concept/figure with type `related`
- Tracks processed pages in `dream_metadata` table (won't re-process unless cleared)

Figure descriptions use real-world identity (institution, role, field) — never vague self-references like "本文作者".

### Phase 4 — Synthesize

For each concept page with **3+ source note backlinks**, generates a structured synthesis:
- Saved at `synthesis/<concept-slug>` (type: `synthesis`)
- Auto-links: `synthesis → concept` (`develops`), `synthesis → source notes` (`evidence`)
- Staleness check: re-synthesizes if any source is newer than the existing synthesis
- Output: structured Markdown with H1, ## thematic sections, ## Working Judgment, ## Open Questions, [[wikilink]] citations

Re-run extract after clearing metadata:
```bash
sqlite3 .rbrain/brain.db "DELETE FROM dream_metadata;"
rbrain dream --stage extract
```

---

## Bulk Operations

```bash
rbrain import <dir>      # import all .md files from directory
rbrain embed --all       # embed all pages missing vectors
rbrain embed --stale     # re-embed only stale pages
rbrain extract --all     # extract [[wikilinks]] from all pages
rbrain sync              # sync filesystem changes → database
rbrain sync --embed      # sync + re-embed changed pages
```

---

## Quality & Maintenance

```bash
rbrain lint                                          # issues report
rbrain export --dir /tmp/out --format json           # export as JSON
rbrain export --dir /tmp/out --format md             # export as Markdown
```

---

## CLI vs MCP

| Prefer CLI for | Prefer MCP for |
|---|---|
| Batch import/export | Quick conversational lookups |
| Stats and health checks | Exploratory retrieval in chat |
| Embedding refreshes | Reading pages without file changes |
| Writing pages from local Markdown | Signal detection during research sessions |
| Dream cycle pipeline | Adding timeline entries on the fly |
| Git progress snapshots | |

---

## Progress Report Format

After each session:
```
Pages: P total / C chunks / L links.
By type: N notes, C concepts, F figures, S synthesis, W wiki.
Signals: X concepts captured, Y timeline entries added, Z links created.
```

```bash
rbrain stats
```

---

## Git Snapshot

```bash
git status --short
git add notes/ concepts/ figures/ synthesis/ wiki/ questions/ evidence/ drafts/
git commit -m "Research progress: X notes, Y concepts, Z questions"
```

---

## Safety Rules

- Never print or commit API keys. Config is at `.rbrain/config.toml` — gitignored by default.
- Do not commit `.rbrain/` database directories.
- Do not run concurrent rbrain write commands against the same brain (SQLite contention).
- Treat `rbrain generate` and `rbrain dream` output as drafts — verify claims with `rbrain get`.
- If embedding fails, keep working: put pages without embedding, run `rbrain embed --stale` later.

---

## Signal Detection (Always-On Within Session)

While completing the main research task, **also capture new insights to the brain** without blocking the response.

### When to create or update a concept page

Trigger: user expresses a new argument, framework, or interpretation worth preserving.

```bash
printf '---\ntype: concept\ntitle: <Title>\ntags: []\n---\n\n# <Title>\n\n## Problem Or Definition\n<exact phrasing>\n\n## Working Judgment\n<judgment>\n' \
  | rbrain put concepts/<slug>
```

Use the user's **exact phrasing** — do not paraphrase original thinking.

### When to add a timeline entry

Trigger: a scholar or historical figure is mentioned with a specific claim or date.

```bash
rbrain timeline figures/<slug> --text "..." --source "<slug> chunk:<id>"
```

Create the figure page first if it doesn't exist:
```bash
printf '---\ntype: figure\ntitle: <Name>\ntags: []\n---\n\n<Name>\n\n<Institution, role, field.>\n' \
  | rbrain put figures/<slug>
```

### When to link

After creating or updating a concept page, cross-link to the grounding source:
```bash
rbrain link concepts/<slug> <source-slug> --type evidence --from-chunk <id>
```

### Signal Log (end of every session)

```
Signals: N concepts captured/updated, M timeline entries added, K links created.
```

### Anti-patterns

- Do NOT create pages for one-sentence passing mentions
- Do NOT paraphrase — the user's exact language is the insight
- Do NOT block the main response to run signal detection
- Do NOT create duplicate pages — check with `rbrain get` first
