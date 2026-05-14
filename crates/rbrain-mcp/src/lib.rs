use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    schemars::{self, JsonSchema},
    tool, tool_router,
};
use rbrain_core::page::Page;
use rbrain_engine::Engine;
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

fn detect_lang(text: &str) -> rbrain_core::page::Language {
    match whatlang::detect(text).map(|i| i.lang()) {
        Some(whatlang::Lang::Jpn) => rbrain_core::page::Language::Ja,
        Some(whatlang::Lang::Kor) => rbrain_core::page::Language::Ko,
        Some(whatlang::Lang::Cmn) => rbrain_core::page::Language::ZhHans,
        _ => rbrain_core::page::Language::En,
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

// ── Result types ────────────────────────────────────────────────────────────

#[derive(Serialize, JsonSchema)]
pub struct ChunkResult {
    pub page_slug: String,
    pub chunk_id: i64,
    pub score: f64,
    /// Chunk text (up to ~500 chars preview)
    pub text: String,
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

#[derive(Serialize, JsonSchema)]
pub struct LinkRef {
    pub source_slug: String,
    pub edge_type: String,
    pub context: Option<String>,
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

impl MutationResult {
    fn ok(msg: impl Into<String>) -> Self { Self { ok: true, message: msg.into() } }
    fn err(msg: impl Into<String>) -> Self { Self { ok: false, message: msg.into() } }
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
    async fn query(&self, Parameters(args): Parameters<QueryArgs>) -> Json<Vec<ChunkResult>> {
        let limit = args.limit.map(|l| l.clamp(1, 50) as usize).unwrap_or(10);
        let expand = args.expand.unwrap_or(false);
        let lang = detect_lang(&args.query);

        match self.engine.search_with_context(&args.query, &lang, limit, expand).await {
            Ok(chunks) => Json(
                chunks
                    .into_iter()
                    .map(|c| ChunkResult {
                        page_slug: c.page_slug,
                        chunk_id: c.chunk_id,
                        score: c.score,
                        text: c.text,
                    })
                    .collect(),
            ),
            Err(e) => {
                tracing::error!("brain_query error: {}", e);
                Json(vec![])
            }
        }
    }

    /// Get a page by slug. Returns full content including compiled_truth and timeline.
    #[tool(
        name = "brain_get",
        description = "Retrieve a page by its slug. Returns the full Markdown content, tags, and metadata."
    )]
    async fn get(&self, Parameters(args): Parameters<GetArgs>) -> Json<Option<PageResult>> {
        match self.engine.get_page(&args.slug).await {
            Ok(page) => Json(Some(PageResult {
                slug: page.slug,
                title: page.title,
                page_type: page.page_type,
                tags: page.tags,
                language: page.language.as_ref().map(|l| l.to_string()),
                compiled_truth: page.compiled_truth,
                timeline: page.timeline,
                updated_at: page.updated_at.to_string(),
            })),
            Err(_) => Json(None),
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
    async fn list(&self, Parameters(args): Parameters<ListArgs>) -> Json<Vec<PageSummary>> {
        match self
            .engine
            .list_pages(args.page_type.as_deref(), args.tag.as_deref())
            .await
        {
            Ok(pages) => Json(
                pages
                    .into_iter()
                    .map(|p| PageSummary {
                        slug: p.slug,
                        title: p.title,
                        page_type: p.page_type,
                        language: p.language.as_ref().map(|l| l.to_string()),
                        updated_at: p.updated_at.to_string(),
                    })
                    .collect(),
            ),
            Err(_) => Json(vec![]),
        }
    }

    /// Traverse the knowledge graph from a page.
    #[tool(
        name = "brain_graph",
        description = "Traverse the knowledge graph starting from a page. \
            direction='out' follows links this page makes, 'in' finds pages linking here, 'both' does both."
    )]
    async fn graph(&self, Parameters(args): Parameters<GraphArgs>) -> Json<Vec<GraphEdge>> {
        let depth = args.depth.map(|d| d.clamp(1, 5) as usize).unwrap_or(2);
        let direction = args.direction.as_deref().unwrap_or("out");

        match self
            .engine
            .graph_query(&args.slug, args.edge_type.as_deref(), depth, direction)
            .await
        {
            Ok(edges) => Json(
                edges
                    .into_iter()
                    .map(|e| GraphEdge {
                        target: e.target,
                        edge_type: e.edge_type,
                        depth: e.depth,
                    })
                    .collect(),
            ),
            Err(_) => Json(vec![]),
        }
    }

    /// Find all pages that link to a given page (backlinks).
    #[tool(
        name = "brain_backlinks",
        description = "Find all pages that link to the given page. Useful for discovering related content and context."
    )]
    async fn backlinks(&self, Parameters(args): Parameters<BacklinksArgs>) -> Json<Vec<LinkRef>> {
        match self.engine.backlinks(&args.slug).await {
            Ok(links) => Json(
                links
                    .into_iter()
                    .map(|l| LinkRef {
                        source_slug: l.target_slug,
                        edge_type: l.edge_type,
                        context: l.context,
                    })
                    .collect(),
            ),
            Err(_) => Json(vec![]),
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
                pages_by_type: s
                    .pages_by_type
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect(),
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
        let lang = detect_lang(&args.topic);

        match self.engine.generate_wiki(&args.topic, &lang, limit, expand).await {
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
                Json(GenerateResult { content: wiki, saved_as })
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
}

pub async fn run_stdio_server(engine: Engine) -> anyhow::Result<()> {
    let server = RBrainMcpServer::new(engine);
    let service = rmcp::serve_server(server, (tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}

pub mod http;
pub use http::run_http_server;
