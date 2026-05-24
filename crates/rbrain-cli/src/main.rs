use clap::{Parser, Subcommand};
use rbrain_core::config::Config;
use rbrain_core::embedder::Embedder;
use rbrain_core::markdown::MarkdownParser;
use rbrain_core::page::Page;
use rbrain_engine::{Engine, extract_links};
use rbrain_llm::mock::MockEmbedder;
use rbrain_llm::qwen::QwenEmbedder;
use rbrain_search::TantivyIndex;
use rbrain_search::vector_store::UsearchStore;
use std::io::Read;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rbrain")]
#[command(about = "Personal AI knowledge base")]
#[command(version)]
struct Cli {
    /// Use deterministic mock embedder (no API key needed, for testing)
    #[arg(long, global = true)]
    mock_embed: bool,

    /// Path to the brain directory (e.g. /path/to/project/.rbrain). Equivalent to RBRAIN_HOME env var.
    #[arg(long, global = true, value_name = "PATH")]
    brain_dir: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise a project-local brain in the current directory (creates .rbrain/)
    Init,
    /// Create or update a page (reads from --content, --file, or stdin; auto re-embeds on save)
    Put {
        slug: String,
        #[arg(long, help = "Page type: note, wiki, book, etc. (default: note)")]
        r#type: Option<String>,
        #[arg(long, help = "Read content from file instead of stdin")]
        file: Option<String>,
        #[arg(long, help = "Page content as a string (alternative to stdin/--file)", allow_hyphen_values = true)]
        content: Option<String>,
    },
    /// Retrieve a page by slug and print its content
    Get {
        slug: String,
    },
    /// Delete a page and all its chunks/embeddings
    Delete {
        slug: String,
    },
    /// List pages with optional type or tag filter
    List {
        #[arg(long, help = "Filter by page type (e.g. wiki, note, book)")]
        r#type: Option<String>,
        #[arg(long, help = "Filter by tag")]
        tag: Option<String>,
        #[arg(long, help = "Output as JSON")]
        json: bool,
        #[arg(short, long, default_value = "50", help = "Max pages to show")]
        limit: usize,
    },
    /// Import all markdown files from a directory
    Import {
        dir: String,
        #[arg(long, help = "Skip embedding after import (faster, do embed --all later)")]
        no_embed: bool,
    },
    /// Sync the repo directory with the database (detect new/changed/deleted files)
    Sync {
        #[arg(long, help = "Re-embed changed pages after sync")]
        embed: bool,
    },
    /// Embed pages into the vector index
    Embed {
        #[arg(help = "Page slug to embed (omit with --all or --stale)")]
        slug: Option<String>,
        #[arg(long, help = "Embed all pages (including already-embedded)")]
        all: bool,
        #[arg(long, help = "Only embed pages with missing or stale embeddings")]
        stale: bool,
    },
    /// Extract wikilinks and timeline entries from pages into the graph
    Extract {
        #[arg(help = "Page slug to extract (omit with --all)")]
        slug: Option<String>,
        #[arg(long, help = "Extract from all pages")]
        all: bool,
    },
    /// Traverse the knowledge graph from a page
    GraphQuery {
        slug: String,
        #[arg(long, help = "Filter by edge type (e.g. references, mentions)")]
        edge_type: Option<String>,
        #[arg(long, help = "Alias for --edge-type")]
        r#type: Option<String>,
        #[arg(long, default_value = "2", help = "Traversal depth (max 5)")]
        depth: usize,
        #[arg(long, default_value = "out", help = "Direction: out, in, or both")]
        direction: String,
    },
    /// Alias for graph-query: traverse the knowledge graph from a page
    Graph {
        slug: String,
        #[arg(long, help = "Filter by edge type (e.g. evidence, related)")]
        edge_type: Option<String>,
        #[arg(long, help = "Alias for --edge-type")]
        r#type: Option<String>,
        #[arg(long, default_value = "2", help = "Traversal depth (max 5)")]
        depth: usize,
        #[arg(long, default_value = "out", help = "Direction: out, in, or both")]
        direction: String,
    },
    /// Show all pages that link to a given page (incoming links)
    Backlinks {
        slug: String,
    },
    /// Show all pages this page links to (outgoing links with evidence context)
    Links {
        slug: String,
    },
    /// Add an explicit typed link from one page to another
    Link {
        /// Source page slug
        from: String,
        /// Target page slug
        to: String,
        /// Link type: evidence, related, person, period, supports, contrasts, develops, mentions, references (default: related)
        #[arg(long, default_value = "related")]
        r#type: String,
        /// Capture a specific chunk as evidence context (use chunk_id shown in search/query output)
        #[arg(long, value_name = "CHUNK_ID")]
        from_chunk: Option<i64>,
        /// Optional free-text context note (overridden by --from-chunk if both given)
        #[arg(long)]
        context: Option<String>,
    },
    /// Remove a link between two pages
    Unlink {
        from: String,
        to: String,
        /// Only remove links of this type (omit to remove all types between these pages)
        #[arg(long)]
        r#type: Option<String>,
    },
    /// List pages with no incoming links (orphan pages not referenced by any other page)
    Orphans,
    /// Hybrid search (vector + keyword + RRF), results grouped by page
    Query {
        query: String,
        #[arg(long, help = "Use LLM query expansion for better recall (extra API call)")]
        expand: bool,
        #[arg(short, long, default_value = "10", help = "Max pages to return")]
        limit: usize,
        #[arg(long, help = "Filter by page type (e.g. book, note, concept)")]
        r#type: Option<String>,
        #[arg(long, help = "Filter by tag")]
        tag: Option<String>,
    },
    /// Keyword-only search (no embedder needed), results grouped by page
    Search {
        query: String,
        #[arg(short, long, default_value = "10", help = "Max pages to return")]
        limit: usize,
        #[arg(long, help = "Filter by page type (e.g. book, note, concept)")]
        r#type: Option<String>,
        #[arg(long, help = "Filter by tag")]
        tag: Option<String>,
    },
    /// Search relevant chunks and synthesise a wiki page via LLM (DeepSeek)
    Generate {
        topic: String,
        #[arg(short, long, default_value = "8", help = "Number of chunks to use as context")]
        limit: usize,
        #[arg(long, help = "Save the generated page to the knowledge base")]
        save: bool,
        #[arg(long, help = "Use LLM query expansion for better recall")]
        expand: bool,
    },
    /// Add a dated entry to a page's timeline (evidence log)
    Timeline {
        slug: String,
        /// Date in YYYY-MM-DD format (default: today)
        #[arg(long)]
        date: Option<String>,
        /// Timeline entry text
        #[arg(long)]
        text: String,
        /// Optional source citation (person, document, URL)
        #[arg(long)]
        source: Option<String>,
    },
    /// Add a short interpretive take to a page (judgment, question, interpretation)
    Take {
        slug: String,
        /// The take content (your interpretation or working judgment)
        content: String,
        /// Take kind: interpretation, question, judgment, hypothesis (default: interpretation)
        #[arg(long, default_value = "interpretation")]
        kind: String,
    },
    /// List all takes (interpretive entries) for a page
    Takes {
        slug: String,
    },
    /// Deep reasoning synthesis: search context + LLM reasoning artifact (contradictions, open questions, working judgment)
    Think {
        topic: String,
        #[arg(short, long, default_value = "12", help = "Number of context chunks")]
        limit: usize,
        #[arg(long, help = "Save the reasoning artifact as a synthesis page")]
        save: bool,
        #[arg(long, help = "Use LLM query expansion for better recall")]
        expand: bool,
    },
    /// Add a tag to a page
    Tag {
        slug: String,
        tag: String,
    },
    /// Remove a tag from a page
    Untag {
        slug: String,
        tag: String,
    },
    /// List all tags on a page
    Tags {
        slug: String,
    },
    /// Export pages to a directory (--format md or json)
    Export {
        #[arg(long, default_value = "/tmp/rbrain-export", help = "Output directory")]
        dir: String,
        #[arg(long, default_value = "md", help = "Output format: md or json")]
        format: String,
    },
    /// Quality check: report missing titles, unembedded pages, broken links, orphans
    Lint,
    /// Run health checks and optionally fix common issues
    Doctor {
        #[arg(long, help = "Attempt to fix detected issues automatically")]
        fix: bool,
    },
    /// Alias for doctor: run health checks
    Health {
        #[arg(long, help = "Attempt to fix detected issues automatically")]
        fix: bool,
    },
    /// Show brain statistics (pages, chunks, embedding coverage)
    Stats,
    /// Show current configuration (API keys redacted)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Job queue management (submit, list, get, cancel, work)
    Jobs {
        #[command(subcommand)]
        action: JobsAction,
    },
    /// Start the MCP server (stdio or HTTP) or the background job supervisor
    Serve {
        #[command(subcommand)]
        action: ServeAction,
    },
    /// Run autonomous dream cycle (lint -> embed -> extract -> synthesize)
    Dream {
        #[arg(long, help = "Only run a specific stage of the dream cycle (lint, embed, extract, synthesize)")]
        stage: Option<String>,
    },
}

