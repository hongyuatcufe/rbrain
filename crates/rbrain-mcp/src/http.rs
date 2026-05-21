use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rbrain_core::page::Page;
use rbrain_engine::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

/// State shared across HTTP handlers
#[derive(Clone)]
pub struct AppState {
    engine: Engine,
}

impl AppState {
    pub fn new(engine: Engine) -> Self {
        Self { engine }
    }
}

/// MCP tool request format
#[derive(Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// MCP tool response format
#[derive(Serialize)]
pub struct ToolResponse {
    pub content: Vec<ToolContent>,
}

#[derive(Serialize)]
pub struct ToolContent {
    pub r#type: String,
    pub text: String,
}

/// HTTP error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: i32,
}

fn calculate_relevance(query: &str, page: &Page) -> f32 {
    let query_lower = query.to_lowercase();
    let mut score = 0.0_f32;

    if page.title.to_lowercase().contains(&query_lower) {
        score += 100.0;
    }

    if page.compiled_truth.to_lowercase().contains(&query_lower) {
        score += 50.0;
    }

    for tag in &page.tags {
        if tag.to_lowercase().contains(&query_lower) {
            score += 30.0;
        }
    }

    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    for word in query_words {
        if page.title.to_lowercase().contains(word) {
            score += 10.0;
        }
        if page.compiled_truth.to_lowercase().contains(word) {
            score += 5.0;
        }
    }

    score
}

