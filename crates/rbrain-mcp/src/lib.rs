use rbrain_core::page::Page;
use rbrain_core::schema::TimelineEntry;
use rbrain_engine::Engine;
use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars::{self, JsonSchema},
    tool, tool_router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct RBrainMcpServer {
    engine: Engine,
}

impl RBrainMcpServer {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }
}

// ── Argument types ─────────────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct QueryArgs {
    /// Search query (any language)
    pub query: String,
    /// Max results to return (default 10, max 50)
    pub limit: Option<i32>,
    /// Use LLM query expansion for better recall (default false)
    pub expand: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetArgs {
    /// Page slug (URL-style identifier)
    pub slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PutArgs {
    /// Page slug
    pub slug: String,
    /// Markdown content
    pub content: String,
    /// Page type: "note", "wiki", "book", etc. (default "note")
    pub page_type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DeleteArgs {
    /// Page slug to delete
    pub slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListArgs {
    /// Filter by page type (e.g. "wiki", "note", "book")
    pub page_type: Option<String>,
    /// Filter by tag
    pub tag: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GraphArgs {
    /// Starting page slug
    pub slug: String,
    /// Edge type filter (e.g. "references", "mentions")
    pub edge_type: Option<String>,
    /// Traversal depth (default 2, max 5)
    pub depth: Option<i32>,
    /// Direction: "out" (outgoing), "in" (incoming), "both"
    pub direction: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct BacklinksArgs {
    /// Target page slug — find all pages linking to this page
    pub slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LinkArgs {
    /// Source page slug (the page making the claim or reference)
    pub from: String,
    /// Target page slug (the source being cited or related page)
    pub to: String,
    /// Relationship type: evidence, related, person, period, supports, contrasts, develops, mentions, references
    pub link_type: Option<String>,
    /// Chunk ID from brain_query results — auto-captures that passage as evidence context
    pub chunk_id: Option<i64>,
    /// Free-text context note (used when chunk_id is not provided)
    pub context: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct UnlinkArgs {
    pub from: String,
    pub to: String,
    /// Remove only links of this type (omit to remove all types between these pages)
    pub link_type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ThinkArgs {
    /// Topic to reason about
    pub topic: String,
    /// Number of context chunks (default 12, max 20)
    pub limit: Option<i32>,
    /// Use LLM query expansion for better recall (default false)
    pub expand: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TimelineArgs {
    /// Page slug to append the timeline entry to
    pub slug: String,
    /// Text description of the event or finding
    pub text: String,
    /// Date in YYYY-MM-DD format (default: today)
    pub date: Option<String>,
    /// Source reference (e.g. "book-slug chunk:150")
    pub source: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TagArgs {
    /// Page slug
    pub slug: String,
    /// Tag to add or remove
    pub tag: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct OutlinksArgs {
    /// Source page slug — find all pages this page links to
    pub slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ResearchRunArgs {
    /// Stable research run id, e.g. "brief-2026-06"
    pub run_id: String,
    /// Human-readable title
    pub title: String,
    /// Optional research question
    pub question: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ResearchProtocolArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecordArtifactArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
    /// Artifact page slug, e.g. "artifacts/brief-2026-06"
    pub slug: String,
    pub title: String,
    pub artifact_kind: String,
    pub path: String,
    pub description: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RegisterInputArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
    /// Input page slug, e.g. "datasets/citations-2026-06"
    pub slug: String,
    pub title: String,
    /// Input page type: dataset, literature_corpus, citation_record, source, method_note, research_memo
    pub input_type: String,
    pub content: String,
    /// Optional structured metadata merged into frontmatter
    pub metadata: Option<serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecordFindingArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
    /// Finding page slug, e.g. "findings/topic-shift"
    pub slug: String,
    pub title: String,
    pub status: String,
    pub content: String,
    /// Evidence/artifact/source slugs supporting this finding
    pub supporting_slugs: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecordValidationReportArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
    /// Validation report page slug
    pub slug: String,
    pub title: String,
    pub validator: String,
    pub status: String,
    pub content: String,
    /// Pages validated by this report
    pub validates_slugs: Option<Vec<String>>,
    /// Optional structured suggested actions
    pub suggested_actions: Option<serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct RecordActionItemArgs {
    /// Research run slug, e.g. "research/runs/brief-2026-06"
    pub run_slug: String,
    /// Action item page slug
    pub slug: String,
    pub title: String,
    pub action_kind: String,
    pub status: String,
    pub content: String,
    /// Pages this action item recommends follow-up for
    pub target_slugs: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProvenanceArgs {
    /// Root page slug to inspect
    pub slug: String,
    /// Traversal depth, 1-5, default 2
    pub depth: Option<i32>,
}

// ── Result types ────────────────────────────────────────────────────────────

#[derive(Serialize, JsonSchema)]
pub struct ChunkResult {
    pub page_slug: String,
    pub chunk_id: i64,
    pub score: f64,
    /// Chunk text (up to ~500 chars preview)
    pub text: String,
}

/// Wrapper for Vec<ChunkResult> — MCP spec requires root type 'object'
#[derive(Serialize, JsonSchema)]
pub struct ChunkList {
    pub results: Vec<ChunkResult>,
}

#[derive(Serialize, JsonSchema)]
pub struct PageResult {
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub compiled_truth: String,
    pub timeline: String,
    pub updated_at: String,
}

#[derive(Serialize, JsonSchema)]
pub struct PageSummary {
    pub slug: String,
    pub title: String,
    pub page_type: String,
    pub language: Option<String>,
    pub updated_at: String,
}

/// Wrapper for Vec<PageSummary>
#[derive(Serialize, JsonSchema)]
pub struct PageList {
    pub results: Vec<PageSummary>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GenerateArgs {
    /// Topic to generate a wiki page about
    pub topic: String,
    /// Number of context chunks to use (default 8, max 20)
    pub limit: Option<i32>,
    /// Save the result as a wiki page in the knowledge base (default false)
    pub save: Option<bool>,
    /// Use LLM query expansion for better recall (default false)
    pub expand: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
pub struct GenerateResult {
    /// Generated Markdown wiki content
    pub content: String,
    /// Slug of the saved page, if save=true
    pub saved_as: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct GraphEdge {
    pub target: String,
    pub edge_type: String,
    pub depth: usize,
}

/// Wrapper for Vec<GraphEdge>
#[derive(Serialize, JsonSchema)]
pub struct GraphList {
    pub results: Vec<GraphEdge>,
}

#[derive(Serialize, JsonSchema)]
pub struct LinkRef {
    pub page_slug: String,
    pub edge_type: String,
    pub context: Option<String>,
    pub chunk_id: Option<i64>,
}

/// Wrapper for Vec<LinkRef>
#[derive(Serialize, JsonSchema)]
pub struct LinkList {
    pub results: Vec<LinkRef>,
}

#[derive(Serialize, JsonSchema)]
pub struct ProvenanceNode {
    pub slug: String,
    pub page_type: String,
    pub title: String,
}

#[derive(Serialize, JsonSchema)]
pub struct ProvenanceEdge {
    pub source_slug: String,
    pub target_slug: String,
    pub edge_type: String,
    pub context: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct ProvenanceResult {
    pub root_slug: String,
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

/// Wrapper for Vec<String> (orphan slugs)
#[derive(Serialize, JsonSchema)]
pub struct SlugList {
    pub slugs: Vec<String>,
}

/// Wrapper for brain_get (Option<PageResult> is not a valid root object)
#[derive(Serialize, JsonSchema)]
pub struct GetResult {
    /// true if the page was found
    pub found: bool,
    pub page: Option<PageResult>,
}

/// Wrapper for brain_think (String is not a valid root object)
#[derive(Serialize, JsonSchema)]
pub struct ThinkResult {
    pub reasoning: String,
}

#[derive(Serialize, JsonSchema)]
pub struct StatsResult {
    pub total_pages: i64,
    pub total_chunks: i64,
    pub embedding_coverage_pct: f64,
    pub graph_density: f64,
    pub pages_by_type: std::collections::HashMap<String, i64>,
    pub pages_by_language: std::collections::HashMap<String, i64>,
}

#[derive(Serialize, JsonSchema)]
pub struct MutationResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize, JsonSchema)]
pub struct ResearchProtocolResult {
    pub protocol: serde_json::Value,
}

#[derive(Serialize, JsonSchema)]
pub struct ValidationRunResult {
    pub results: serde_json::Value,
}

impl MutationResult {
    fn ok(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: msg.into(),
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: msg.into(),
        }
    }
}

// ── MCP Tools ───────────────────────────────────────────────────────────────

#[tool_router(server_handler)]
impl RBrainMcpServer {
    /// Hybrid search (vector + keyword + RRF). Returns ranked chunks with text.
    /// Use this to find relevant content before writing or synthesising wiki pages.
    #[tool(
        name = "brain_query",
        description = "Hybrid search combining vector similarity and keyword search (RRF fusion). \
            Returns ranked text chunks with source page. Use before writing wiki pages or answering questions."
    )]
    async fn query(&self, Parameters(args): Parameters<QueryArgs>) -> Json<ChunkList> {
        let limit = args.limit.map(|l| l.clamp(1, 50) as usize).unwrap_or(10);
        let expand = args.expand.unwrap_or(false);
        let lang = rbrain_core::page::Language::detect(&args.query);

        match self
            .engine
            .search_with_context(&args.query, &lang, limit, expand)
            .await
        {
            Ok(chunks) => Json(ChunkList {
                results: chunks
                    .into_iter()
                    .map(|c| ChunkResult {
                        page_slug: c.page_slug,
                        chunk_id: c.chunk_id,
                        score: c.score,
                        text: c.text,
                    })
                    .collect(),
            }),
            Err(e) => {
                tracing::error!("brain_query error: {}", e);
                Json(ChunkList { results: vec![] })
            }
        }
    }

    /// Get a page by slug. Returns full content including compiled_truth and timeline.
    #[tool(
        name = "brain_get",
        description = "Retrieve a page by its slug. Returns the full Markdown content, tags, and metadata."
    )]
    async fn get(&self, Parameters(args): Parameters<GetArgs>) -> Json<GetResult> {
        match self.engine.get_page(&args.slug).await {
            Ok(page) => Json(GetResult {
                found: true,
                page: Some(PageResult {
                    slug: page.slug,
                    title: page.title,
                    page_type: page.page_type,
                    tags: page.tags,
                    language: page.language.as_ref().map(|l| l.to_string()),
                    compiled_truth: page.compiled_truth,
                    timeline: TimelineEntry::render_compat(&page.timeline),
                    updated_at: page.updated_at.to_string(),
                }),
            }),
            Err(_) => Json(GetResult {
                found: false,
                page: None,
            }),
        }
    }

    /// Create or update a page.
    #[tool(
        name = "brain_put",
        description = "Create or update a page in the knowledge base. Use page_type 'wiki' for synthesised summaries, \
            'note' for raw notes, 'book' for imported books."
    )]
    async fn put(&self, Parameters(args): Parameters<PutArgs>) -> Json<MutationResult> {
        let page = Page::new(
            args.slug,
            args.page_type.unwrap_or_else(|| "note".to_string()),
            args.content,
        );
        match self.engine.put_page(page).await {
            Ok(_) => Json(MutationResult::ok("Page saved")),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Delete a page and all its chunks/embeddings.
    #[tool(
        name = "brain_delete",
        description = "Delete a page and all its associated chunks and embeddings."
    )]
    async fn delete(&self, Parameters(args): Parameters<DeleteArgs>) -> Json<MutationResult> {
        match self.engine.delete_page(&args.slug).await {
            Ok(_) => Json(MutationResult::ok("Page deleted")),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// List pages with optional filters.
    #[tool(
        name = "brain_list",
        description = "List all pages with optional type or tag filter. Returns summaries without full content."
    )]
    async fn list(&self, Parameters(args): Parameters<ListArgs>) -> Json<PageList> {
        match self
            .engine
            .list_pages(args.page_type.as_deref(), args.tag.as_deref())
            .await
        {
            Ok(pages) => Json(PageList {
                results: pages
                    .into_iter()
                    .map(|p| PageSummary {
                        slug: p.slug,
                        title: p.title,
                        page_type: p.page_type,
                        language: p.language.as_ref().map(|l| l.to_string()),
                        updated_at: p.updated_at.to_string(),
                    })
                    .collect(),
            }),
            Err(_) => Json(PageList { results: vec![] }),
        }
    }

    /// Traverse the knowledge graph from a page.
    #[tool(
        name = "brain_graph",
        description = "Traverse the knowledge graph starting from a page. \
            direction='out' follows links this page makes, 'in' finds pages linking here, 'both' does both."
    )]
    async fn graph(&self, Parameters(args): Parameters<GraphArgs>) -> Json<GraphList> {
        let depth = args.depth.map(|d| d.clamp(1, 5) as usize).unwrap_or(2);
        let direction = args.direction.as_deref().unwrap_or("out");

        match self
            .engine
            .graph_query(&args.slug, args.edge_type.as_deref(), depth, direction)
            .await
        {
            Ok(edges) => Json(GraphList {
                results: edges
                    .into_iter()
                    .map(|e| GraphEdge {
                        target: e.target,
                        edge_type: e.edge_type,
                        depth: e.depth,
                    })
                    .collect(),
            }),
            Err(_) => Json(GraphList { results: vec![] }),
        }
    }

    /// Find all pages that link to a given page (backlinks).
    #[tool(
        name = "brain_backlinks",
        description = "Find all pages that link to the given page. Useful for discovering related content and context."
    )]
    async fn backlinks(&self, Parameters(args): Parameters<BacklinksArgs>) -> Json<LinkList> {
        match self.engine.backlinks(&args.slug).await {
            Ok(links) => Json(LinkList {
                results: links
                    .into_iter()
                    .map(|l| LinkRef {
                        page_slug: l.target_slug,
                        edge_type: l.edge_type,
                        context: l.context,
                        chunk_id: l.chunk_id,
                    })
                    .collect(),
            }),
            Err(_) => Json(LinkList { results: vec![] }),
        }
    }

    /// Get knowledge base statistics.
    #[tool(
        name = "brain_stats",
        description = "Get statistics about the knowledge base: total pages/chunks, embedding coverage, graph density."
    )]
    async fn stats(&self) -> Json<StatsResult> {
        match self.engine.get_stats().await {
            Ok(s) => Json(StatsResult {
                total_pages: s.total_pages(),
                total_chunks: s.total_chunks,
                embedding_coverage_pct: s.embedding_coverage,
                graph_density: s.graph_density,
                pages_by_type: s.pages_by_type.into_iter().map(|(k, v)| (k, v)).collect(),
                pages_by_language: s
                    .pages_by_language
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect(),
            }),
            Err(_) => Json(StatsResult {
                total_pages: 0,
                total_chunks: 0,
                embedding_coverage_pct: 0.0,
                graph_density: 0.0,
                pages_by_type: Default::default(),
                pages_by_language: Default::default(),
            }),
        }
    }

    /// Search relevant content and synthesise a wiki page using DeepSeek LLM.
    /// Use this to create or update wiki pages from existing knowledge base content.
    #[tool(
        name = "brain_generate",
        description = "Search the knowledge base for relevant content and synthesise a Markdown wiki page \
            using the DeepSeek LLM. Optionally saves the result back to the brain with save=true. \
            Requires deepseek.api_key to be configured."
    )]
    async fn generate(&self, Parameters(args): Parameters<GenerateArgs>) -> Json<GenerateResult> {
        let limit = args.limit.map(|l| l.clamp(1, 20) as usize).unwrap_or(8);
        let expand = args.expand.unwrap_or(false);
        let lang = rbrain_core::page::Language::detect(&args.topic);

        match self
            .engine
            .generate_wiki(&args.topic, &lang, limit, expand)
            .await
        {
            Ok(wiki) => {
                let saved_as = if args.save.unwrap_or(false) {
                    let slug = args
                        .topic
                        .to_lowercase()
                        .replace(' ', "-")
                        .replace(['/', '\\', '.'], "-");
                    let page = Page::new(slug.clone(), "wiki".to_string(), wiki.clone());
                    match self.engine.put_page(page).await {
                        Ok(_) => Some(slug),
                        Err(e) => {
                            tracing::error!("brain_generate: failed to save page: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };
                Json(GenerateResult {
                    content: wiki,
                    saved_as,
                })
            }
            Err(e) => {
                tracing::error!("brain_generate error: {}", e);
                Json(GenerateResult {
                    content: format!("Error: {}", e),
                    saved_as: None,
                })
            }
        }
    }

    /// Add a typed link between two pages, optionally anchored to a specific chunk as evidence.
    #[tool(
        name = "brain_link",
        description = "Create a typed directed link between two pages. \
            Use chunk_id (from brain_query results) to anchor the link to a specific passage as evidence — \
            the chunk text is automatically captured as context. \
            link_type: evidence | related | person | period | supports | contrasts | develops | mentions | references"
    )]
    async fn link(&self, Parameters(args): Parameters<LinkArgs>) -> Json<MutationResult> {
        let link_type = args.link_type.as_deref().unwrap_or("related");

        // Resolve context from chunk_id if provided
        let context: Option<String> = if let Some(chunk_id) = args.chunk_id {
            match self.engine.fetch_chunk_by_id(chunk_id).await {
                Ok(Some((text, _page_slug))) => Some(text),
                Ok(None) => {
                    return Json(MutationResult::err(format!(
                        "Chunk {} not found. Use chunk_id from brain_query results.",
                        chunk_id
                    )));
                }
                Err(e) => {
                    return Json(MutationResult::err(format!("Failed to fetch chunk: {}", e)));
                }
            }
        } else {
            args.context.clone()
        };

        match self
            .engine
            .add_link(
                &args.from,
                &args.to,
                link_type,
                context.as_deref(),
                args.chunk_id,
            )
            .await
        {
            Ok(_) => Json(MutationResult::ok(format!(
                "Link added: {} --[{}]--> {}{}",
                args.from,
                link_type,
                args.to,
                if args.chunk_id.is_some() {
                    format!(" (context from chunk:{})", args.chunk_id.unwrap())
                } else {
                    String::new()
                }
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Remove a link between two pages.
    #[tool(
        name = "brain_unlink",
        description = "Remove a typed link between two pages. \
            Specify link_type to remove only that type; omit to remove all links between these pages."
    )]
    async fn unlink(&self, Parameters(args): Parameters<UnlinkArgs>) -> Json<MutationResult> {
        match self
            .engine
            .remove_link(&args.from, &args.to, args.link_type.as_deref())
            .await
        {
            Ok(n) if n > 0 => Json(MutationResult::ok(format!(
                "Removed {} link(s): {} --> {}",
                n, args.from, args.to
            ))),
            Ok(_) => Json(MutationResult::err(format!(
                "No matching link found: {} --> {}",
                args.from, args.to
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// List pages with no incoming links (orphan pages).
    #[tool(
        name = "brain_orphans",
        description = "Find pages that have no incoming links — not referenced by any other page. \
            Use to discover isolated knowledge that needs to be connected to the graph."
    )]
    async fn orphans(&self) -> Json<SlugList> {
        match self.engine.orphan_pages().await {
            Ok(slugs) => Json(SlugList { slugs }),
            Err(_) => Json(SlugList { slugs: vec![] }),
        }
    }

    /// Deep reasoning synthesis on a topic.
    #[tool(
        name = "brain_think",
        description = "Search the knowledge base for relevant chunks and run structured deep reasoning: \
            core claims, tensions & contradictions, working judgment, open questions. \
            Returns a Markdown reasoning artifact with inline citations. Requires deepseek.api_key."
    )]
    async fn think(&self, Parameters(args): Parameters<ThinkArgs>) -> Json<ThinkResult> {
        let lang = rbrain_core::page::Language::detect(&args.topic);
        let limit = args.limit.map(|l| l.clamp(1, 20) as usize).unwrap_or(12);
        let expand = args.expand.unwrap_or(false);
        match self.engine.think(&args.topic, &lang, limit, expand).await {
            Ok(result) => Json(ThinkResult { reasoning: result }),
            Err(e) => Json(ThinkResult {
                reasoning: format!("Error: {}", e),
            }),
        }
    }

    /// Create a structured research run ledger page.
    #[tool(
        name = "brain_create_research_run",
        description = "Create a structured research_run page for recording an academic workflow. \
            The page is schema-checked and can produce artifacts/findings through provenance edges."
    )]
    async fn create_research_run(
        &self,
        Parameters(args): Parameters<ResearchRunArgs>,
    ) -> Json<MutationResult> {
        match self
            .engine
            .create_research_run(&args.run_id, &args.title, args.question.as_deref())
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!(
                "Research run created: {}",
                page.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Return a grouped protocol view for a research run.
    #[tool(
        name = "brain_get_research_protocol",
        description = "Return a grouped research_run protocol: inputs, artifacts, findings, validation reports, action items, and relation edges."
    )]
    async fn get_research_protocol(
        &self,
        Parameters(args): Parameters<ResearchProtocolArgs>,
    ) -> Json<ResearchProtocolResult> {
        match self.engine.get_research_protocol(&args.run_slug).await {
            Ok(protocol) => Json(ResearchProtocolResult {
                protocol: serde_json::to_value(protocol).unwrap_or_else(|_| serde_json::json!({})),
            }),
            Err(e) => Json(ResearchProtocolResult {
                protocol: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }

    /// Run built-in validators for a research run and persist validation reports/action items.
    #[tool(
        name = "brain_validate_research_run",
        description = "Run built-in validators for a research_run, then write validation_report and action_item pages back to rbrain."
    )]
    async fn validate_research_run(
        &self,
        Parameters(args): Parameters<ResearchProtocolArgs>,
    ) -> Json<ValidationRunResult> {
        match self.engine.validate_research_run(&args.run_slug).await {
            Ok(results) => Json(ValidationRunResult {
                results: serde_json::to_value(results).unwrap_or_else(|_| serde_json::json!([])),
            }),
            Err(e) => Json(ValidationRunResult {
                results: serde_json::json!({ "error": e.to_string() }),
            }),
        }
    }

    /// Record an artifact produced by a research run.
    #[tool(
        name = "brain_record_artifact",
        description = "Create an artifact page and link the research run to it with a produces edge."
    )]
    async fn record_artifact(
        &self,
        Parameters(args): Parameters<RecordArtifactArgs>,
    ) -> Json<MutationResult> {
        match self
            .engine
            .record_artifact(
                &args.run_slug,
                &args.slug,
                &args.title,
                &args.artifact_kind,
                &args.path,
                &args.description,
            )
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!(
                "Artifact recorded: {}",
                page.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Register a dataset, corpus, citation record, source, method note, or memo used by a research run.
    #[tool(
        name = "brain_register_input",
        description = "Create an input page used by a research run and link it with an appropriate provenance edge."
    )]
    async fn register_input(
        &self,
        Parameters(args): Parameters<RegisterInputArgs>,
    ) -> Json<MutationResult> {
        match self
            .engine
            .register_input(
                &args.run_slug,
                &args.slug,
                &args.title,
                &args.input_type,
                &args.content,
                args.metadata,
            )
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!("Input registered: {}", page.slug))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Record a finding produced by a research run.
    #[tool(
        name = "brain_record_finding",
        description = "Create a finding page, link the research run to it with produces, \
            and link the finding to supporting evidence/artifacts with supports edges."
    )]
    async fn record_finding(
        &self,
        Parameters(args): Parameters<RecordFindingArgs>,
    ) -> Json<MutationResult> {
        let supporting_slugs = args.supporting_slugs.unwrap_or_default();
        match self
            .engine
            .record_finding(
                &args.run_slug,
                &args.slug,
                &args.title,
                &args.status,
                &args.content,
                &supporting_slugs,
            )
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!(
                "Finding recorded: {}",
                page.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Record a validation report for a research run artifact, finding, or input.
    #[tool(
        name = "brain_record_validation_report",
        description = "Create a validation_report page, link the research run to it with produces, \
            and link the report to validated pages with validates edges."
    )]
    async fn record_validation_report(
        &self,
        Parameters(args): Parameters<RecordValidationReportArgs>,
    ) -> Json<MutationResult> {
        let validates_slugs = args.validates_slugs.unwrap_or_default();
        match self
            .engine
            .record_validation_report(
                &args.run_slug,
                &args.slug,
                &args.title,
                &args.validator,
                &args.status,
                &args.content,
                &validates_slugs,
                args.suggested_actions,
            )
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!(
                "Validation report recorded: {}",
                page.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Record an action item suggested by a validator, user, or agent.
    #[tool(
        name = "brain_record_action_item",
        description = "Create an action_item page, link the research run to it with produces, \
            and link the action item to its target pages with recommends edges."
    )]
    async fn record_action_item(
        &self,
        Parameters(args): Parameters<RecordActionItemArgs>,
    ) -> Json<MutationResult> {
        let target_slugs = args.target_slugs.unwrap_or_default();
        match self
            .engine
            .record_action_item(
                &args.run_slug,
                &args.slug,
                &args.title,
                &args.action_kind,
                &args.status,
                &args.content,
                &target_slugs,
            )
            .await
        {
            Ok(page) => Json(MutationResult::ok(format!(
                "Action item recorded: {}",
                page.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Return the provenance graph around a finding, artifact, or run.
    #[tool(
        name = "brain_provenance_of",
        description = "Return the local provenance graph around a page using research evidence edges \
            such as produces, supports, derived_from, uses_dataset, uses_corpus, computed_by, validates."
    )]
    async fn provenance_of(
        &self,
        Parameters(args): Parameters<ProvenanceArgs>,
    ) -> Json<ProvenanceResult> {
        let depth = args.depth.map(|d| d.clamp(1, 5) as usize).unwrap_or(2);
        match self.engine.provenance_of(&args.slug, depth).await {
            Ok(graph) => Json(ProvenanceResult {
                root_slug: graph.root_slug,
                nodes: graph
                    .nodes
                    .into_iter()
                    .map(|node| ProvenanceNode {
                        slug: node.slug,
                        page_type: node.page_type,
                        title: node.title,
                    })
                    .collect(),
                edges: graph
                    .edges
                    .into_iter()
                    .map(|edge| ProvenanceEdge {
                        source_slug: edge.source_slug,
                        target_slug: edge.target_slug,
                        edge_type: edge.edge_type,
                        context: edge.context,
                    })
                    .collect(),
            }),
            Err(_) => Json(ProvenanceResult {
                root_slug: args.slug,
                nodes: vec![],
                edges: vec![],
            }),
        }
    }

    /// Append a dated entry to a page's timeline.
    #[tool(
        name = "brain_add_timeline_entry",
        description = "Append a dated event or finding to a page's timeline log. \
            Use this to record when a scholar is mentioned, a claim is encountered, or a source is read. \
            date defaults to today (YYYY-MM-DD) if omitted."
    )]
    async fn add_timeline_entry(
        &self,
        Parameters(args): Parameters<TimelineArgs>,
    ) -> Json<MutationResult> {
        let date = args
            .date
            .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
        match self
            .engine
            .add_timeline_entry(&args.slug, &date, &args.text, args.source.as_deref())
            .await
        {
            Ok(_) => Json(MutationResult::ok(format!(
                "Timeline entry added to '{}'",
                args.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Add a tag to a page.
    #[tool(
        name = "brain_add_tag",
        description = "Add a tag to a page. Tags enable filtering with brain_list and brain_query."
    )]
    async fn add_tag(&self, Parameters(args): Parameters<TagArgs>) -> Json<MutationResult> {
        match self.engine.add_tag(&args.slug, &args.tag).await {
            Ok(_) => Json(MutationResult::ok(format!(
                "Tag '{}' added to '{}'",
                args.tag, args.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Remove a tag from a page.
    #[tool(name = "brain_remove_tag", description = "Remove a tag from a page.")]
    async fn remove_tag(&self, Parameters(args): Parameters<TagArgs>) -> Json<MutationResult> {
        match self.engine.remove_tag(&args.slug, &args.tag).await {
            Ok(_) => Json(MutationResult::ok(format!(
                "Tag '{}' removed from '{}'",
                args.tag, args.slug
            ))),
            Err(e) => Json(MutationResult::err(format!("Failed: {}", e))),
        }
    }

    /// Find all pages this page links to (outgoing links).
    #[tool(
        name = "brain_outlinks",
        description = "Find all pages that this page links to (outgoing links). \
            Complement to brain_backlinks. Shows what a page references or cites."
    )]
    async fn outlinks(&self, Parameters(args): Parameters<OutlinksArgs>) -> Json<LinkList> {
        match self.engine.outlinks(&args.slug).await {
            Ok(links) => Json(LinkList {
                results: links
                    .into_iter()
                    .map(|l| LinkRef {
                        page_slug: l.target_slug,
                        edge_type: l.edge_type,
                        context: l.context,
                        chunk_id: l.chunk_id,
                    })
                    .collect(),
            }),
            Err(_) => Json(LinkList { results: vec![] }),
        }
    }
}

pub async fn run_stdio_server(engine: Engine) -> anyhow::Result<()> {
    let server = RBrainMcpServer::new(engine);
    let service = rmcp::serve_server(server, (tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}

pub mod http;
pub use http::{run_http_server, run_http_server_with_options};
