use clap::{Parser, Subcommand};
use rbrain_core::config::Config;
use rbrain_core::embedder::Embedder;
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
struct Cli {
    /// Use deterministic mock embedder (no API key needed, for testing)
    #[arg(long, global = true)]
    mock_embed: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialise a project-local brain in the current directory (creates .rbrain/)
    Init,
    /// Create or update a page (reads from stdin or --file; auto re-embeds on save)
    Put {
        slug: String,
        #[arg(long, help = "Page type: note, wiki, book, etc. (default: note)")]
        r#type: Option<String>,
        #[arg(long, help = "Read content from file instead of stdin")]
        file: Option<String>,
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
    Sync,
    /// Embed pages into the vector index
    Embed {
        #[arg(help = "Page slug to embed (omit with --all to embed everything)")]
        slug: Option<String>,
        #[arg(long, help = "Embed all pages that are missing embeddings")]
        all: bool,
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
        #[arg(long, default_value = "2", help = "Traversal depth (max 5)")]
        depth: usize,
        #[arg(long, default_value = "out", help = "Direction: out, in, or both")]
        direction: String,
    },
    /// Show all pages that link to a given page
    Backlinks {
        slug: String,
    },
    /// Hybrid search (vector + keyword + RRF), results grouped by page
    Query {
        query: String,
        #[arg(long, help = "Use LLM query expansion for better recall (extra API call)")]
        expand: bool,
        #[arg(short, long, default_value = "10", help = "Max pages to return")]
        limit: usize,
    },
    /// Keyword-only search (no embedder needed), results grouped by page
    Search {
        query: String,
        #[arg(short, long, default_value = "10", help = "Max pages to return")]
        limit: usize,
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
    /// Run health checks and optionally fix common issues
    Doctor {
        #[arg(long, help = "Attempt to fix detected issues automatically")]
        fix: bool,
    },
    /// Show brain statistics (pages, chunks, embedding coverage)
    Stats,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let mock_embed = cli.mock_embed;

    match cli.command {
        Commands::Init => {
            let cwd = std::env::current_dir()?;
            let local_dir = cwd.join(".rbrain");
            std::fs::create_dir_all(&local_dir)?;
            std::fs::create_dir_all(local_dir.join("tantivy"))?;
            std::fs::create_dir_all(local_dir.join("dictionaries"))?;

            // Add .rbrain/ to .gitignore so DB files are never committed.
            update_gitignore(&cwd)?;

            let config = Config::load()?;
            Engine::open(config.clone()).await?;

            if config.is_local() {
                println!("Brain initialised (project-local): {}", local_dir.display());
            } else {
                println!("Brain initialised (global): {}", config.data_dir.display());
            }
        }
        Commands::Put { slug, r#type, file } => {
            let config = Config::load()?;
            // Use search engine so we can re-embed after saving.
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;

            let content = if let Some(f) = file {
                std::fs::read_to_string(f)?
            } else {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };

            let page = Page::new(
                slug,
                r#type.unwrap_or_else(|| "note".to_string()),
                content,
            );

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
            let config = Config::load()?;
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
                    println!("Content:\n{}", page.compiled_truth);
                }
                Err(_) => {
                    eprintln!("Page not found: {}", slug);
                    return Ok(());
                }
            }
        }
        Commands::Delete { slug } => {
            let config = Config::load()?;
            let engine = Engine::open(config.clone()).await?;
            engine.delete_page(&slug).await?;
            println!("Page deleted");
        }
        Commands::List { r#type, tag, json, limit } => {
            let config = Config::load()?;
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
            let config = Config::load()?;
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
        Commands::Sync => {
            let config = Config::load()?;
            let engine = Engine::open(config.clone()).await?;
            let (imported, updated, orphaned) = engine.sync().await?;
            println!("Sync complete:");
            println!("  New: {}", imported.len());
            println!("  Updated: {}", updated.len());
            println!("  Orphaned: {}", orphaned.len());
            if !orphaned.is_empty() {
                println!("  Orphaned pages: {}", orphaned.join(", "));
            }
        }
        Commands::Embed { slug, all } => {
            let config = Config::load()?;
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            
            if all {
                let pages = engine.list_pages(None, None).await?;
                println!("Embedding {} pages...", pages.len());
                for (idx, page) in pages.iter().enumerate() {
                    match engine.chunk_and_embed_page(page).await {
                        Ok(_) => println!("  [{}/{}] {}", idx + 1, pages.len(), page.slug),
                        Err(e) => println!("  [{}/{}] {} - Error: {}", idx + 1, pages.len(), page.slug, e),
                    }
                }
                println!("Embedding complete!");
            } else if let Some(s) = slug {
                let page = engine.get_page(&s).await?;
                match engine.chunk_and_embed_page(&page).await {
                    Ok(_) => println!("Embedded: {}", s),
                    Err(e) => println!("Error embedding {}: {}", s, e),
                }
            } else {
                eprintln!("Either --all or a slug must be provided");
                std::process::exit(1);
            }
        }
        Commands::Extract { slug, all } => {
            let config = Config::load()?;
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
        Commands::GraphQuery { slug, edge_type, depth, direction } => {
            let config = Config::load()?;
            let engine = Engine::open(config.clone()).await?;
            let edges = engine.graph_query(&slug, edge_type.as_deref(), depth, &direction).await?;
            println!("Graph query for '{}' (depth={}, direction={}):", slug, depth, direction);
            if let Some(et) = edge_type {
                println!("  Filtered by edge type: {}", et);
            }
            for edge in edges {
                println!("  [{}] {} (depth={})", edge.edge_type, edge.target, edge.depth);
            }
        }
        Commands::Backlinks { slug } => {
            let config = Config::load()?;
            let engine = Engine::open(config.clone()).await?;
            let links = engine.backlinks(&slug).await?;
            println!("Backlinks to '{}':", slug);
            for link in links {
                println!("  <- {} ({})", link.target_slug, link.edge_type);
                if let Some(ctx) = &link.context {
                    println!("     Context: {}", ctx);
                }
            }
        }
        Commands::Query { query, expand, limit } => {
            let config = Config::load()?;
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = detect_lang(&query);

            let chunks = engine.search_with_context(&query, &lang, limit * 3, expand).await?;

            if chunks.is_empty() {
                println!("No results found for: {}", query);
            } else {
                print_grouped_results(&query, &chunks, limit);
            }
        }
        Commands::Search { query, limit } => {
            let config = Config::load()?;
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = detect_lang(&query);

            let ids = engine.keyword_search(&query, &lang, limit * 3).await?;
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
            let config = Config::load()?;
            let engine = init_engine_with_search(config.clone(), mock_embed).await?;
            let lang = detect_lang(&topic);

            eprintln!("Searching for: {}…", topic);
            let wiki = engine.generate_wiki(&topic, &lang, limit, expand).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("{}", wiki);

            if save {
                let slug = topic.to_lowercase().replace(' ', "-").replace(['/', '\\', '.'], "-");
                let page = Page::new(slug.clone(), "wiki".to_string(), wiki);
                engine.put_page(page).await?;
                eprintln!("\nSaved as wiki page: {}", slug);
            }
        }
        Commands::Doctor { fix } => {
            let config = Config::load()?;
            let engine = Engine::open(config.clone()).await?;

            let issues = engine.health_check().await?;

            if issues.is_empty() {
                println!("Brain is healthy!");
            } else {
                println!("Found {} issues:", issues.len());
                for issue in &issues {
                    println!("  - {}", issue);
                }

                if fix {
                    let fixed_chunks = engine.fix_stale_chunks().await?;
                    if fixed_chunks > 0 {
                        println!("  Queued {} pages for re-embedding", fixed_chunks);
                    }

                    let fixed_orphans = engine.fix_orphan_pages().await?;
                    if fixed_orphans > 0 {
                        println!("  Removed {} orphan pages", fixed_orphans);
                    }

                    println!("Brain repairs complete!");
                } else {
                    std::process::exit(1);
                }
            }
        }
        Commands::Stats => {
            let config = Config::load()?;
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
        Commands::Serve { action } => {
            let config = Config::load()?;

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
            let config = Config::load()?;
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
        // Show up to 2 chunk previews per page
        for chunk in page_chunks.iter().take(2) {
            let preview: String = chunk.text.chars().take(180).collect();
            let preview = if chunk.text.chars().count() > 180 {
                format!("{}…", preview)
            } else {
                preview
            };
            println!("    ↳ {}", preview);
        }
        if n > 2 {
            println!("    ↳ … ({} more chunks)", n - 2);
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

fn detect_lang(text: &str) -> rbrain_core::page::Language {
    match whatlang::detect(text).map(|i| i.lang()) {
        Some(whatlang::Lang::Jpn) => rbrain_core::page::Language::Ja,
        Some(whatlang::Lang::Kor) => rbrain_core::page::Language::Ko,
        Some(whatlang::Lang::Cmn) => rbrain_core::page::Language::ZhHans,
        Some(whatlang::Lang::Eng) => rbrain_core::page::Language::En,
        _ => rbrain_core::page::Language::En,
    }
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