async fn call_tool(
    engine: &Engine,
    name: &str,
    arguments: serde_json::Value,
) -> Result<String, (i32, String)> {
    match name {
        "brain_search" => {
            let query = arguments.get("query")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s);
            let limit = arguments.get("limit")
                .and_then(|v| v.as_i64())
                .map(|l| l.max(1).min(50) as usize)
                .unwrap_or(10);
            let lang = arguments.get("lang")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let pages = engine.list_pages(None, None).await
                .map_err(|e| (-32000, format!("Failed to list pages: {}", e)))?;

            let mut results: Vec<(Page, f32)> = pages.into_iter()
                .filter(|p| {
                    if let Some(lang_code) = &lang {
                        p.language.as_ref().map_or(false, |l| {
                            l.to_string() == *lang_code || 
                            (*lang_code == "zh" && l.to_string().starts_with("zh"))
                        })
                    } else {
                        true
                    }
                })
                .map(|p| {
                    let score = calculate_relevance(query, &p);
                    (p, score)
                })
                .collect();

            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(limit);

            let search_results: Vec<_> = results.into_iter()
                .map(|(p, score)| json!({
                    "slug": p.slug,
                    "title": p.title,
                    "page_type": p.page_type,
                    "score": score,
                }))
                .collect();

            Ok(serde_json::to_string_pretty(&search_results).unwrap_or_default())
        }
        "brain_get" => {
            let slug = arguments.get("slug")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s);
            
            let page = engine.get_page(slug).await
                .map_err(|e| (-32000, format!("Page not found: {}", e)))?;

            let result = json!({
                "slug": page.slug,
                "title": page.title,
                "page_type": page.page_type,
                "tags": page.tags,
                "compiled_truth": page.compiled_truth,
                "timeline": page.timeline,
                "language": page.language.as_ref().map(|l| l.to_string()),
                "updated_at": page.updated_at.to_string(),
            });

            Ok(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "brain_put" => {
            let slug = arguments.get("slug")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s)
                .to_string();
            let content = arguments.get("content")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s)
                .to_string();
            let page_type = arguments.get("page_type")
                .and_then(|v| v.as_str())
                .map_or_else(|| "note", |s| s)
                .to_string();

            let page = Page::new(slug, page_type, content);
            engine.put_page(page).await
                .map_err(|e| (-32000, format!("Failed to save page: {}", e)))?;

            Ok("Page saved successfully".to_string())
        }
        "brain_list" => {
            let filter = arguments.get("filter")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let pages = engine.list_pages(None, filter.as_deref()).await
                .map_err(|e| (-32000, format!("Failed to list pages: {}", e)))?;

            let results: Vec<_> = pages.into_iter()
                .map(|p| json!({
                    "slug": p.slug,
                    "title": p.title,
                    "page_type": p.page_type,
                    "updated_at": p.updated_at.to_string(),
                }))
                .collect();

            Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
        }
        "brain_graph_query" => {
            let slug = arguments.get("slug")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s);
            let edge_type = arguments.get("edge_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let depth = arguments.get("depth")
                .and_then(|v| v.as_i64())
                .map(|d| d.max(1).min(5) as usize)
                .unwrap_or(2);
            let direction = arguments.get("direction")
                .and_then(|v| v.as_str())
                .map_or_else(|| "out", |s| s);

            let edges = engine.graph_query(slug, edge_type.as_deref(), depth, direction).await
                .map_err(|e| (-32000, format!("Graph query failed: {}", e)))?;

            let results: Vec<_> = edges.into_iter()
                .map(|e| json!({
                    "target": e.target,
                    "edge_type": e.edge_type,
                    "depth": e.depth,
                }))
                .collect();

            Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
        }
        "brain_backlinks" => {
            let slug = arguments.get("slug")
                .and_then(|v| v.as_str())
                .map_or_else(|| "", |s| s);

            let links = engine.backlinks(slug).await
                .map_err(|e| (-32000, format!("Failed to get backlinks: {}", e)))?;

            let results: Vec<_> = links.into_iter()
                .map(|l| json!({
                    "source_slug": l.target_slug,
                    "edge_type": l.edge_type,
                    "context": l.context,
                }))
                .collect();

            Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
        }
        "brain_stats" => {
            let pages = engine.list_pages(None, None).await
                .map_err(|e| (-32000, format!("Failed to get stats: {}", e)))?;

            let total_pages = pages.len();
            let by_type: std::collections::HashMap<String, usize> = pages.iter()
                .fold(std::collections::HashMap::new(), |mut acc, p| {
                    *acc.entry(p.page_type.clone()).or_insert(0) += 1;
                    acc
                });

            let stats = json!({
                "total_pages": total_pages,
                "by_type": by_type,
            });

            Ok(serde_json::to_string_pretty(&stats).unwrap_or_default())
        }
        "brain_think" => {
            let topic = arguments.get("topic")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let limit = arguments.get("limit")
                .and_then(|v| v.as_i64())
                .map(|l| l.max(1).min(20) as usize)
                .unwrap_or(12);
            let expand = arguments.get("expand")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let lang = rbrain_core::page::Language::detect(&topic);
            engine.think(&topic, &lang, limit, expand).await
                .map_err(|e| (-32000, format!("Think failed: {}", e)))
        }
        "brain_add_timeline_entry" => {
            let slug = arguments.get("slug")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = arguments.get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let date = arguments.get("date")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
            let source = arguments.get("source")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            engine.add_timeline_entry(&slug, &date, &text, source.as_deref()).await
                .map(|_| format!("Timeline entry added to '{}'", slug))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_add_tag" => {
            let slug = arguments.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tag = arguments.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_string();
            engine.add_tag(&slug, &tag).await
                .map(|_| format!("Tag '{}' added to '{}'", tag, slug))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_remove_tag" => {
            let slug = arguments.get("slug").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tag = arguments.get("tag").and_then(|v| v.as_str()).unwrap_or("").to_string();
            engine.remove_tag(&slug, &tag).await
                .map(|_| format!("Tag '{}' removed from '{}'", tag, slug))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_outlinks" => {
            let slug = arguments.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            let links = engine.outlinks(slug).await
                .map_err(|e| (-32000, format!("Failed: {}", e)))?;
            let results: Vec<_> = links.into_iter()
                .map(|l| json!({
                    "target_slug": l.target_slug,
                    "edge_type": l.edge_type,
                    "context": l.context,
                }))
                .collect();
            Ok(serde_json::to_string_pretty(&results).unwrap_or_default())
        }
        "brain_delete" => {
            let slug = arguments.get("slug").and_then(|v| v.as_str()).unwrap_or("");
            engine.delete_page(slug).await
                .map(|_| format!("Page '{}' deleted", slug))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_link" => {
            let from = arguments.get("from").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let to = arguments.get("to").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let link_type = arguments.get("link_type").and_then(|v| v.as_str()).unwrap_or("related").to_string();
            let context = arguments.get("context").and_then(|v| v.as_str()).map(|s| s.to_string());
            engine.add_link(&from, &to, &link_type, context.as_deref()).await
                .map(|_| format!("Link created: {} --[{}]--> {}", from, link_type, to))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_unlink" => {
            let from = arguments.get("from").and_then(|v| v.as_str()).unwrap_or("");
            let to = arguments.get("to").and_then(|v| v.as_str()).unwrap_or("");
            let link_type = arguments.get("link_type").and_then(|v| v.as_str());
            engine.remove_link(from, to, link_type).await
                .map(|n| format!("Removed {} link(s)", n))
                .map_err(|e| (-32000, format!("Failed: {}", e)))
        }
        "brain_orphans" => {
            let slugs = engine.orphan_pages().await
                .map_err(|e| (-32000, format!("Failed: {}", e)))?;
            Ok(serde_json::to_string_pretty(&slugs).unwrap_or_default())
        }
        _ => Err((-32601, format!("Unknown tool: {}", name))),
    }
}