#[derive(Subcommand)]
enum JobsAction {
    Submit {
        name: String,
        params: String,
        #[arg(long)]
        queue: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
    },
    List {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },
    Get { id: i64 },
    Cancel { id: i64 },
    Work {
        #[arg(long, default_value = "1")]
        concurrency: usize,
    },
    Stats,
}

#[derive(Subcommand)]
enum ServeAction {
    Mcp {
        #[arg(long)]
        http: Option<String>,
    },
    Supervisor {
        #[arg(long, default_value = "1")]
        concurrency: usize,
        #[arg(long, default_value = "60")]
        interval_secs: u64,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show all configuration values (API keys redacted)
    Show,
    /// Get a specific configuration value by key (e.g. models.think, deepseek.model)
    Get {
        key: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let mock_embed = cli.mock_embed;
    let brain_dir = cli.brain_dir;

    // Helper macro so every command arm can call load_config!() without repetition.
    macro_rules! load_config {
        () => {
            if let Some(ref bd) = brain_dir {
                Config::load_with_brain_dir(bd)?
            } else {
                Config::load()?
            }
        };
    }

    match cli.command {
        Commands::Init => {
            let cwd = std::env::current_dir()?;
            let local_dir = cwd.join(".rbrain");
            std::fs::create_dir_all(&local_dir)?;
            std::fs::create_dir_all(local_dir.join("tantivy"))?;
            std::fs::create_dir_all(local_dir.join("dictionaries"))?;

            // Add .rbrain/ to .gitignore so DB files are never committed.
            update_gitignore(&cwd)?;

            let config = load_config!();
            Engine::open(config.clone()).await?;

            if config.is_local() {
                println!("Brain initialised (project-local): {}", local_dir.display());
            } else {
                println!("Brain initialised (global): {}", config.data_dir.display());
            }
        }
        Commands::Put { slug, r#type, file, content: content_flag } => {
            let config = load_config!();
            // Use search engine so we can re-embed after saving.
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;

            let content = if let Some(c) = content_flag {
                c
            } else if let Some(f) = file {
                std::fs::read_to_string(f)?
            } else {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };

            // Parse frontmatter so that type/title/tags/timeline are extracted correctly.
            let parse_result = MarkdownParser::parse(&content);
            let page_type = r#type
                .or_else(|| parse_result.frontmatter.get("type").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| "note".to_string());

            let mut page = Page::new(slug, page_type, parse_result.compiled_truth);
            page.timeline = parse_result.timeline;
            page.frontmatter = parse_result.frontmatter.clone();

            // Extract tags from frontmatter if present
            if let Some(tags_val) = parse_result.frontmatter.get("tags") {
                if let Some(arr) = tags_val.as_array() {
                    page.tags = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                }
            }

            // Extract title from frontmatter if present
            if let Some(title_val) = parse_result.frontmatter.get("title") {
                if let Some(t) = title_val.as_str() {
                    page.title = t.to_string();
                }
            }

            // Detect language from content
            page.language = Some(rbrain_core::page::Language::detect(&page.compiled_truth));

            engine.put_page(page.clone()).await?;
            println!("Page saved: {}", page.slug);

            // Re-embed immediately if an embedder is available.
            if engine.has_embedder() {
                eprint!("Embedding… ");
                match engine.chunk_and_embed_page(&page).await {
                    Ok(_) => eprintln!("done."),
                    Err(e) => eprintln!("warning: embed failed ({}). Run `rbrain embed {}` manually.", e, page.slug),
                }
            }
        }
        Commands::Get { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;

            match engine.find_page_fuzzy(&slug).await {
                Ok((page, score)) => {
                    if score < 1.0 {
                        eprintln!("No exact match found for '{}'. Using best match: '{}' (similarity: {:.2})",
                            slug, page.slug, score);
                    }
                    println!("Slug: {}", page.slug);
                    println!("Title: {}", page.title);
                    println!("Type: {}", page.page_type);
                    println!("Tags: {}", page.tags.join(", "));
                    println!("Language: {:?}", page.language);
                    println!("Updated: {}", page.updated_at);
                    println!("\n{}", page.compiled_truth);
                    if !page.timeline.trim().is_empty() {
                        println!("\n---\n\n{}", page.timeline);
                    }
                }
                Err(_) => {
                    eprintln!("Page not found: {}", slug);
                    return Ok(());
                }
            }
        }
        Commands::Delete { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            engine.delete_page(&slug).await?;
            println!("Page deleted");
        }
        Commands::List { r#type, tag, json, limit } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let pages = engine.list_pages(r#type.as_deref(), tag.as_deref()).await?;
            let total = pages.len();
            let pages: Vec<_> = pages.into_iter().take(limit).collect();

            if json {
                println!("{}", serde_json::to_string_pretty(&pages)?);
            } else {
                for page in &pages {
                    println!("{} ({}) - {}", page.slug, page.page_type, page.title);
                }
                if total > limit {
                    println!("  … and {} more (use -l {} to see all)", total - limit, total);
                }
            }
        }
        Commands::Import { dir, no_embed } => {
            let config = load_config!();
            let engine = if no_embed {
                Engine::open(config.clone()).await?
            } else {
                init_engine_with_search(config.clone(), mock_embed).await?
            };
            let imported = engine.import_dir(&dir).await?;
            println!("Imported {} pages", imported.len());
            for slug in &imported {
                println!("  {}", slug);
            }
        }
        Commands::Sync { embed } => {
            let config = load_config!();
            let engine = if embed {
                init_engine_with_search(config.clone(), mock_embed).await?
            } else {
                Engine::open(config.clone()).await?
            };
            let (imported, updated, orphaned) = engine.sync().await?;
            println!("Sync complete:");
            println!("  New: {}", imported.len());
            println!("  Updated: {}", updated.len());
            println!("  Orphaned: {}", orphaned.len());
            if !orphaned.is_empty() {
                println!("  Orphaned pages: {}", orphaned.join(", "));
            }
            if embed && engine.has_embedder() {
                let changed: Vec<String> = imported.iter().chain(updated.iter()).cloned().collect();
                if !changed.is_empty() {
                    println!("Re-embedding {} changed page(s)...", changed.len());
                    for (idx, slug) in changed.iter().enumerate() {
                        if let Ok(page) = engine.get_page(slug).await {
                            match engine.chunk_and_embed_page(&page).await {
                                Ok(_) => println!("  [{}/{}] {}", idx + 1, changed.len(), slug),
                                Err(e) => eprintln!("  [{}/{}] {} — embed error: {}", idx + 1, changed.len(), slug, e),
                            }
                        }
                    }
                }
            }
        }
        Commands::Embed { slug, all, stale } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;

            if stale {
                let pages = engine.list_stale_pages().await?;
                if pages.is_empty() {
                    println!("All pages are up-to-date.");
                } else {
                    let pb = indicatif::ProgressBar::new(pages.len() as u64);
                    pb.set_style(
                        indicatif::ProgressStyle::with_template(
                            "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}"
                        )
                        .unwrap()
                        .progress_chars("=>-"),
                    );
                    pb.set_message("embedding stale pages...");
                    let mut errors = 0usize;
                    for page in pages.iter() {
                        pb.set_message(page.slug.clone());
                        match engine.chunk_and_embed_page(page).await {
                            Ok(_) => {}
                            Err(e) => {
                                pb.println(format!("  WARN {}: {}", page.slug, e));
                                errors += 1;
                            }
                        }
                        pb.inc(1);
                    }
                    pb.finish_with_message(format!(
                        "done ({} ok, {} errors)",
                        pages.len() - errors, errors
                    ));
                }
            } else if all {
                let pages = engine.list_pages(None, None).await?;
                let pb = indicatif::ProgressBar::new(pages.len() as u64);
                pb.set_style(
                    indicatif::ProgressStyle::with_template(
                        "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}"
                    )
                    .unwrap()
                    .progress_chars("=>-"),
                );
                pb.set_message("embedding...");
                let mut errors = 0usize;
                for page in pages.iter() {
                    pb.set_message(page.slug.clone());
                    match engine.chunk_and_embed_page(page).await {
                        Ok(_) => {}
                        Err(e) => {
                            pb.println(format!("  WARN {}: {}", page.slug, e));
                            errors += 1;
                        }
                    }
                    pb.inc(1);
                }
                pb.finish_with_message(format!(
                    "complete ({} ok, {} errors)",
                    pages.len() - errors, errors
                ));
            } else if let Some(s) = slug {
                let page = engine.get_page(&s).await?;
                match engine.chunk_and_embed_page(&page).await {
                    Ok(_) => println!("Embedded: {}", s),
                    Err(e) => eprintln!("Error embedding {}: {}", s, e),
                }
            } else {
                eprintln!("Provide a slug, --all, or --stale");
                std::process::exit(1);
            }
        }
        Commands::Extract { slug, all } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;

            if all {
                let pages = engine.list_pages(None, None).await?;
                for page in &pages {
                    let full_content = format!("{} {}", page.compiled_truth, page.timeline);
                    let links = extract_links(&full_content);
                    if !links.is_empty() {
                        println!("{}:", page.slug);
                        for link in links {
                            println!("  -> {} ({})", link.target_slug, link.edge_type);
                        }
                    }
                }
            } else if let Some(s) = slug {
                let page = engine.get_page(&s).await?;
                let full_content = format!("{} {}", page.compiled_truth, page.timeline);
                let links = extract_links(&full_content);
                println!("Links in {}:", s);
                for link in links {
                    println!("  -> {} ({})", link.target_slug, link.edge_type);
                    if let Some(ctx) = &link.context {
                        println!("     Context: {}", ctx);
                    }
                }
            } else {
                eprintln!("Either --all or a slug must be provided");
            }
        }
        Commands::GraphQuery { slug, edge_type, r#type, depth, direction }
        | Commands::Graph { slug, edge_type, r#type, depth, direction } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            // --type is an alias for --edge-type
            let effective_edge_type = edge_type.or(r#type);
            let edges = engine.graph_query(&slug, effective_edge_type.as_deref(), depth, &direction).await?;
            println!("Graph query for '{}' (depth={}, direction={}):", slug, depth, direction);
            if let Some(ref et) = effective_edge_type {
                println!("  Filtered by edge type: {}", et);
            }
            for edge in edges {
                println!("  [{}] {} (depth={})", edge.edge_type, edge.target, edge.depth);
                if let Some(ctx) = &edge.context {
                    let first = ctx.split("\n\n---\n\n").next().unwrap_or(ctx);
                    let preview: String = first.chars().take(160).collect();
                    let preview = if first.chars().count() > 160 { format!("{}…", preview) } else { preview };
                    println!("     Context: {}", preview);
                }
            }
        }
        Commands::Backlinks { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let links = engine.backlinks(&slug).await?;
            println!("Backlinks to '{}':", slug);
            if links.is_empty() {
                println!("  (none — no other page links to this page)");
            }
            for link in links {
                println!("  <- {} ({})", link.target_slug, link.edge_type);
                if let Some(ctx) = &link.context {
                    println!("     Context: {}", ctx);
                }
            }
        }
        Commands::Links { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let links = engine.outlinks(&slug).await?;
            println!("Links from '{}':", slug);
            if links.is_empty() {
                println!("  (none — this page has no outgoing links)");
            }
            for link in &links {
                println!("  -> {} ({})", link.target_slug, link.edge_type);
                if let Some(ctx) = &link.context {
                    let passages: Vec<&str> = ctx.split("\n\n---\n\n").collect();
                    if passages.len() == 1 {
                        let preview: String = ctx.chars().take(200).collect();
                        let preview = if ctx.chars().count() > 200 { format!("{}…", preview) } else { preview };
                        println!("     Context: {}", preview);
                    } else {
                        println!("     Context ({} passages):", passages.len());
                        for (i, p) in passages.iter().enumerate() {
                            let preview: String = p.chars().take(160).collect();
                            let preview = if p.chars().count() > 160 { format!("{}…", preview) } else { preview.to_string() };
                            println!("       [{}] {}", i + 1, preview);
                        }
                    }
                }
            }
        }
        Commands::Link { from, to, r#type, from_chunk, context } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;

            // Resolve context: chunk text takes precedence over free-text --context
            let resolved_context: Option<String> = if let Some(chunk_id) = from_chunk {
                match engine.fetch_chunk_by_id(chunk_id).await? {
                    Some((text, page_slug)) => {
                        // Verify the chunk belongs to the target page
                        let norm_to = MarkdownParser::normalize_slug(&to);
                        if page_slug != norm_to {
                            eprintln!(
                                "Warning: chunk {} belongs to '{}', not '{}'. Proceeding anyway.",
                                chunk_id, page_slug, to
                            );
                        }
                        Some(format!("[chunk:{}] {}", chunk_id, text))
                    }
                    None => {
                        anyhow::bail!("Chunk {} not found. Run `rbrain search` or `rbrain query` to see chunk IDs.", chunk_id);
                    }
                }
            } else {
                context
            };

            engine.add_link(&from, &to, &r#type, resolved_context.as_deref(), from_chunk).await?;
            if from_chunk.is_some() {
                println!("Link added: {} --[{}]--> {} (context from chunk:{})", from, r#type, to, from_chunk.unwrap());
            } else {
                println!("Link added: {} --[{}]--> {}", from, r#type, to);
            }
        }
        Commands::Unlink { from, to, r#type } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let removed = engine.remove_link(&from, &to, r#type.as_deref()).await?;
            if removed > 0 {
                println!("Removed {} link(s): {} --> {}", removed, from, to);
            } else {
                println!("No matching link found: {} --> {}", from, to);
            }
        }
        Commands::Orphans => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let orphans = engine.orphan_pages().await?;
            if orphans.is_empty() {
                println!("No orphan pages — every page has at least one incoming link.");
            } else {
                println!("{} orphan page(s) (no incoming links):", orphans.len());
                for slug in &orphans {
                    println!("  {}", slug);
                }
            }
        }
        Commands::Query { query, expand, limit, r#type, tag } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = rbrain_core::page::Language::detect(&query);

            let chunks = engine.search_with_context_filtered(
                &query, &lang, limit * 3, expand,
                r#type.as_deref(), tag.as_deref(),
            ).await?;

            if chunks.is_empty() {
                println!("No results found for: {}", query);
            } else {
                print_grouped_results(&query, &chunks, limit);
            }
        }
        Commands::Search { query, limit, r#type, tag } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = rbrain_core::page::Language::detect(&query);

            let ids = engine.keyword_search_filtered(
                &query, &lang, limit * 3,
                r#type.as_deref(), tag.as_deref(),
            ).await?;
            if ids.is_empty() {
                println!("No results found for: {}", query);
            } else {
                let chunk_ids: Vec<i64> = ids.iter().map(|(id, _)| *id).collect();
                let texts = engine.fetch_chunks_text(&chunk_ids).await?;
                let text_map: std::collections::HashMap<i64, (String, String)> =
                    texts.into_iter().map(|(id, text, slug)| (id, (text, slug))).collect();

                let chunks: Vec<rbrain_engine::ChunkResult> = ids
                    .into_iter()
                    .filter_map(|(chunk_id, score)| {
                        text_map.get(&chunk_id).map(|(text, slug)| rbrain_engine::ChunkResult {
                            chunk_id,
                            score: score as f64,
                            text: text.clone(),
                            page_slug: slug.clone(),
                        })
                    })
                    .collect();

                print_grouped_results(&query, &chunks, limit);
            }
        }
        Commands::Generate { topic, limit, save, expand } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = rbrain_core::page::Language::detect(&topic);

            eprintln!("Searching for: {}…", topic);
            let wiki = engine.generate_wiki(&topic, &lang, limit, expand).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("{}", wiki);

            if save {
                let slug = topic.to_lowercase().replace(' ', "-").replace(['/', '\\', '.'], "-");
                let page = Page::new(slug.clone(), "wiki".to_string(), wiki);
                engine.put_page(page.clone()).await?;
                eprintln!("\nSaved as wiki page: {}", slug);
                if engine.has_embedder() {
                    eprint!("Embedding… ");
                    match engine.chunk_and_embed_page(&page).await {
                        Ok(_) => eprintln!("done."),
                        Err(e) => eprintln!("warning: embed failed ({}). Run `rbrain embed {}` manually.", e, slug),
                    }
                }
            }
        }
        Commands::Timeline { slug, date, text, source } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let date_str = date.unwrap_or_else(|| {
                chrono::Utc::now().format("%Y-%m-%d").to_string()
            });
            engine.add_timeline_entry(&slug, &date_str, &text, source.as_deref()).await?;
            println!("Timeline entry added to '{}': {} — {}", slug, date_str, text);
        }
        Commands::Take { slug, content, kind } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            engine.add_take(&slug, &content, &kind).await?;
            println!("Take added to '{}' [{}]: {}", slug, kind, content);
        }
        Commands::Takes { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let page = engine.get_page(&slug).await?;
            let takes: Vec<&str> = page.timeline
                .lines()
                .filter(|l| l.contains("[take/"))
                .collect();
            if takes.is_empty() {
                println!("No takes recorded for '{}'.", slug);
            } else {
                println!("Takes for '{}':", slug);
                for take in takes {
                    println!("  {}", take);
                }
            }
        }
        Commands::Think { topic, limit, save, expand } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = rbrain_core::page::Language::detect(&topic);

            eprintln!("Thinking about: {}…", topic);
            let reasoning = engine.think(&topic, &lang, limit, expand).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("{}", reasoning);

            if save {
                let slug = format!(
                    "synthesis/{}",
                    topic.to_lowercase().replace(' ', "-").replace(['/', '\\', '.'], "-")
                );
                let page = Page::new(slug.clone(), "synthesis".to_string(), reasoning);
                engine.put_page(page.clone()).await?;
                eprintln!("\nSaved as synthesis page: {}", slug);
                if engine.has_embedder() {
                    eprint!("Embedding… ");
                    match engine.chunk_and_embed_page(&page).await {
                        Ok(_) => eprintln!("done."),
                        Err(e) => eprintln!("warning: embed failed ({}). Run `rbrain embed {}` manually.", e, slug),
                    }
                }
            }
        }
        Commands::Tag { slug, tag } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            engine.add_tag(&slug, &tag).await?;
            println!("Tag '{}' added to '{}'", tag, slug);
        }
        Commands::Untag { slug, tag } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            engine.remove_tag(&slug, &tag).await?;
            println!("Tag '{}' removed from '{}'", tag, slug);
        }
        Commands::Tags { slug } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let page = engine.get_page(&slug).await?;
            if page.tags.is_empty() {
                println!("No tags on '{}'.", slug);
            } else {
                println!("Tags for '{}':", slug);
                for tag in &page.tags {
                    println!("  {}", tag);
                }
            }
        }
        Commands::Export { dir, format } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let json = format == "json";
            let out_dir = std::path::Path::new(&dir);
            let count = engine.export_pages(out_dir, json).await?;
            println!("Exported {} pages to {} (format={})", count, dir, format);
        }
        Commands::Lint => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let warnings = engine.lint().await?;
            if warnings.is_empty() {
                println!("No issues found — brain looks clean.");
            } else {
                let (warns, infos): (Vec<_>, Vec<_>) = warnings.iter().partition(|(lvl, _, _)| lvl == "WARN");
                for (lvl, slug, msg) in &warnings {
                    println!("{} {}: {}", lvl, slug, msg);
                }
                println!("\n{} warning(s), {} info(s)", warns.len(), infos.len());
                if warns.iter().any(|(_, _, m)| m.contains("not embedded")) {
                    eprintln!("\nTip: run `rbrain embed --stale` to fix missing embeddings.");
                }
            }
        }
        Commands::Doctor { fix } | Commands::Health { fix } => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;

            // ── Stats ────────────────────────────────────────────────────────
            let stats = engine.get_stats().await?;
            let coverage_by_type = engine.embedding_coverage_by_type().await?;
            let top_linked = engine.top_pages_by_indegree(5).await?;
            let orphans = engine.orphan_pages().await?;

            println!("═══ Brain Health Report ════════════════════════════════");

            // Pages by type
            println!("\n── Pages ──────────────────────────────────────────────");
            println!("  Total: {}", stats.total_pages());
            let mut types: Vec<_> = stats.pages_by_type.iter().collect();
            types.sort_by(|a, b| b.1.cmp(a.1));
            for (t, n) in &types {
                println!("  {:12} {}", t, n);
            }

            // Embedding coverage
            println!("\n── Embedding Coverage ─────────────────────────────────");
            println!("  Overall: {:.1}%  ({} / {} chunks)",
                stats.embedding_coverage,
                (stats.total_chunks as f64 * stats.embedding_coverage / 100.0) as i64,
                stats.total_chunks);
            for (t, emb, total) in &coverage_by_type {
                let pct = if *total > 0 { (*emb as f64 / *total as f64) * 100.0 } else { 100.0 };
                let bar_len = (pct / 5.0) as usize; // 20-char bar
                let bar = format!("{}{}", "█".repeat(bar_len), "░".repeat(20 - bar_len));
                println!("  {:12} [{bar}] {:.0}%  ({emb}/{total} chunks)",
                    t, pct, emb = emb, total = total);
            }

            // Graph
            println!("\n── Graph ──────────────────────────────────────────────");
            let link_count = engine.link_count().await?;
            println!("  Links: {}   Orphans: {}   Density: {:.2} edges/page",
                link_count, orphans.len(), stats.graph_density);
            println!("  Top-linked pages:");
            for (slug, deg) in &top_linked {
                println!("    {:3} ← {}", deg, slug);
            }

            // Storage
            println!("\n── Storage ────────────────────────────────────────────");
            let db_size = std::fs::metadata(&config.db_path).map(|m| m.len()).unwrap_or(0);
            let vec_size = std::fs::metadata(&config.vectors_path).map(|m| m.len()).unwrap_or(0);
            let tantivy_size: u64 = walkdir::WalkDir::new(&config.tantivy_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            println!("  SQLite DB:     {}", fmt_bytes(db_size));
            println!("  Vector store:  {}", fmt_bytes(vec_size));
            println!("  Tantivy index: {}", fmt_bytes(tantivy_size));

            // Issues
            println!("\n── Issues ─────────────────────────────────────────────");
            let issues = engine.health_check().await?;
            if issues.is_empty() {
                println!("  ✓ No issues found");
            } else {
                for issue in &issues {
                    println!("  ✗ {}", issue);
                }
                if !orphans.is_empty() {
                    println!("  ✗ {} orphan pages (no incoming links) — run `rbrain orphans`", orphans.len());
                }
                if fix {
                    let fixed_chunks = engine.fix_stale_chunks().await?;
                    if fixed_chunks > 0 { println!("  → Queued {} pages for re-embedding", fixed_chunks); }
                    println!("  Repairs complete.");
                } else {
                    println!("\nTip: run `rbrain doctor --fix` to auto-repair, or `rbrain embed --stale`.");
                }
            }
            println!("═══════════════════════════════════════════════════════");
        }
        Commands::Stats => {
            let config = load_config!();
            let engine = Engine::open(config.clone()).await?;
            let stats = engine.get_stats().await?;

            println!("Brain Statistics:");
            println!("  Total pages: {}", stats.total_pages());
            println!("  Total chunks: {}", stats.total_chunks);
            println!("  Embedding coverage: {:.1}%", stats.embedding_coverage);
            println!("  Graph density: {:.2} edges/page", stats.graph_density);
            println!("  Recent activity (7d): {} updates", stats.recent_activity);

            if !stats.pages_by_type.is_empty() {
                println!("\nPages by type:");
                for (page_type, count) in &stats.pages_by_type {
                    println!("  - {}: {}", page_type, count);
                }
            }

            if !stats.pages_by_language.is_empty() {
                println!("\nPages by language:");
                for (lang, count) in &stats.pages_by_language {
                    println!("  - {}: {}", lang, count);
                }
            }
        }
        Commands::Config { action } => {
            let config = load_config!();
            match action {
                ConfigAction::Show => {
                    println!("Brain path:       {}", config.data_dir.display());
                    println!("Repo dir:         {}", config.repo_dir.display());
                    println!("DB path:          {}", config.db_path.display());
                    println!("Vectors path:     {}", config.vectors_path.display());
                    println!("Tantivy dir:      {}", config.tantivy_dir.display());
                    println!("Embedding dim:    {}", config.embedding_dim);
                    println!("Log level:        {}", config.log_level);
                    println!("\n[qwen]");
                    println!("  base_url:       {}", config.qwen.base_url);
                    println!("  model:          {}", config.qwen.model);
                    let qwen_key = if config.qwen.api_key.is_empty() { "(not set)".to_string() } else { format!("{}…", &config.qwen.api_key[..config.qwen.api_key.len().min(8)]) };
                    println!("  api_key:        {}", qwen_key);
                    println!("\n[deepseek]");
                    println!("  base_url:       {}", config.deepseek.base_url);
                    println!("  model:          {}", config.deepseek.model);
                    let ds_key = if config.deepseek.api_key.is_empty() { "(not set)".to_string() } else { format!("{}…", &config.deepseek.api_key[..config.deepseek.api_key.len().min(8)]) };
                    println!("  api_key:        {}", ds_key);
                }
                ConfigAction::Get { key } => {
                    let value = match key.as_str() {
                        "data_dir" | "brain_dir" => config.data_dir.display().to_string(),
                        "repo_dir" => config.repo_dir.display().to_string(),
                        "db_path" => config.db_path.display().to_string(),
                        "vectors_path" => config.vectors_path.display().to_string(),
                        "tantivy_dir" => config.tantivy_dir.display().to_string(),
                        "embedding_dim" => config.embedding_dim.to_string(),
                        "log_level" => config.log_level.clone(),
                        "qwen.base_url" | "models.embed" => config.qwen.base_url.clone(),
                        "qwen.model" => config.qwen.model.clone(),
                        "qwen.api_key" => "(redacted)".to_string(),
                        "deepseek.base_url" => config.deepseek.base_url.clone(),
                        "deepseek.model" | "models.think" | "models.default" => config.deepseek.model.clone(),
                        "deepseek.api_key" => "(redacted)".to_string(),
                        other => {
                            eprintln!("Unknown config key: {}. Try `rbrain config show` to see available keys.", other);
                            std::process::exit(1);
                        }
                    };
                    println!("{}", value);
                }
            }
        }
        Commands::Serve { action } => {
            let config = load_config!();

            match action {
                ServeAction::Mcp { http } => {
                    // MCP server gets full search capability so brain_query/brain_search work
                    let engine = init_engine_with_search(config.clone(), mock_embed).await?;
                    if let Some(addr) = http {
                        eprintln!("Starting MCP HTTP server on {}", addr);
                        rbrain_mcp::run_http_server(engine, &addr).await?;
                    } else {
                        eprintln!("Starting MCP stdio server");
                        rbrain_mcp::run_stdio_server(engine).await?;
                    }
                }
                ServeAction::Supervisor { concurrency, interval_secs } => {
                    println!("Starting supervisor with concurrency={} interval={}s", concurrency, interval_secs);
                    println!("Press Ctrl+C to stop");

                    let engine = Engine::open(config.clone()).await?;
                    let db = rbrain_db::open_database(&config.db_path).await?;
                    let queue = Arc::new(rbrain_worker::JobQueue::new(db));

                    let mut worker = rbrain_worker::Worker::new(queue, concurrency);
                    worker.register_handler(Arc::new(rbrain_worker::EmbedPageHandler::new(engine.clone())));
                    worker.register_handler(Arc::new(rbrain_worker::SyncRepoHandler::new(engine.clone())));
                    worker.register_handler(Arc::new(rbrain_worker::ExtractLinksHandler::new(engine.clone())));

                    let (tx, rx) = tokio::sync::watch::channel(false);

                    tokio::select! {
                        result = worker.run(rx) => {
                            if let Err(e) = result {
                                eprintln!("Worker error: {}", e);
                            }
                        }
                        _ = tokio::signal::ctrl_c() => {
                            println!("\nReceived shutdown signal...");
                            let _ = tx.send(true);
                        }
                    }

                    println!("Supervisor stopped");
                }
            }
        }
        Commands::Jobs { action } => {
            let config = load_config!();
            let db = rbrain_db::open_database(&config.db_path).await?;
            let queue = Arc::new(rbrain_worker::JobQueue::new(db));

            match action {
                JobsAction::Submit { name, params, queue: queue_name, priority } => {
                    let params_json: serde_json::Value = serde_json::from_str(&params)
                        .map_err(|e| anyhow::anyhow!("Invalid JSON params: {}", e))?;

                    let job_id = queue.submit_job(
                        &name,
                        &params_json,
                        queue_name.as_deref(),
                        priority,
                        None,
                        None,
                    ).await?;

                    println!("Job submitted: id={}", job_id);
                }
                JobsAction::List { status, limit } => {
                    let status_filter = status.map(|s| s.parse::<rbrain_worker::JobStatus>().map_err(|e| anyhow::anyhow!("{}", e))).transpose()?;
                    let jobs = queue.list_jobs(status_filter, limit).await?;

                    println!("{:<6} {:<12} {:<20} {:<10} {:<8} {:<20}",
                        "ID", "Status", "Name", "Attempts", "Priority", "Created");
                    println!("{}", "-".repeat(80));

                    for job in jobs {
                        let created = job.created_at.format("%Y-%m-%d %H:%M:%S");
                        println!("{:<6} {:<12} {:<20} {:<10} {:<8} {}",
                            job.id, job.status.to_string(), job.name, job.attempts, job.priority, created);
                    }
                }
                JobsAction::Get { id } => {
                    let job = queue.get_job(id).await?;
                    println!("Job ID: {}", job.id);
                    println!("Name: {}", job.name);
                    println!("Status: {}", job.status);
                    println!("Queue: {}", job.queue);
                    println!("Priority: {}", job.priority);
                    println!("Attempts: {}/{}", job.attempts, job.max_attempts);
                    println!("Params: {}", job.params);
                    if let Some(error) = &job.last_error {
                        println!("Last Error: {}", error);
                    }
                    if let Some(result) = &job.result {
                        println!("Result: {}", result);
                    }
                    let created = job.created_at.format("%Y-%m-%d %H:%M:%S");
                    println!("Created: {}", created);
                    if let Some(started) = job.started_at {
                        println!("Started: {}", started.format("%Y-%m-%d %H:%M:%S"));
                    }
                    if let Some(finished) = job.finished_at {
                        println!("Finished: {}", finished.format("%Y-%m-%d %H:%M:%S"));
                    }
                }
                JobsAction::Cancel { id } => {
                    queue.cancel_job(id).await?;
                    println!("Job {} cancelled (with any descendants)", id);
                }
                JobsAction::Work { concurrency } => {
                    println!("Starting worker with concurrency {}...", concurrency);
                    println!("Press Ctrl+C to stop");

                    let (tx, rx) = tokio::sync::watch::channel(false);

                    let engine = Engine::open(config.clone()).await?;
                    let mut worker = rbrain_worker::Worker::new(queue, concurrency);

                    worker.register_handler(Arc::new(rbrain_worker::EmbedPageHandler::new(engine.clone())));
                    worker.register_handler(Arc::new(rbrain_worker::SyncRepoHandler::new(engine.clone())));
                    worker.register_handler(Arc::new(rbrain_worker::ExtractLinksHandler::new(engine.clone())));

                    tokio::select! {
                        result = worker.run(rx) => {
                            if let Err(e) = result {
                                eprintln!("Worker error: {}", e);
                            }
                        }
                        _ = tokio::signal::ctrl_c() => {
                            println!("\nReceived shutdown signal...");
                            let _ = tx.send(true);
                        }
                    }

                    println!("Worker stopped");
                }
                JobsAction::Stats => {
                    let stats = queue.get_stats().await?;
                    println!("Job Statistics:");
                    println!("  Pending:   {}", stats.pending);
                    println!("  Running:   {}", stats.running);
                    println!("  Done:      {}", stats.done);
                    println!("  Failed:    {}", stats.failed);
                    println!("  Cancelled: {}", stats.cancelled);
                    println!("  Total:     {}",
                        stats.pending + stats.running + stats.done + stats.failed + stats.cancelled);
                }
            }
        }
        Commands::Dream { stage } => {
            let config = load_config!();
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            engine.run_dream_cycle(stage.as_deref()).await?;
        }
    }

    Ok(())
}