/// Handle tool calls via HTTP POST /mcp
async fn handle_tool_call(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ToolRequest>,
) -> impl IntoResponse {
    info!("Handling tool call: {}", request.name);

    match call_tool(&state.engine, &request.name, request.arguments).await {
        Ok(result) => {
            let response = ToolResponse {
                content: vec![ToolContent {
                    r#type: "text".to_string(),
                    text: result,
                }],
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err((code, message)) => {
            error!("Tool call failed: {}", message);
            let error_response = ErrorResponse {
                error: message,
                code,
            };
            (StatusCode::BAD_REQUEST, Json(error_response)).into_response()
        }
    }
}

/// List available tools via HTTP GET /mcp/tools
async fn list_tools() -> impl IntoResponse {
    let tools = json!([
        {
            "name": "brain_search",
            "description": "Search pages by title, content, and tags. Returns relevant pages sorted by relevance score.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query string to match against page titles, content, and tags"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (1-50, default: 10)"
                    },
                    "lang": {
                        "type": "string",
                        "description": "Filter by language code (e.g., 'en', 'ja', 'zh-hans', 'zh-hant', 'ko')"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "brain_get",
            "description": "Get a page by slug. Returns full page content including compiled truth and timeline.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "Page slug (URL-friendly identifier, e.g., 'projects/ai-rag')"
                    }
                },
                "required": ["slug"]
            }
        },
        {
            "name": "brain_put",
            "description": "Create or update a page. Writes the page to the knowledge base.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "Page slug (unique identifier)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Markdown content for the compiled truth section"
                    },
                    "page_type": {
                        "type": "string",
                        "description": "Type of page (e.g., 'note', 'project', 'daily'), default: 'note'"
                    }
                },
                "required": ["slug", "content"]
            }
        },
        {
            "name": "brain_list",
            "description": "List all pages with optional tag filter. Returns page summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "description": "Filter pages by tag (optional)"
                    }
                },
                "required": []
            }
        },
        {
            "name": "brain_graph_query",
            "description": "Query the knowledge graph. Traverse outgoing/incoming links from a page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "Starting page slug"
                    },
                    "edge_type": {
                        "type": "string",
                        "description": "Filter by edge type (e.g., 'relates', 'depends'), optional"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Traversal depth (1-5, default: 2)"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["out", "in", "both"],
                        "description": "Direction: 'out' for outgoing, 'in' for incoming, 'both' for both directions"
                    }
                },
                "required": ["slug"]
            }
        },
        {
            "name": "brain_backlinks",
            "description": "Get all pages that link to a given page. Useful for finding related content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "Target page slug to find backlinks for"
                    }
                },
                "required": ["slug"]
            }
        },
        {
            "name": "brain_stats",
            "description": "Get statistics about the knowledge base: total pages and breakdown by type.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "brain_think",
            "description": "Search the knowledge base and run structured deep reasoning: core claims, tensions, working judgment, open questions. Returns a Markdown artifact with inline citations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "topic": { "type": "string", "description": "Topic to reason about" },
                    "limit": { "type": "integer", "description": "Number of context chunks (default 12)" },
                    "expand": { "type": "boolean", "description": "Use LLM query expansion (default false)" }
                },
                "required": ["topic"]
            }
        },
        {
            "name": "brain_add_timeline_entry",
            "description": "Append a dated event or finding to a page's timeline log.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Page slug" },
                    "text": { "type": "string", "description": "Event description" },
                    "date": { "type": "string", "description": "Date in YYYY-MM-DD (default: today)" },
                    "source": { "type": "string", "description": "Source reference e.g. 'book-slug chunk:150'" }
                },
                "required": ["slug", "text"]
            }
        },
        {
            "name": "brain_add_tag",
            "description": "Add a tag to a page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string" },
                    "tag": { "type": "string" }
                },
                "required": ["slug", "tag"]
            }
        },
        {
            "name": "brain_remove_tag",
            "description": "Remove a tag from a page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string" },
                    "tag": { "type": "string" }
                },
                "required": ["slug", "tag"]
            }
        },
        {
            "name": "brain_outlinks",
            "description": "Find all pages this page links to (outgoing links). Complement to brain_backlinks.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Source page slug" }
                },
                "required": ["slug"]
            }
        }
    ]);

    (StatusCode::OK, Json(tools))
}

/// Run the HTTP MCP server
pub async fn run_http_server(engine: Engine, addr: &str) -> anyhow::Result<()> {
    let state = Arc::new(AppState::new(engine));

    let app = Router::new()
        .route("/mcp", post(handle_tool_call))
        .route("/mcp/tools", get(list_tools))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("MCP HTTP server listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