/// Group chunks by page_slug, sort pages by best score, print up to `page_limit` pages.
fn print_grouped_results(query: &str, chunks: &[rbrain_engine::ChunkResult], page_limit: usize) {
    // Group: slug → (best_score, ordered chunks)
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut pages: Vec<(String, f64, Vec<&rbrain_engine::ChunkResult>)> = Vec::new();

    for chunk in chunks {
        if let Some(&idx) = seen.get(chunk.page_slug.as_str()) {
            let entry = &mut pages[idx];
            if chunk.score > entry.1 { entry.1 = chunk.score; }
            entry.2.push(chunk);
        } else {
            seen.insert(&chunk.page_slug, pages.len());
            pages.push((chunk.page_slug.clone(), chunk.score, vec![chunk]));
        }
    }

    pages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    pages.truncate(page_limit);

    let total_pages = pages.len();
    let total_chunks: usize = pages.iter().map(|(_, _, ch)| ch.len()).sum();
    println!("Found {} page(s) / {} chunk(s) for: {}\n", total_pages, total_chunks, query);

    for (rank, (slug, best_score, page_chunks)) in pages.iter().enumerate() {
        let n = page_chunks.len();
        println!(
            "[{}] {} — {} chunk{} (best score={:.4})",
            rank + 1, slug, n, if n == 1 { "" } else { "s" }, best_score
        );
        // Show up to 2 chunk previews per page with chunk_id for evidence linking
        for chunk in page_chunks.iter().take(2) {
            let preview: String = chunk.text.chars().take(180).collect();
            let preview = if chunk.text.chars().count() > 180 {
                format!("{}…", preview)
            } else {
                preview
            };
            println!("    ↳ [chunk:{}] {}", chunk.chunk_id, preview);
        }
        if n > 2 {
            println!("    ↳ … ({} more chunks, use --show-chunks to list all)", n - 2);
        }
        println!();
    }
}

/// Add `.rbrain/` to the project's .gitignore (create the file if absent).
fn update_gitignore(project_dir: &std::path::Path) -> anyhow::Result<()> {
    use std::io::Write;
    let gitignore = project_dir.join(".gitignore");
    let entry = ".rbrain/\n";

    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore)?;
        if !content.contains(".rbrain/") {
            let mut f = std::fs::OpenOptions::new().append(true).open(&gitignore)?;
            f.write_all(entry.as_bytes())?;
        }
    } else {
        std::fs::write(&gitignore, entry)?;
    }
    Ok(())
}

fn fmt_bytes(n: u64) -> String {
    if n < 1024 { format!("{} B", n) }
    else if n < 1024 * 1024 { format!("{:.1} KB", n as f64 / 1024.0) }
    else if n < 1024 * 1024 * 1024 { format!("{:.1} MB", n as f64 / 1024.0 / 1024.0) }
    else { format!("{:.2} GB", n as f64 / 1024.0 / 1024.0 / 1024.0) }
}


async fn init_engine_with_search(config: Config, mock_embed: bool) -> anyhow::Result<Engine> {
    let keyword_index = Arc::new(TantivyIndex::new(config.tantivy_dir.clone())?);
    let vector_store = Arc::new(UsearchStore::new(config.vectors_path.clone(), config.embedding_dim)?);

    let embedder: Arc<dyn Embedder> = if mock_embed {
        Arc::new(MockEmbedder::new(config.embedding_dim))
    } else {
        match QwenEmbedder::from_config(&config.qwen) {
            Ok(e) => Arc::new(e),
            Err(e) => {
                eprintln!("Warning: failed to init Qwen embedder ({}). Run with --mock-embed for offline testing.", e);
                return Ok(Engine::open(config).await?);
            }
        }
    };

    Ok(Engine::open_with_search(config, embedder, vector_store, keyword_index).await?)
}

