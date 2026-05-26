use rbrain_core::config::Config;
use rbrain_core::embedder::Embedder;
use rbrain_core::error::{BrainError, Result};
use rbrain_core::keyword_index::KeywordIndex;
use rbrain_core::markdown::MarkdownParser;
use rbrain_core::page::Page;
use rbrain_core::vector_store::VectorStore;
use rbrain_llm::{DeepSeekClient, Intent};
use rbrain_search::chunker::Chunker;
use rbrain_search::keyword_index::TantivyIndex;
use rbrain_search::rrf;
use sha2::{Digest, Sha256};
use sqlx::Row;
use sqlx::SqlitePool;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use strsim::levenshtein;
use walkdir::WalkDir;

use crate::links::{extract_links, LinkRef};

#[derive(Clone)]
pub struct Engine {
    inner: Arc<EngineInner>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

struct EngineInner {
    db: SqlitePool,
    config: Config,
    embedder: Option<Arc<dyn Embedder>>,
    vector_store: Option<Arc<dyn VectorStore>>,
    keyword_index: Option<Arc<TantivyIndex>>,
    deepseek: Option<Arc<DeepSeekClient>>,
}

impl Engine {
    /// Open engine without embedding/vector search capabilities
    pub async fn open(config: Config) -> Result<Self> {
        let db = rbrain_db::open_database(&config.db_path).await?;

        sqlx::migrate!("../../migrations")
            .run(&db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let deepseek = DeepSeekClient::from_config(&config.deepseek).ok().map(Arc::new);

        let keyword_index = Arc::new(TantivyIndex::new(config.tantivy_dir.clone())?);

        Ok(Self {
            inner: Arc::new(EngineInner {
                db,
                config,
                embedder: None,
                vector_store: None,
                keyword_index: Some(keyword_index),
                deepseek,
            }),
        })
    }

    /// Open engine with full embedding and vector search capabilities
    pub async fn open_with_search(
        config: Config,
        embedder: Arc<dyn Embedder>,
        vector_store: Arc<dyn VectorStore>,
        keyword_index: Arc<TantivyIndex>,
    ) -> Result<Self> {
        let db = rbrain_db::open_database(&config.db_path).await?;

        sqlx::migrate!("../../migrations")
            .run(&db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let deepseek = DeepSeekClient::from_config(&config.deepseek).ok().map(Arc::new);

        Ok(Self {
            inner: Arc::new(EngineInner {
                db,
                config,
                embedder: Some(embedder),
                vector_store: Some(vector_store),
                keyword_index: Some(keyword_index),
                deepseek,
            }),
        })
    }

    pub async fn put_page(&self, page: Page) -> Result<()> {
        self.put_page_inner(page, false).await
    }

    pub async fn put_page_force(&self, page: Page) -> Result<()> {
        self.put_page_inner(page, true).await
    }

    async fn put_page_inner(&self, page: Page, force: bool) -> Result<()> {
        let repo_path = self
            .inner
            .config
            .repo_dir
            .join(format!("{}.md", page.slug));

        if !force && repo_path.exists() {
            let existing_content = std::fs::read_to_string(&repo_path)?;
            let existing_hash = MarkdownParser::content_hash(&existing_content);

            let db_hash: Option<String> = sqlx::query_scalar(
                "SELECT content_hash FROM pages WHERE slug = ?1",
            )
            .bind(&page.slug)
            .fetch_optional(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            if let Some(hash) = db_hash {
                if hash != existing_hash {
                    return Err(BrainError::Conflict(
                        "file edited externally; run sync first".to_string(),
                    ));
                }
            }
        }

        // Ensure frontmatter reflects current page fields (type, title, tags).
        // Page::new() starts with an empty frontmatter; programmatic callers set fields
        // on the struct but not the frontmatter Value, so we merge them here.
        let fm = {
            let mut m = match &page.frontmatter {
                serde_json::Value::Object(map) => map.clone(),
                _ => serde_json::Map::new(),
            };
            m.insert("type".to_string(), serde_json::Value::String(page.page_type.clone()));
            if !page.title.is_empty() {
                m.insert("title".to_string(), serde_json::Value::String(page.title.clone()));
            }
            let tags_val: Vec<serde_json::Value> = page.tags.iter()
                .map(|t| serde_json::Value::String(t.clone()))
                .collect();
            m.insert("tags".to_string(), serde_json::Value::Array(tags_val));
            serde_json::Value::Object(m)
        };
        let canonical = MarkdownParser::to_canonical(
            &fm,
            &page.compiled_truth,
            &page.timeline,
        );
        let content_hash = MarkdownParser::content_hash(&canonical);
        let normalized_slug = MarkdownParser::normalize_slug(&page.slug);

        if let Some(parent) = repo_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = repo_path.with_extension("tmp");
        std::fs::write(&tmp_path, &canonical)?;
        let file = std::fs::File::open(&tmp_path)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, &repo_path)?;

        let tags_json = serde_json::to_string(&page.tags)?;
        let frontmatter_json = serde_json::to_string(&page.frontmatter)?;
        let language_str = page.language.as_ref().map(|l| l.to_string());

        sqlx::query(
            "INSERT OR REPLACE INTO pages \
             (slug, page_type, title, tags, frontmatter, compiled_truth, timeline, language, content_hash, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'), datetime('now'))",
        )
        .bind(&normalized_slug)
        .bind(&page.page_type)
        .bind(&page.title)
        .bind(&tags_json)
        .bind(&frontmatter_json)
        .bind(&page.compiled_truth)
        .bind(&page.timeline)
        .bind(&language_str)
        .bind(&content_hash)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        sqlx::query("DELETE FROM links WHERE source_slug = ?1")
            .bind(&normalized_slug)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let full_content = format!("{} {}", page.compiled_truth, page.timeline);
        let links = extract_links(&full_content);
        for link in links {
            sqlx::query(
                "INSERT OR IGNORE INTO links \
                 (source_slug, target_slug, edge_type, context, created_at, chunk_id) \
                 VALUES (?1, ?2, ?3, ?4, datetime('now'), -1)",
            )
            .bind(&normalized_slug)
            .bind(&link.target_slug)
            .bind(&link.edge_type)
            .bind(&link.context)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }

        if self.inner.embedder.is_some() && self.inner.vector_store.is_some() {
            self.submit_embed_job(&page.slug).await?;
        }

        Ok(())
    }

    async fn submit_embed_job(&self, slug: &str) -> Result<()> {
        let params = serde_json::json!({ "slug": slug });
        let params_str = serde_json::to_string(&params)?;

        sqlx::query(
            "INSERT INTO jobs (queue, name, params, status, priority, depth, created_at) \
             VALUES ('default', 'embed_page', ?1, 'pending', 0, 0, datetime('now'))"
        )
        .bind(&params_str)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    /// Returns true when the engine has embedding + vector search configured.
    pub fn has_embedder(&self) -> bool {
        self.inner.embedder.is_some() && self.inner.vector_store.is_some()
    }

    pub async fn chunk_and_embed_page(&self, page: &Page) -> Result<()> {
        let embedder = self.inner.embedder.as_ref().ok_or_else(|| {
            BrainError::ApiUnreachable {
                provider: "embedder".to_string(),
                message: "Embedder not configured — run with a search-enabled engine".to_string(),
            }
        })?;
        let vector_store = self.inner.vector_store.as_ref().ok_or_else(|| {
            BrainError::ApiUnreachable {
                provider: "vector_store".to_string(),
                message: "Vector store not configured".to_string(),
            }
        })?;
        let keyword_index = self.inner.keyword_index.as_ref().ok_or_else(|| {
            BrainError::ApiUnreachable {
                provider: "keyword_index".to_string(),
                message: "Keyword index not configured".to_string(),
            }
        })?;

        // Remove old chunk vectors before deleting DB records (IDs are lost after DELETE).
        let old_chunk_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM chunks WHERE page_slug = ?1"
        )
        .bind(&page.slug)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if !old_chunk_ids.is_empty() {
            if let Some(vs) = &self.inner.vector_store {
                for id in &old_chunk_ids {
                    let _ = vs.delete(*id).await;
                }
            }
            if let Some(ki) = &self.inner.keyword_index {
                for id in &old_chunk_ids {
                    let _ = ki.delete(*id).await;
                }
            }
        }

        sqlx::query("DELETE FROM chunks WHERE page_slug = ?1")
            .bind(&page.slug)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let lang = page.language.clone().unwrap_or(rbrain_core::page::Language::En);

        let chunker = Chunker::new(&lang);

        let compiled_truth_chunks =
            chunker.chunk(&page.compiled_truth, &page.slug, &lang);

        let timeline_chunks = if page.timeline.trim().is_empty() {
            Vec::new()
        } else {
            chunker.chunk(&page.timeline, &page.slug, &lang)
        };

        let mut idx = 0;
        let mut chunk_ids: Vec<i64> = Vec::new();
        let mut chunk_texts: Vec<String> = Vec::new();

        for chunk in compiled_truth_chunks {
            let language_str = chunk.language.to_string();
            let is_compiled_truth = if chunk.is_compiled_truth { 1 } else { 0 };

            let chunk_id: i64 = sqlx::query_scalar(
                "INSERT INTO chunks \
                 (page_slug, chunk_idx, text, is_compiled_truth, language, has_embedding, indexed_in_vectors, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, datetime('now')) \
                 RETURNING id",
            )
            .bind(&chunk.page_slug)
            .bind(idx as i64)
            .bind(&chunk.text)
            .bind(is_compiled_truth)
            .bind(&language_str)
            .fetch_one(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            chunk_ids.push(chunk_id);
            chunk_texts.push(chunk.text);
            idx += 1;
        }

        for chunk in timeline_chunks {
            let language_str = chunk.language.to_string();
            let is_compiled_truth = if chunk.is_compiled_truth { 1 } else { 0 };

            let chunk_id: i64 = sqlx::query_scalar(
                "INSERT INTO chunks \
                 (page_slug, chunk_idx, text, is_compiled_truth, language, has_embedding, indexed_in_vectors, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, datetime('now')) \
                 RETURNING id",
            )
            .bind(&chunk.page_slug)
            .bind(idx as i64)
            .bind(&chunk.text)
            .bind(is_compiled_truth)
            .bind(&language_str)
            .fetch_one(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            chunk_ids.push(chunk_id);
            chunk_texts.push(chunk.text);
            idx += 1;
        }

        let embeddings = embedder.embed_batch(&chunk_texts).await?;

        let mut vector_items = Vec::new();
        for (chunk_id, embedding) in chunk_ids.iter().zip(embeddings.iter()) {
            let embedding_bytes: Vec<u8> = embedding
                .iter()
                .flat_map(|f: &f32| f.to_le_bytes().to_vec())
                .collect();

            sqlx::query(
                "UPDATE chunks SET embedding = ?1, embedding_model = ?2, has_embedding = 1 \
                 WHERE id = ?3",
            )
            .bind(&embedding_bytes)
            .bind("text-embedding-v4")
            .bind(chunk_id)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            vector_items.push((*chunk_id, embedding.clone()));

            let chunk_text = chunk_texts.get(chunk_ids.iter().position(|&id| id == *chunk_id).unwrap_or(0))
                .cloned()
                .unwrap_or_default();
            keyword_index.upsert(*chunk_id, &page.slug, &chunk_text, &lang).await?;
        }

        vector_store.upsert_batch(&vector_items).await?;

        for chunk_id in &chunk_ids {
            sqlx::query("UPDATE chunks SET indexed_in_vectors = 1 WHERE id = ?1")
                .bind(chunk_id)
                .execute(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }

        vector_store.save().await?;
        keyword_index.commit().await?;

        Ok(())
    }

    pub async fn get_page(&self, slug: &str) -> Result<Page> {
        let normalized = MarkdownParser::normalize_slug(slug);

        let row = sqlx::query(
            "SELECT slug, page_type, title, tags, frontmatter, compiled_truth, timeline, language, content_hash, created_at, updated_at \
             FROM pages WHERE slug = ?1",
        )
        .bind(&normalized)
        .fetch_optional(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        .ok_or_else(|| BrainError::Conflict(format!("page not found: {}", normalized)))?;

        let tags: Vec<String> = serde_json::from_str(row.get::<String, _>("tags").as_str())?;
        let frontmatter: serde_json::Value =
            serde_json::from_str(row.get::<String, _>("frontmatter").as_str())?;
        let language = row
            .get::<Option<String>, _>("language")
            .and_then(|l: String| l.parse().ok());

        Ok(Page {
            slug: row.get("slug"),
            page_type: row.get("page_type"),
            title: row.get("title"),
            tags,
            frontmatter,
            compiled_truth: row.get("compiled_truth"),
            timeline: row.get("timeline"),
            language,
            content_hash: row.get("content_hash"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn get_page_by_slug(&self, slug: &str) -> Result<Page> {
        self.get_page(slug).await
    }

    pub async fn delete_page(&self, slug: &str) -> Result<()> {
        let normalized = MarkdownParser::normalize_slug(slug);
        let repo_path = self
            .inner
            .config
            .repo_dir
            .join(format!("{}.md", normalized));

        let chunk_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM chunks WHERE page_slug = ?1"
        )
        .bind(&normalized)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        // Delete from DB first — if index cleanup fails, the page is gone from DB
        // (clean state). Stale index entries are harmless: searches hit the DB to
        // verify existence and will skip them.
        sqlx::query("DELETE FROM chunks WHERE page_slug = ?1")
            .bind(&normalized)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        sqlx::query("DELETE FROM links WHERE source_slug = ?1 OR target_slug = ?1")
            .bind(&normalized)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        sqlx::query("DELETE FROM pages WHERE slug = ?1")
            .bind(&normalized)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if repo_path.exists() {
            std::fs::remove_file(&repo_path)?;
        }

        if let Some(vector_store) = &self.inner.vector_store {
            for chunk_id in &chunk_ids {
                let _ = vector_store.delete(*chunk_id).await;
            }
            let _ = vector_store.save().await;
        }

        if let Some(keyword_index) = &self.inner.keyword_index {
            for chunk_id in &chunk_ids {
                let _ = keyword_index.delete(*chunk_id).await;
            }
            let _ = keyword_index.commit().await;
        }

        Ok(())
    }

    pub async fn list_pages(
        &self,
        page_type: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<Page>> {
        const BASE: &str = "SELECT slug, page_type, title, tags, frontmatter, compiled_truth, timeline, language, content_hash, created_at, updated_at FROM pages";

        let rows = match (page_type, tag) {
            (Some(pt), Some(tg)) => {
                let tag_pattern = format!("%{}%", tg);
                sqlx::query(&format!("{} WHERE page_type = ?1 AND tags LIKE ?2 ORDER BY updated_at DESC", BASE))
                    .bind(pt)
                    .bind(tag_pattern)
                    .fetch_all(&self.inner.db)
                    .await
            }
            (Some(pt), None) => {
                sqlx::query(&format!("{} WHERE page_type = ?1 ORDER BY updated_at DESC", BASE))
                    .bind(pt)
                    .fetch_all(&self.inner.db)
                    .await
            }
            (None, Some(tg)) => {
                let tag_pattern = format!("%{}%", tg);
                sqlx::query(&format!("{} WHERE tags LIKE ?1 ORDER BY updated_at DESC", BASE))
                    .bind(tag_pattern)
                    .fetch_all(&self.inner.db)
                    .await
            }
            (None, None) => {
                sqlx::query(&format!("{} ORDER BY updated_at DESC", BASE))
                    .fetch_all(&self.inner.db)
                    .await
            }
        }
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut pages = Vec::new();
        for row in rows {
            let tags: Vec<String> =
                serde_json::from_str(row.get::<String, _>("tags").as_str())?;
            let frontmatter: serde_json::Value =
                serde_json::from_str(row.get::<String, _>("frontmatter").as_str())?;
            let language = row
                .get::<Option<String>, _>("language")
                .and_then(|l: String| l.parse().ok());

            pages.push(Page {
                slug: row.get("slug"),
                page_type: row.get("page_type"),
                title: row.get("title"),
                tags,
                frontmatter,
                compiled_truth: row.get("compiled_truth"),
                timeline: row.get("timeline"),
                language,
                content_hash: row.get("content_hash"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(pages)
    }


    pub async fn find_page_fuzzy(&self, slug: &str) -> Result<(Page, f64)> {
        let normalized = MarkdownParser::normalize_slug(slug);

        if let Ok(page) = self.get_page(&normalized).await {
            return Ok((page, 1.0));
        }

        let all_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM pages")
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if !all_slugs.is_empty() {
            let mut best_match = None;
            let mut best_score = 0.0_f64;

            for db_slug in &all_slugs {
                let distance = levenshtein(&normalized, db_slug) as f64;
                let max_len = normalized.len().max(db_slug.len()) as f64;
                let score = 1.0 - (distance / max_len);

                if score > best_score {
                    best_score = score;
                    best_match = Some(db_slug);
                }
            }

            if let Some(matched_slug) = best_match {
                return self.get_page(&matched_slug).await.map(|p| (p, best_score));
            }
        }

        Err(BrainError::Conflict(format!("page not found: {}", slug)))
    }

    pub async fn sync(&self) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
        let mut imported = Vec::new();
        let mut updated = Vec::new();
        let mut orphaned = Vec::new();

        let repo_dir = &self.inner.config.repo_dir;
        if !repo_dir.exists() {
            return Ok((imported, updated, orphaned));
        }

        let db_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM pages")
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut db_slug_set: HashSet<String> = db_slugs.into_iter().collect();

        for entry in WalkDir::new(repo_dir)
            .into_iter()
            .filter_entry(|e| {
                // Skip hidden directories and common non-content directories
                let name = e.file_name().to_string_lossy();
                if e.file_type().is_dir() {
                    !name.starts_with('.') && name != "node_modules" && name != "__pycache__"
                        && name != "target" && name != "dist" && name != "build"
                        && !name.ends_with(".dist-info") && !name.ends_with(".data")
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        {
            let path = entry.path();
            let relative = path.strip_prefix(repo_dir)
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            let slug = relative.to_string_lossy()
                .trim_end_matches(".md")
                .replace(std::path::MAIN_SEPARATOR, "/");

            let content = std::fs::read_to_string(path)?;
            let hash = MarkdownParser::content_hash(&content);

            let db_hash: Option<String> = sqlx::query_scalar(
                "SELECT content_hash FROM pages WHERE slug = ?"
            )
            .bind(&slug)
            .fetch_optional(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            match db_hash {
                Some(existing_hash) if existing_hash == hash => {
                    db_slug_set.remove(&slug);
                }
                Some(_) => {
                    self.sync_file_to_db(&slug, &content, &hash).await?;
                    updated.push(slug.clone());
                    db_slug_set.remove(&slug);
                }
                None => {
                    self.sync_file_to_db(&slug, &content, &hash).await?;
                    imported.push(slug.clone());
                    db_slug_set.remove(&slug);
                }
            }
        }

        for slug in db_slug_set {
            orphaned.push(slug);
        }

        Ok((imported, updated, orphaned))
    }

    /// Update the DB record for a file that was edited externally, without
    /// rewriting the file on disk. The raw file body is stored as compiled_truth
    /// so that hand-written markdown (including `---` horizontal rules) is preserved.
    async fn sync_file_to_db(&self, slug: &str, content: &str, hash: &str) -> Result<()> {
        let parse_result = MarkdownParser::parse(content);
        let fm = &parse_result.frontmatter;
        let page_type = fm.get("type").and_then(|v| v.as_str()).unwrap_or("note").to_string();
        let title = fm.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tags: Vec<String> = fm.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        let language = Some(rbrain_core::page::Language::detect(&parse_result.compiled_truth).to_string());
        let normalized_slug = MarkdownParser::normalize_slug(slug);
        let frontmatter_json = serde_json::to_string(fm).unwrap_or_else(|_| "{}".to_string());

        // Store the full body (pre-split) so `---` horizontal rules in hand-written
        // markdown are not lost. compiled_truth holds the verbatim body text.
        let body_start = {
            let mut pos = 0;
            let bytes = content.as_bytes();
            if content.starts_with("---") {
                pos = 3;
                while pos < bytes.len() {
                    if bytes[pos] == b'\n' && content[pos+1..].starts_with("---") {
                        pos += 4;
                        while pos < bytes.len() && bytes[pos] != b'\n' { pos += 1; }
                        if pos < bytes.len() { pos += 1; }
                        break;
                    }
                    pos += 1;
                }
            }
            pos
        };
        let raw_body = content[body_start..].trim().to_string();

        sqlx::query(
            "INSERT OR REPLACE INTO pages \
             (slug, page_type, title, tags, frontmatter, compiled_truth, timeline, language, content_hash, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, ?8, \
                     COALESCE((SELECT created_at FROM pages WHERE slug = ?1), datetime('now')), \
                     datetime('now'))",
        )
        .bind(&normalized_slug)
        .bind(&page_type)
        .bind(&title)
        .bind(&tags_json)
        .bind(&frontmatter_json)
        .bind(&raw_body)
        .bind(&language)
        .bind(hash)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        sqlx::query("DELETE FROM links WHERE source_slug = ?1")
            .bind(&normalized_slug)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let links = extract_links(&raw_body);
        for link in links {
            sqlx::query(
                "INSERT OR IGNORE INTO links \
                 (source_slug, target_slug, edge_type, context, created_at, chunk_id) \
                 VALUES (?1, ?2, ?3, ?4, datetime('now'), -1)",
            )
            .bind(&normalized_slug)
            .bind(&link.target_slug)
            .bind(&link.edge_type)
            .bind(&link.context)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }

        Ok(())
    }

    pub async fn import_dir(&self, dir: &str) -> Result<Vec<String>> {
        let mut imported = Vec::new();

        // When `dir` is a single file, use its parent as the prefix base so the
        // slug becomes the filename (without extension) rather than an empty string.
        let base_path = std::path::Path::new(dir);
        let prefix = if base_path.is_file() {
            base_path.parent().unwrap_or(base_path).to_path_buf()
        } else {
            base_path.to_path_buf()
        };

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                if e.file_type().is_dir() {
                    !name.starts_with('.') && name != "node_modules" && name != "__pycache__"
                        && name != "target" && name != "dist" && name != "build"
                        && !name.ends_with(".dist-info") && !name.ends_with(".data")
                } else {
                    true
                }
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        {
            let path = entry.path();
            let content = std::fs::read_to_string(path)?;
            let hash = MarkdownParser::content_hash(&content);

            let relative = path.strip_prefix(&prefix)
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            let slug = relative.to_string_lossy()
                .trim_end_matches(".md")
                .trim()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let normalized = MarkdownParser::normalize_slug(&slug);

            let db_hash: Option<String> = sqlx::query_scalar(
                "SELECT content_hash FROM pages WHERE slug = ?"
            )
            .bind(&normalized)
            .fetch_optional(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            if let Some(existing) = db_hash {
                if existing == hash {
                    continue;
                }
            }

            let parse_result = MarkdownParser::parse(&content);
            let page_type = parse_result.frontmatter.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("note")
                .to_string();

            let mut page = Page::new(
                normalized.clone(),
                page_type,
                parse_result.compiled_truth,
            );
            page.timeline = parse_result.timeline;
            page.frontmatter = parse_result.frontmatter;
            if let Some(t) = page.frontmatter.get("title").and_then(|v| v.as_str()) {
                page.title = t.to_string();
            }
            if let Some(tags_val) = page.frontmatter.get("tags").and_then(|v| v.as_array()) {
                page.tags = tags_val.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            }

            let full_text = format!("{} {}", page.compiled_truth, page.timeline);
            page.language = Some(rbrain_core::page::Language::detect(&full_text));

            self.put_page(page.clone()).await?;
            if self.has_embedder() {
                if let Err(e) = self.chunk_and_embed_page(&page).await {
                    tracing::warn!("embed failed for {}: {}", page.slug, e);
                }
            }
            imported.push(slug);
        }

        Ok(imported)
    }

    pub async fn keyword_search(&self, query: &str, lang: &rbrain_core::page::Language, k: usize) -> Result<Vec<(i64, f32)>> {
        self.keyword_search_filtered(query, lang, k, None, None).await
    }

    /// Keyword search with optional page_type and tag filters applied post-Tantivy.
    pub async fn keyword_search_filtered(
        &self,
        query: &str,
        lang: &rbrain_core::page::Language,
        k: usize,
        page_type: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<(i64, f32)>> {
        let keyword_index = self
            .inner
            .keyword_index
            .as_ref()
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "search".to_string(),
                message: "Keyword index not configured".to_string(),
            })?;

        // Fetch extra results so filtering doesn't starve the result set
        let raw = keyword_index.search(query, lang, k * 5).await?;
        if raw.is_empty() || (page_type.is_none() && tag.is_none()) {
            return Ok(raw.into_iter().take(k).collect());
        }

        // Build a filtered set of allowed chunk IDs via SQL
        let ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        let mut sql = format!(
            "SELECT c.id FROM chunks c JOIN pages p ON c.page_slug = p.slug WHERE c.id IN ({})",
            placeholders
        );
        let mut conditions = Vec::new();
        if page_type.is_some() {
            conditions.push("p.page_type = ?");
        }
        if tag.is_some() {
            conditions.push("p.tags LIKE ?");
        }
        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut q = sqlx::query(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        if let Some(pt) = page_type {
            q = q.bind(pt);
        }
        if let Some(tg) = tag {
            q = q.bind(format!("%{}%", tg));
        }

        let allowed: std::collections::HashSet<i64> = q
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .iter()
            .map(|r| r.get::<i64, _>("id"))
            .collect();

        Ok(raw
            .into_iter()
            .filter(|(id, _)| allowed.contains(id))
            .take(k)
            .collect())
    }

    /// Hybrid search with optional page_type and tag filters.
    pub async fn search_with_context_filtered(
        &self,
        query: &str,
        lang: &rbrain_core::page::Language,
        k: usize,
        expand: bool,
        page_type: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<ChunkResult>> {
        if page_type.is_none() && tag.is_none() {
            return self.search_with_context(query, lang, k, expand).await;
        }

        // Run full hybrid search with extra headroom, then filter
        let mut ranked = if expand {
            self.expanded_search(query, lang, k * 5).await?
        } else {
            self.hybrid_search(query, lang, k * 5).await?
        };

        // Also run keyword search with the type/tag filter directly, to guarantee
        // that filtered pages aren't crowded out of the global hybrid ranking by
        // many high-scoring pages of other types.
        let filtered_kw = self.keyword_search_filtered(query, lang, k * 5, page_type, tag).await?;
        if !filtered_kw.is_empty() {
            let existing_ids: HashSet<i64> = ranked.iter().map(|(id, _)| *id).collect();
            for (id, score) in filtered_kw {
                if !existing_ids.contains(&id) {
                    ranked.push((id, score as f64));
                }
            }
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }

        let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
        if ids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let mut sql = format!(
            "SELECT c.id FROM chunks c JOIN pages p ON c.page_slug = p.slug WHERE c.id IN ({})",
            placeholders
        );
        let mut conditions = Vec::new();
        if page_type.is_some() {
            conditions.push("p.page_type = ?");
        }
        if tag.is_some() {
            conditions.push("p.tags LIKE ?");
        }
        if !conditions.is_empty() {
            sql.push_str(" AND ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut q = sqlx::query(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        if let Some(pt) = page_type {
            q = q.bind(pt);
        }
        if let Some(tg) = tag {
            q = q.bind(format!("%{}%", tg));
        }

        let allowed: std::collections::HashSet<i64> = q
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
            .iter()
            .map(|r| r.get::<i64, _>("id"))
            .collect();

        let filtered: Vec<(i64, f64)> = ranked
            .into_iter()
            .filter(|(id, _)| allowed.contains(id))
            .take(k)
            .collect();

        let fetch_ids: Vec<i64> = filtered.iter().map(|(id, _)| *id).collect();
        let texts = self.fetch_chunks_text(&fetch_ids).await?;
        let text_map: HashMap<i64, (String, String)> = texts
            .into_iter()
            .map(|(id, text, slug)| (id, (text, slug)))
            .collect();

        Ok(filtered
            .into_iter()
            .filter_map(|(id, score)| {
                text_map.get(&id).map(|(text, slug)| ChunkResult {
                    chunk_id: id,
                    score,
                    text: text.clone(),
                    page_slug: slug.clone(),
                })
            })
            .collect())
    }

    pub async fn hybrid_search(
        &self,
        query: &str,
        lang: &rbrain_core::page::Language,
        k: usize,
    ) -> Result<Vec<(i64, f64)>> {
        let vector_results = self.vector_search(query, k).await?;
        let keyword_results = self.keyword_search(query, lang, k).await?;

        let vector_rrf: Vec<(i64, usize)> = vector_results
            .into_iter()
            .enumerate()
            .map(|(rank, (chunk_id, _))| (chunk_id, rank + 1))
            .collect();

        let keyword_rrf: Vec<(i64, usize)> = keyword_results
            .into_iter()
            .enumerate()
            .map(|(rank, (chunk_id, _))| (chunk_id, rank + 1))
            .collect();

        let fused = rrf(vec![vector_rrf, keyword_rrf], 60.0);

        Ok(fused)
    }

    pub async fn expanded_search(
        &self,
        query: &str,
        lang: &rbrain_core::page::Language,
        k: usize,
    ) -> Result<Vec<(i64, f64)>> {
        let deepseek = self.inner.deepseek.as_ref().cloned();

        let (intent, expansions) = if let Some(client) = deepseek {
            let intent_result = tokio::time::timeout(
                Duration::from_secs(2),
                client.classify_intent(query)
            ).await;

            let expansions_result = tokio::time::timeout(
                Duration::from_secs(2),
                client.expand_query(query, 3)
            ).await;

            let intent = match intent_result {
                Ok(Ok(i)) => i,
                _ => Intent::General,
            };

            let expansions = match expansions_result {
                Ok(Ok(e)) => e,
                _ => vec![query.to_string()],
            };

            (intent, expansions)
        } else {
            (Intent::General, vec![query.to_string()])
        };

        let mut all_queries = vec![query.to_string()];
        all_queries.extend(expansions);

        let mut seen = std::collections::HashSet::new();
        all_queries.retain(|q| seen.insert(q.clone()));

        let _query_embeddings = if let Some(embedder) = &self.inner.embedder {
            match embedder.embed_batch(&all_queries).await {
                Ok(embs) => embs,
                Err(_) => return self.hybrid_search(query, lang, k).await,
            }
        } else {
            return self.hybrid_search(query, lang, k).await;
        };

        let mut all_results: Vec<Vec<(i64, usize)>> = Vec::new();

        for (idx, _) in all_queries.iter().enumerate() {
            let variant = &all_queries[idx];

            let vector_results = self.vector_search(variant, k).await.unwrap_or_default();
            let keyword_results = self.keyword_search(variant, lang, k).await.unwrap_or_default();

            let vector_rrf: Vec<(i64, usize)> = vector_results
                .into_iter()
                .enumerate()
                .map(|(rank, (chunk_id, _))| (chunk_id, rank + 1))
                .collect();

            let keyword_rrf: Vec<(i64, usize)> = keyword_results
                .into_iter()
                .enumerate()
                .map(|(rank, (chunk_id, _))| (chunk_id, rank + 1))
                .collect();

            if !vector_rrf.is_empty() {
                all_results.push(vector_rrf);
            }
            if !keyword_rrf.is_empty() {
                all_results.push(keyword_rrf);
            }
        }

        let fused = rrf(all_results, 60.0);

        let boosted = if matches!(intent, Intent::Entity) {
            self.apply_backlink_boost(&fused).await?
        } else {
            fused
        };

        Ok(boosted)
    }

    async fn apply_backlink_boost(&self, results: &[(i64, f64)]) -> Result<Vec<(i64, f64)>> {
        if results.is_empty() {
            return Ok(results.to_vec());
        }

        let chunk_ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
        let placeholders: String = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

        let query = format!("SELECT id, page_slug FROM chunks WHERE id IN ({})", placeholders);
        let mut sql_query = sqlx::query(&query);
        for chunk_id in &chunk_ids {
            sql_query = sql_query.bind(chunk_id);
        }

        let rows = sql_query
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut chunk_to_slug: HashMap<i64, String> = HashMap::new();
        for row in rows {
            chunk_to_slug.insert(row.get::<i64, _>("id"), row.get("page_slug"));
        }

        let unique_slugs: Vec<String> = chunk_to_slug.values().cloned().collect();
        if unique_slugs.is_empty() {
            return Ok(results.to_vec());
        }

        let slug_placeholders: String = unique_slugs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let indegree_query = format!("SELECT slug, indegree FROM page_stats WHERE slug IN ({})", slug_placeholders);

        let mut indegree_sql = sqlx::query(&indegree_query);
        for slug in &unique_slugs {
            indegree_sql = indegree_sql.bind(slug);
        }

        let indegree_rows = indegree_sql
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut indegrees: HashMap<String, i64> = HashMap::new();
        for row in indegree_rows {
            indegrees.insert(row.get("slug"), row.get::<i64, _>("indegree"));
        }

        let mut boosted_results: Vec<(i64, f64)> = results
            .iter()
            .map(|&(chunk_id, score)| {
                let slug = chunk_to_slug.get(&chunk_id);
                let indegree = slug.and_then(|s| indegrees.get(s)).copied().unwrap_or(0) as f64;
                let boost = 1.0 + indegree.ln_1p() * 0.15;
                (chunk_id, score * boost)
            })
            .collect();

        boosted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(boosted_results)
    }

    pub async fn vector_search(&self, query: &str, k: usize) -> Result<Vec<(i64, f32)>> {
        let embedder = self
            .inner
            .embedder
            .as_ref()
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "search".to_string(),
                message: "Vector search not configured".to_string(),
            })?;
        let vector_store = self
            .inner
            .vector_store
            .as_ref()
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "search".to_string(),
                message: "Vector store not configured".to_string(),
            })?;

        let query_embedding = embedder.embed_one(query).await?;
        let results = vector_store.search(&query_embedding, k).await?;

        if results.is_empty() {
            return Ok(results);
        }

        let chunk_ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
        let placeholders: String = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("SELECT id, page_slug FROM chunks WHERE id IN ({})", placeholders);

        let mut sql_query = sqlx::query(&query);
        for chunk_id in &chunk_ids {
            sql_query = sql_query.bind(chunk_id);
        }

        let rows = sql_query
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut chunk_to_slug: HashMap<i64, String> = HashMap::new();
        for row in rows {
            chunk_to_slug.insert(row.get::<i64, _>("id"), row.get("page_slug"));
        }

        let unique_slugs: Vec<String> = chunk_to_slug.values().cloned().collect();
        if unique_slugs.is_empty() {
            return Ok(results);
        }

        let slug_placeholders: String = unique_slugs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let indegree_query = format!("SELECT slug, indegree FROM page_stats WHERE slug IN ({})", slug_placeholders);

        let mut indegree_sql = sqlx::query(&indegree_query);
        for slug in &unique_slugs {
            indegree_sql = indegree_sql.bind(slug);
        }

        let indegree_rows = indegree_sql
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut indegrees: HashMap<String, i64> = HashMap::new();
        for row in indegree_rows {
            indegrees.insert(row.get("slug"), row.get::<i64, _>("indegree"));
        }

        let mut boosted_results: Vec<(i64, f32)> = results
            .into_iter()
            .map(|(chunk_id, score)| {
                let slug = chunk_to_slug.get(&chunk_id);
                let indegree = slug.and_then(|s| indegrees.get(s)).copied().unwrap_or(0) as f64;
                let boost = 1.0 + indegree.ln_1p() * 0.1;
                (chunk_id, score * boost as f32)
            })
            .collect();

        boosted_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(boosted_results)
    }

    /// Fetch chunk text and page_slug for a set of chunk IDs, preserving rank order.
    pub async fn fetch_chunks_text(&self, ids: &[i64]) -> Result<Vec<(i64, String, String)>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("SELECT id, text, page_slug FROM chunks WHERE id IN ({})", placeholders);
        let mut sql = sqlx::query(&query);
        for id in ids {
            sql = sql.bind(id);
        }
        let rows = sql
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut map: HashMap<i64, (String, String)> = rows
            .iter()
            .map(|r| (r.get::<i64, _>("id"), (r.get::<String, _>("text"), r.get::<String, _>("page_slug"))))
            .collect();

        Ok(ids
            .iter()
            .filter_map(|id| map.remove(id).map(|(text, slug)| (*id, text, slug)))
            .collect())
    }

    /// Fetch a single chunk by id. Returns (text, page_slug) or None if not found.
    pub async fn fetch_chunk_by_id(&self, chunk_id: i64) -> Result<Option<(String, String)>> {
        let row = sqlx::query("SELECT text, page_slug FROM chunks WHERE id = ?1")
            .bind(chunk_id)
            .fetch_optional(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(row.map(|r| (r.get::<String, _>("text"), r.get::<String, _>("page_slug"))))
    }

    /// Hybrid search returning ranked chunks with full text context.
    pub async fn search_with_context(
        &self,
        query: &str,
        lang: &rbrain_core::page::Language,
        k: usize,
        expand: bool,
    ) -> Result<Vec<ChunkResult>> {
        let ranked = if expand {
            self.expanded_search(query, lang, k).await?
        } else {
            self.hybrid_search(query, lang, k).await?
        };

        let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
        let texts = self.fetch_chunks_text(&ids).await?;

        let text_map: HashMap<i64, (String, String)> = texts
            .into_iter()
            .map(|(id, text, slug)| (id, (text, slug)))
            .collect();

        Ok(ranked
            .into_iter()
            .filter_map(|(id, score)| {
                text_map.get(&id).map(|(text, slug)| ChunkResult {
                    chunk_id: id,
                    score,
                    text: text.clone(),
                    page_slug: slug.clone(),
                })
            })
            .collect())
    }

    pub async fn backlinks(&self, slug: &str) -> Result<Vec<LinkRef>> {
        let normalized = MarkdownParser::normalize_slug(slug);

        let rows = sqlx::query(
            "SELECT source_slug, edge_type, context, chunk_id FROM links WHERE target_slug = ?1",
        )
        .bind(&normalized)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut links = Vec::new();
        for row in rows {
            let cid_val: i64 = row.get("chunk_id");
            let chunk_id = if cid_val == -1 { None } else { Some(cid_val) };
            links.push(LinkRef {
                target_slug: row.get("source_slug"),
                edge_type: row.get("edge_type"),
                context: row.get("context"),
                chunk_id,
            });
        }

        Ok(links)
    }

    /// List outgoing links from a page (with type and evidence context).
    pub async fn outlinks(&self, slug: &str) -> Result<Vec<LinkRef>> {
        let normalized = MarkdownParser::normalize_slug(slug);

        let rows = sqlx::query(
            "SELECT target_slug, edge_type, context, chunk_id FROM links WHERE source_slug = ?1 ORDER BY edge_type, target_slug",
        )
        .bind(&normalized)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut links = Vec::new();
        for row in rows {
            let cid_val: i64 = row.get("chunk_id");
            let chunk_id = if cid_val == -1 { None } else { Some(cid_val) };
            links.push(LinkRef {
                target_slug: row.get("target_slug"),
                edge_type: row.get("edge_type"),
                context: row.get("context"),
                chunk_id,
            });
        }

        Ok(links)
    }

    /// Add an explicit typed link between two pages (does not require [[wikilink]] syntax).
    /// If a link of the same (source, target, type) already exists and new context is provided,
    /// the context is appended rather than replaced — so multiple chunk passages accumulate.
    pub async fn add_link(
        &self,
        source_slug: &str,
        target_slug: &str,
        edge_type: &str,
        context: Option<&str>,
        chunk_id: Option<i64>,
    ) -> Result<()> {
        let source = MarkdownParser::normalize_slug(source_slug);
        let target = MarkdownParser::normalize_slug(target_slug);
        let cid = chunk_id.unwrap_or(-1);

        // Check if a link already exists
        let existing_context: Option<String> = sqlx::query_scalar(
            "SELECT context FROM links WHERE source_slug = ?1 AND target_slug = ?2 AND edge_type = ?3 AND chunk_id = ?4"
        )
        .bind(&source)
        .bind(&target)
        .bind(edge_type)
        .bind(cid)
        .fetch_optional(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        .flatten();

        let merged_context = match (existing_context, context) {
            (Some(existing), Some(new)) => Some(format!("{}\n\n---\n\n{}", existing, new)),
            (Some(existing), None) => Some(existing),
            (None, new) => new.map(|s| s.to_string()),
        };

        sqlx::query(
            "INSERT INTO links (source_slug, target_slug, edge_type, context, created_at, chunk_id) \
             VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5) \
             ON CONFLICT(source_slug, target_slug, edge_type, chunk_id) DO UPDATE SET context = ?4",
        )
        .bind(&source)
        .bind(&target)
        .bind(edge_type)
        .bind(merged_context.as_deref())
        .bind(cid)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(())
    }

    /// Remove a link between two pages (optionally filter by edge type).
    pub async fn remove_link(
        &self,
        source_slug: &str,
        target_slug: &str,
        edge_type: Option<&str>,
    ) -> Result<u64> {
        let source = MarkdownParser::normalize_slug(source_slug);
        let target = MarkdownParser::normalize_slug(target_slug);
        let result = if let Some(et) = edge_type {
            sqlx::query(
                "DELETE FROM links WHERE source_slug = ?1 AND target_slug = ?2 AND edge_type = ?3",
            )
            .bind(&source)
            .bind(&target)
            .bind(et)
            .execute(&self.inner.db)
            .await
        } else {
            sqlx::query("DELETE FROM links WHERE source_slug = ?1 AND target_slug = ?2")
                .bind(&source)
                .bind(&target)
                .execute(&self.inner.db)
                .await
        }
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(result.rows_affected())
    }

    /// Find pages that have no incoming links (orphan pages).
    pub async fn orphan_pages(&self) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT slug FROM pages \
             WHERE slug NOT IN (SELECT DISTINCT target_slug FROM links) \
             ORDER BY slug",
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(rows)
    }

    /// Total number of links in the graph.
    pub async fn link_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM links")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0))
    }

    /// Return the top-n pages by incoming link count (indegree).
    pub async fn top_pages_by_indegree(&self, n: usize) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT p.slug, COALESCE(ps.indegree, 0) as indegree \
             FROM pages p LEFT JOIN page_stats ps ON p.slug = ps.slug \
             ORDER BY indegree DESC LIMIT ?1"
        )
        .bind(n as i64)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(rows.iter().map(|r| (r.get::<String, _>("slug"), r.get::<i64, _>("indegree"))).collect())
    }

    /// Return (embedded_chunks, total_chunks) counts by page_type.
    pub async fn embedding_coverage_by_type(&self) -> Result<Vec<(String, i64, i64)>> {
        let rows = sqlx::query(
            "SELECT p.page_type, \
             COUNT(c.id) as total, \
             SUM(CASE WHEN c.has_embedding = 1 THEN 1 ELSE 0 END) as embedded \
             FROM pages p LEFT JOIN chunks c ON c.page_slug = p.slug \
             GROUP BY p.page_type ORDER BY total DESC"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(rows.iter().map(|r| (
            r.get::<String, _>("page_type"),
            r.get::<i64, _>("embedded"),
            r.get::<i64, _>("total"),
        )).collect())
    }

    /// Prepend a dated entry to a page's timeline section.
    /// Direct SQL UPDATE — does NOT touch the links table, preserving explicit graph links.
    pub async fn add_timeline_entry(
        &self,
        slug: &str,
        date: &str,
        text: &str,
        source: Option<&str>,
    ) -> Result<()> {
        let normalized = MarkdownParser::normalize_slug(slug);
        let page = self.get_page(&normalized).await?;
        let entry = if let Some(src) = source {
            format!("- {}: {} [Source: {}]", date, text, src)
        } else {
            format!("- {}: {}", date, text)
        };
        let new_timeline = if page.timeline.trim().is_empty() {
            entry
        } else {
            format!("{}\n{}", entry, page.timeline)
        };

        // Write file first; only update DB after filesystem succeeds.
        let repo_path = self.inner.config.repo_dir.join(format!("{}.md", normalized));
        let new_hash = if repo_path.exists() {
            let canonical = MarkdownParser::to_canonical(&page.frontmatter, &page.compiled_truth, &new_timeline);
            std::fs::write(&repo_path, &canonical)?;
            Some(MarkdownParser::content_hash(&canonical))
        } else {
            None
        };

        sqlx::query(
            "UPDATE pages SET timeline = ?1, updated_at = datetime('now') WHERE slug = ?2"
        )
        .bind(&new_timeline)
        .bind(&normalized)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if let Some(hash) = new_hash {
            sqlx::query("UPDATE pages SET content_hash = ?1 WHERE slug = ?2")
                .bind(&hash)
                .bind(&normalized)
                .execute(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }
        Ok(())
    }

    /// Append a short interpretive take to a page's timeline section.
    /// Direct SQL UPDATE — does NOT touch the links table, preserving explicit graph links.
    pub async fn add_take(
        &self,
        slug: &str,
        content: &str,
        kind: &str,
    ) -> Result<()> {
        let normalized = MarkdownParser::normalize_slug(slug);
        let page = self.get_page(&normalized).await?;
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let entry = format!("- [take/{}] {}: {}", kind, date, content);
        let new_timeline = if page.timeline.trim().is_empty() {
            entry
        } else {
            format!("{}\n{}", page.timeline, entry)
        };

        // Write file first; only update DB after filesystem succeeds.
        let repo_path = self.inner.config.repo_dir.join(format!("{}.md", normalized));
        let new_hash = if repo_path.exists() {
            let canonical = MarkdownParser::to_canonical(&page.frontmatter, &page.compiled_truth, &new_timeline);
            std::fs::write(&repo_path, &canonical)?;
            Some(MarkdownParser::content_hash(&canonical))
        } else {
            None
        };

        sqlx::query(
            "UPDATE pages SET timeline = ?1, updated_at = datetime('now') WHERE slug = ?2"
        )
        .bind(&new_timeline)
        .bind(&normalized)
        .execute(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if let Some(hash) = new_hash {
            sqlx::query("UPDATE pages SET content_hash = ?1 WHERE slug = ?2")
                .bind(&hash)
                .bind(&normalized)
                .execute(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }
        Ok(())
    }

    /// Deep-reasoning synthesis: search context, then prompt LLM to reason through
    /// contradictions, open questions, and form a working judgment.
    /// Unlike generate_wiki (output-focused), think is reasoning-artifact-focused.
    pub async fn think(
        &self,
        topic: &str,
        lang: &rbrain_core::page::Language,
        limit: usize,
        expand: bool,
    ) -> Result<String> {
        let deepseek = self.inner.deepseek.as_ref().ok_or_else(|| {
            BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "deepseek.api_key not configured".to_string(),
            }
        })?;

        let chunks = self.search_with_context(topic, lang, limit, expand).await?;
        if chunks.is_empty() {
            return Ok(format!("No relevant content found in brain for: {}", topic));
        }

        let is_cjk = matches!(lang,
            rbrain_core::page::Language::ZhHans
            | rbrain_core::page::Language::ZhHant
            | rbrain_core::page::Language::Ja
            | rbrain_core::page::Language::Ko
        );

        let context = chunks
            .iter()
            .map(|c| {
                if is_cjk {
                    format!("[来源: {} | chunk:{}]\n{}", c.page_slug, c.chunk_id, c.text)
                } else {
                    format!("[source: {} | chunk:{}]\n{}", c.page_slug, c.chunk_id, c.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let (system, user) = if is_cjk {
            let sys = "你是一位严谨的学术研究者。根据提供的原始材料，对给定问题进行深入推理：\
                1. 梳理材料中的核心观点与论据\
                2. 指出材料间的张力、矛盾或空白\
                3. 形成有依据的工作判断（注明不确定之处）\
                4. 列出尚待回答的开放性问题\
                5. 用Markdown格式，包含：## 核心观点 / ## 张力与矛盾 / ## 工作判断 / ## 开放问题\
                引用材料时用 [[slug | chunk:N]] 格式标注来源（slug 和 chunk 编号来自材料头部标注），每个核心论断至少注明一处。";
            let usr = format!("研究问题：{}\n\n材料：\n\n{}", topic, context);
            (sys, usr)
        } else {
            let sys = "You are a rigorous academic researcher. Based on the provided source materials, \
                reason deeply about the given question:\
                1. Identify the core claims and arguments in the materials\
                2. Note tensions, contradictions, or gaps between sources\
                3. Form a working judgment (flagging uncertainty where it exists)\
                4. List open questions that remain unanswered\
                Use Markdown with sections: ## Core Claims / ## Tensions & Gaps / ## Working Judgment / ## Open Questions\
                Cite sources using [[slug | chunk:N]] wikilink format (slug and chunk number from the source header). Each key claim must cite at least one source.";
            let usr = format!("Research question: {}\n\nSources:\n\n{}", topic, context);
            (sys, usr)
        };

        deepseek.chat(system, &user).await
    }

    pub async fn graph_query(
        &self,
        slug: &str,
        edge_type: Option<&str>,
        depth: usize,
        direction: &str,
    ) -> Result<Vec<GraphEdge>> {
        let normalized = MarkdownParser::normalize_slug(slug);

        let mut edges = Vec::new();

        let query = match direction {
            "out" => {
                "WITH RECURSIVE graph AS (
                    SELECT target_slug, edge_type, 1 AS depth
                    FROM links
                    WHERE source_slug = ?1 AND (?2 IS NULL OR edge_type = ?2)
                    UNION ALL
                    SELECT l.target_slug, l.edge_type, g.depth + 1
                    FROM links l
                    JOIN graph g ON l.source_slug = g.target_slug
                    WHERE g.depth < ?3 AND (?2 IS NULL OR l.edge_type = ?2)
                )
                SELECT DISTINCT target_slug, edge_type, depth FROM graph ORDER BY depth"
            }
            "in" => {
                "WITH RECURSIVE graph AS (
                    SELECT source_slug, edge_type, 1 AS depth
                    FROM links
                    WHERE target_slug = ?1 AND (?2 IS NULL OR edge_type = ?2)
                    UNION ALL
                    SELECT l.source_slug, l.edge_type, g.depth + 1
                    FROM links l
                    JOIN graph g ON l.target_slug = g.source_slug
                    WHERE g.depth < ?3 AND (?2 IS NULL OR l.edge_type = ?2)
                )
                SELECT DISTINCT source_slug AS target_slug, edge_type, depth FROM graph ORDER BY depth"
            }
            "both" | _ => {
                "WITH RECURSIVE graph AS (
                    SELECT target_slug AS node, edge_type, 'out' AS dir, 1 AS depth
                    FROM links
                    WHERE source_slug = ?1 AND (?2 IS NULL OR edge_type = ?2)
                    UNION ALL
                    SELECT source_slug AS node, edge_type, 'in' AS dir, 1 AS depth
                    FROM links
                    WHERE target_slug = ?1 AND (?2 IS NULL OR edge_type = ?2)
                    UNION ALL
                    SELECT l.target_slug, l.edge_type, 'out', g.depth + 1
                    FROM links l
                    JOIN graph g ON l.source_slug = g.node
                    WHERE g.dir = 'out' AND g.depth < ?3 AND (?2 IS NULL OR l.edge_type = ?2)
                    UNION ALL
                    SELECT l.source_slug, l.edge_type, 'in', g.depth + 1
                    FROM links l
                    JOIN graph g ON l.target_slug = g.node
                    WHERE g.dir = 'in' AND g.depth < ?3 AND (?2 IS NULL OR l.edge_type = ?2)
                )
                SELECT DISTINCT node AS target_slug, edge_type, depth FROM graph ORDER BY depth"
            }
        };

        let rows = sqlx::query(query)
            .bind(&normalized)
            .bind(edge_type)
            .bind(depth as i64)
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        for row in rows {
            edges.push(GraphEdge {
                target: row.get("target_slug"),
                edge_type: row.get("edge_type"),
                depth: row.get::<i64, _>("depth") as usize,
                context: None,
            });
        }

        // Enrich depth-1 edges with context from the links table
        let depth1_targets: Vec<String> = edges.iter()
            .filter(|e| e.depth == 1)
            .map(|e| e.target.clone())
            .collect();

        if !depth1_targets.is_empty() {
            // Use all-? style (no ?N mixing) to avoid sqlx binding confusion
            let ph = depth1_targets.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let ctx_sql = format!(
                "SELECT target_slug, edge_type, context FROM links WHERE source_slug = ? AND target_slug IN ({})",
                ph
            );
            let mut ctx_q = sqlx::query(&ctx_sql).bind(&normalized);
            for t in &depth1_targets {
                ctx_q = ctx_q.bind(t);
            }
            let ctx_rows = ctx_q
                .fetch_all(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

            let mut ctx_map: HashMap<(String, String), String> = HashMap::new();
            for r in ctx_rows {
                let tgt: String = r.get("target_slug");
                let et: String = r.get("edge_type");
                let ctx: Option<String> = r.get("context");
                if let Some(c) = ctx {
                    ctx_map.insert((tgt, et), c);
                }
            }
            for edge in edges.iter_mut() {
                if edge.depth == 1 {
                    edge.context = ctx_map.get(&(edge.target.clone(), edge.edge_type.clone())).cloned();
                }
            }
        }

        Ok(edges)
    }

    /// Health check - returns list of issues found
    pub async fn health_check(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();

        let db_check: std::result::Result<i64, _> = sqlx::query_scalar("SELECT 1")
            .fetch_one(&self.inner.db)
            .await;
        if db_check.is_err() {
            issues.push("Database connection failed".to_string());
        }

        if let Some(vector_store) = &self.inner.vector_store {
            if let Err(e) = vector_store.save().await {
                issues.push(format!("Vector store error: {}", e));
            }
        }

        let stale_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM chunks WHERE has_embedding = 0 OR indexed_in_vectors = 0"
        )
        .fetch_one(&self.inner.db)
        .await
        .unwrap_or(0);

        if stale_count > 0 {
            issues.push(format!("{} stale chunks need embedding/indexing", stale_count));
        }

        let repo_dir = &self.inner.config.repo_dir;
        if repo_dir.exists() {
            let db_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM pages")
                .fetch_all(&self.inner.db)
                .await
                .unwrap_or_default();

            for slug in db_slugs {
                let md_path = repo_dir.join(format!("{}.md", slug));
                if !md_path.exists() {
                    issues.push(format!("Orphan page: {}", slug));
                }
            }
        }

        Ok(issues)
    }

    /// Get statistics about the knowledge base
    pub async fn get_stats(&self) -> Result<BrainStats> {
        let rows = sqlx::query(
            "SELECT page_type, COUNT(*) as cnt FROM pages GROUP BY page_type"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut pages_by_type: HashMap<String, i64> = HashMap::new();
        for row in rows {
            let page_type: String = row.get("page_type");
            let count: i64 = row.get("cnt");
            pages_by_type.insert(page_type, count);
        }

        let lang_rows = sqlx::query(
            "SELECT language, COUNT(*) as cnt FROM pages GROUP BY language"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut pages_by_language: HashMap<String, i64> = HashMap::new();
        for row in lang_rows {
            let lang: Option<String> = row.get("language");
            let count: i64 = row.get("cnt");
            pages_by_language.insert(lang.unwrap_or("unknown".to_string()), count);
        }

        let total_chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0);

        let with_embedding: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks WHERE has_embedding = 1")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0);

        let embedding_coverage = if total_chunks > 0 {
            (with_embedding as f64 / total_chunks as f64) * 100.0
        } else {
            0.0
        };

        let page_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pages")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0);

        let link_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM links")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0);

        let graph_density = if page_count > 0 {
            link_count as f64 / page_count as f64
        } else {
            0.0
        };

        let recent_activity: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pages WHERE updated_at > datetime('now', '-7 days')"
        )
        .fetch_one(&self.inner.db)
        .await
        .unwrap_or(0);

        Ok(BrainStats {
            pages_by_type,
            pages_by_language,
            total_chunks,
            embedding_coverage,
            graph_density,
            recent_activity,
        })
    }

    /// Fix stale chunks by re-embedding them
    pub async fn fix_stale_chunks(&self) -> Result<usize> {
        let stale_slugs: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT page_slug FROM chunks WHERE has_embedding = 0 OR indexed_in_vectors = 0"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let count = stale_slugs.len();

        for slug in stale_slugs {
            self.submit_embed_job(&slug).await?;
        }

        Ok(count)
    }

    /// Delete orphaned pages
    pub async fn fix_orphan_pages(&self) -> Result<usize> {
        let repo_dir = &self.inner.config.repo_dir;
        if !repo_dir.exists() {
            return Ok(0);
        }

        let db_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM pages")
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut orphaned = Vec::new();
        for slug in db_slugs {
            let md_path = repo_dir.join(format!("{}.md", slug));
            if !md_path.exists() {
                orphaned.push(slug);
            }
        }

        let count = orphaned.len();

        for slug in orphaned {
            self.delete_page(&slug).await?;
        }

        Ok(count)
    }

    /// Search for context chunks and synthesise a wiki page with the DeepSeek LLM.
    /// Returns the generated Markdown string.
    pub async fn generate_wiki(
        &self,
        topic: &str,
        lang: &rbrain_core::page::Language,
        limit: usize,
        expand: bool,
    ) -> Result<String> {
        let deepseek = self.inner.deepseek.as_ref().ok_or_else(|| {
            BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "DeepSeek not configured — set deepseek.api_key in ~/.rbrain/config.toml"
                    .to_string(),
            }
        })?;

        let chunks = self.search_with_context(topic, lang, limit, expand).await?;

        if chunks.is_empty() {
            return Err(BrainError::ApiUnreachable {
                provider: "search".to_string(),
                message: format!("No relevant content found for: {topic}"),
            });
        }

        let context = chunks
            .iter()
            .map(|c| format!("[来源: {} | chunk:{}]\n{}", c.page_slug, c.chunk_id, c.text))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system = "你是知识库编辑。根据提供的原文材料，生成一篇简洁的Markdown wiki页面。\
            要求：包含一级标题、2-4个核心观点（用##小节）、简短结语。\
            严格基于原文，不添加原文没有的内容。输出简体中文。\
            引用原文时，用 [[slug | chunk:N]] 格式标注来源（slug 和 chunk 编号均来自原文材料头部的标注）。每个核心观点至少标注一处来源。";
        let user = format!(
            "主题：【{topic}】\n\n原文材料：\n\n{context}\n\n请生成wiki页面。"
        );

        deepseek.chat(system, &user).await
    }

    /// Add a tag to a page (no-op if already present).
    pub async fn add_tag(&self, slug: &str, tag: &str) -> Result<()> {
        let mut page = self.get_page(slug).await?;
        if !page.tags.contains(&tag.to_string()) {
            page.tags.push(tag.to_string());
            self.write_tags_to_file_and_db(&page).await?;
        }
        Ok(())
    }

    /// Remove a tag from a page (no-op if not present).
    pub async fn remove_tag(&self, slug: &str, tag: &str) -> Result<()> {
        let mut page = self.get_page(slug).await?;
        let before = page.tags.len();
        page.tags.retain(|t| t != tag);
        if page.tags.len() != before {
            self.write_tags_to_file_and_db(&page).await?;
        }
        Ok(())
    }

    async fn write_tags_to_file_and_db(&self, page: &Page) -> Result<()> {
        let tags_json = serde_json::to_string(&page.tags)?;
        // Write file first; only update DB after filesystem succeeds.
        let repo_path = self.inner.config.repo_dir.join(format!("{}.md", page.slug));
        let new_hash = if repo_path.exists() {
            let content = std::fs::read_to_string(&repo_path)?;
            let updated = update_frontmatter_tags(&content, &page.tags);
            let hash = MarkdownParser::content_hash(&updated);
            std::fs::write(&repo_path, updated)?;
            Some(hash)
        } else {
            None
        };

        sqlx::query("UPDATE pages SET tags = ?1, updated_at = datetime('now') WHERE slug = ?2")
            .bind(&tags_json)
            .bind(&page.slug)
            .execute(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if let Some(hash) = new_hash {
            sqlx::query("UPDATE pages SET content_hash = ?1 WHERE slug = ?2")
                .bind(&hash)
                .bind(&page.slug)
                .execute(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }
        Ok(())
    }

    /// List pages that have no embedded chunks (never embedded, or all chunks stale).
    pub async fn list_stale_pages(&self) -> Result<Vec<Page>> {
        let stale_slugs: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT page_slug FROM chunks WHERE has_embedding = 0 OR indexed_in_vectors = 0
             UNION
             SELECT slug FROM pages
             WHERE slug NOT IN (SELECT DISTINCT page_slug FROM chunks WHERE page_slug IS NOT NULL)"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut pages = Vec::new();
        for slug in stale_slugs {
            if let Ok(page) = self.get_page(&slug).await {
                pages.push(page);
            }
        }
        Ok(pages)
    }

    /// Lint the knowledge base: return (warning_type, slug, message) tuples.
    pub async fn lint(&self) -> Result<Vec<(String, String, String)>> {
        let mut warnings: Vec<(String, String, String)> = Vec::new();

        // Pages with no/empty title
        let rows = sqlx::query("SELECT slug, title FROM pages WHERE title = '' OR title IS NULL")
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        for row in rows {
            warnings.push(("WARN".into(), row.get::<String, _>("slug"), "missing title".into()));
        }

        // Pages with no embedded chunks
        let unembedded: Vec<String> = sqlx::query_scalar(
            "SELECT slug FROM pages WHERE slug NOT IN (SELECT DISTINCT page_slug FROM chunks WHERE has_embedding = 1)"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        for slug in unembedded {
            warnings.push(("WARN".into(), slug, "not embedded (run: rbrain embed --stale)".into()));
        }

        // Orphan pages (no incoming links) — skip for small brains (<= 5 pages)
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pages")
            .fetch_one(&self.inner.db)
            .await
            .unwrap_or(0);
        if total > 5 {
            let orphans = self.orphan_pages().await?;
            for slug in orphans {
                warnings.push(("INFO".into(), slug, "orphan page — no incoming links".into()));
            }
        }

        // Broken links (target page does not exist)
        let broken: Vec<(String, String)> = sqlx::query_as(
            "SELECT source_slug, target_slug FROM links \
             WHERE target_slug NOT IN (SELECT slug FROM pages)"
        )
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        for (src, tgt) in broken {
            warnings.push(("WARN".into(), src, format!("broken link → {}", tgt)));
        }

        Ok(warnings)
    }

    /// Export all pages to a directory as .md files (json=true → .json files).
    pub async fn export_pages(&self, dir: &std::path::Path, json: bool) -> Result<usize> {
        std::fs::create_dir_all(dir)?;
        let pages = self.list_pages(None, None).await?;
        let count = pages.len();
        for page in &pages {
            if json {
                let links = self.backlinks(&page.slug).await?;
                let data = serde_json::json!({
                    "slug": page.slug,
                    "type": page.page_type,
                    "title": page.title,
                    "tags": page.tags,
                    "language": page.language.as_ref().map(|l| l.to_string()),
                    "compiled_truth": page.compiled_truth,
                    "timeline": page.timeline,
                    "backlinks": links.iter().map(|l| serde_json::json!({
                        "from": l.target_slug,
                        "type": l.edge_type,
                        "context": l.context,
                    })).collect::<Vec<_>>(),
                });
                let path = dir.join(format!("{}.json", page.slug.replace('/', "__")));
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, serde_json::to_string_pretty(&data)?)?;
            } else {
                let path = dir.join(format!("{}.md", page.slug.replace('/', "__")));
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut md = format!("# {}\n\n{}", page.title, page.compiled_truth);
                if !page.timeline.trim().is_empty() {
                    md.push_str(&format!("\n\n---\n\n{}", page.timeline));
                }
                std::fs::write(path, md)?;
            }
        }
        Ok(count)
    }

    pub fn get_db(&self) -> &SqlitePool {
        &self.inner.db
    }

    pub fn get_config(&self) -> &Config {
        &self.inner.config
    }

    pub async fn run_dream_cycle(&self, stage: Option<&str>) -> Result<()> {
        println!("=== Starting Dream Cycle ===");
        let run_lint = stage.is_none() || stage == Some("lint");
        let run_embed = stage.is_none() || stage == Some("embed");
        let run_extract = stage.is_none() || stage == Some("extract");
        let run_synthesize = stage.is_none() || stage == Some("synthesize");
        
        if run_lint {
            self.dream_lint().await?;
        }
        if run_embed {
            self.dream_embed().await?;
        }
        if run_extract {
            self.dream_extract().await?;
        }
        if run_synthesize {
            self.dream_synthesize().await?;
        }
        println!("\n=== Dream Cycle Finished ===");
        Ok(())
    }

    /// Find the DB chunk_id of the chunk in `page_slug` whose text contains `context`.
    /// Returns None if context is empty, chunks don't exist yet, or no match found.
    async fn find_chunk_id_for_context(&self, page_slug: &str, context: &str) -> Option<i64> {
        if context.trim().is_empty() {
            return None;
        }
        let needle: String = context.chars().take(30).collect();
        let rows = sqlx::query("SELECT id, text FROM chunks WHERE page_slug = ?1")
            .bind(page_slug)
            .fetch_all(&self.inner.db)
            .await
            .ok()?;
        rows.into_iter()
            .find(|r| {
                let text: String = r.get("text");
                text.contains(&needle)
            })
            .map(|r| r.get::<i64, _>("id"))
    }

    async fn dream_lint(&self) -> Result<()> {
        println!("\n[Dream Cycle] Phase 1: Linting knowledge base...");
        let warnings = self.lint().await?;
        if warnings.is_empty() {
            println!("  No issues found.");
        } else {
            for (level, slug, msg) in warnings {
                println!("  {} [{}]: {}", level, slug, msg);
            }
        }
        Ok(())
    }

    async fn dream_embed(&self) -> Result<()> {
        println!("\n[Dream Cycle] Phase 2: Embedding stale/missing chunks...");
        if !self.has_embedder() {
            println!("  No embedder configured, skipping embedding phase.");
            return Ok(());
        }
        
        let pages = self.list_stale_pages().await?;
        if pages.is_empty() {
            println!("  All pages are up-to-date.");
        } else {
            println!("  Embedding {} stale page(s)...", pages.len());
            for page in &pages {
                match self.chunk_and_embed_page(page).await {
                    Ok(_) => println!("    Embedded: {}", page.slug),
                    Err(e) => eprintln!("    WARN: failed to embed {}: {}", page.slug, e),
                }
            }
        }
        Ok(())
    }

    async fn dream_extract(&self) -> Result<()> {
        println!("\n[Dream Cycle] Phase 3: Extracting concepts, figures, and timeline events...");
        
        let query = "
            SELECT p.slug, p.page_type, p.title, p.compiled_truth, p.timeline, p.updated_at 
            FROM pages p 
            LEFT JOIN dream_metadata d ON p.slug = d.slug 
            WHERE (d.last_extracted_at IS NULL OR p.updated_at > d.last_extracted_at) 
              AND p.page_type = 'note'
        ";
        
        let rows = sqlx::query(query)
            .fetch_all(&self.inner.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            
        if rows.is_empty() {
            println!("  No pages require extraction.");
            return Ok(());
        }
        
        let deepseek = self.inner.deepseek.as_ref();
        
        for row in rows {
            let slug: String = row.get("slug");
            let page_type: String = row.get("page_type");
            let title: String = row.get("title");
            let compiled_truth: String = row.get("compiled_truth");
            let timeline: String = row.get("timeline");
            
            println!("  Extracting from page: {}", slug);
            
            let knowledge = if let Some(client) = deepseek {
                let system = "You are a knowledge extractor. Extract key concepts, scholars/figures (people), and timeline events from the provided academic text.\n\
                              Rules:\n\
                              - For figures: describe the person by their REAL-WORLD identity (institution, role, field of expertise). NEVER use vague references like '本文作者', 'the author', 'this article's author', '该文作者'. If you can identify them from the text (e.g. affiliation in the abstract), state it; otherwise write their field only (e.g. '教育学研究者，专注高等教育学自主知识体系').\n\
                              - For concepts: describe based on how the text defines or uses it; include the source article slug for attribution.\n\
                              - For events: use ISO date (YYYY-MM-DD or YYYY-MM or YYYY); omit if date is unknown.\n\
                              - For events: if the event is about a specific scholar or person you extracted as a figure, set figure_slug to \"research/figures/<slugified-name>\" (lowercase, spaces→hyphens, keep CJK as-is). If not tied to a person, leave figure_slug as empty string.\n\
                              - Only extract entities with substantive presence in the text (not passing mentions).\n\
                              Your response must be a raw JSON object (no markdown fences) conforming exactly to:\n\
                              {\n\
                                \"concepts\": [\n\
                                  { \"name\": \"Concept Name\", \"description\": \"Definition or role as used in source: <slug>\", \"context\": \"Relevant text snippet\" }\n\
                                ],\n\
                                \"figures\": [\n\
                                  { \"name\": \"Full Name\", \"description\": \"Institution/role/field — do NOT say 本文作者\", \"context\": \"Relevant text snippet\" }\n\
                                ],\n\
                                \"events\": [\n\
                                  { \"date\": \"YYYY-MM-DD\", \"description\": \"Event description\", \"context\": \"Relevant text snippet\", \"figure_slug\": \"research/figures/姓名 or empty\" }\n\
                                ]\n\
                              }";

                let display_title = if title.trim().is_empty() { slug.as_str() } else { title.as_str() };
                let user = format!("Source slug: {}\nTitle: {}\nType: {}\n\nContent:\n{}", slug, display_title, page_type, compiled_truth);
                match client.chat(system, &user).await {
                    Ok(resp) => {
                        let cleaned = clean_json(&resp);
                        match serde_json::from_str::<ExtractedKnowledge>(cleaned) {
                            Ok(k) => k,
                            Err(e) => {
                                eprintln!("    WARN: failed to parse JSON from LLM: {}. Response was: {}", e, resp);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("    WARN: LLM chat call failed: {}", e);
                        continue;
                    }
                }
            } else {
                // Mock extraction for testing/offline
                let page = Page {
                    slug: slug.clone(),
                    page_type: page_type.clone(),
                    title: title.clone(),
                    tags: Vec::new(),
                    frontmatter: serde_json::Value::Object(serde_json::Map::new()),
                    compiled_truth: compiled_truth.clone(),
                    timeline: timeline.clone(),
                    language: None,
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    content_hash: String::new(),
                };
                self.mock_extract_knowledge(&page)
            };
            
            // 1. Save extracted concepts
            for concept in &knowledge.concepts {
                if concept.name.trim().is_empty() {
                    continue;
                }
                let concept_slug = format!("research/concepts/{}", slugify(&concept.name));
                let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pages WHERE slug = ?1")
                    .bind(&concept_slug)
                    .fetch_one(&self.inner.db)
                    .await
                    .unwrap_or(0) > 0;
                    
                if !exists {
                    let mut cp = Page::new(concept_slug.clone(), "concept".to_string(), concept.description.clone());
                    cp.title = concept.name.clone();
                    cp.language = Some(rbrain_core::page::Language::detect(&concept.description));
                    self.put_page(cp).await?;
                    println!("    Created concept page: {}", concept_slug);
                }
                
                // Link source -> concept (anchor to chunk if context can be located)
                let link_ctx = if concept.context.is_empty() { None } else { Some(concept.context.clone()) };
                let chunk_id = self.find_chunk_id_for_context(&slug, &concept.context).await;
                if let Err(e) = self.add_link(&slug, &concept_slug, "related", link_ctx.as_deref(), chunk_id).await {
                    eprintln!("    WARN: failed to create link from {} to {}: {}", slug, concept_slug, e);
                }
            }
            
            // 2. Save extracted figures
            for figure in &knowledge.figures {
                if figure.name.trim().is_empty() {
                    continue;
                }
                let figure_slug = format!("research/figures/{}", slugify(&figure.name));
                let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pages WHERE slug = ?1")
                    .bind(&figure_slug)
                    .fetch_one(&self.inner.db)
                    .await
                    .unwrap_or(0) > 0;
                    
                if !exists {
                    let mut fp = Page::new(figure_slug.clone(), "figure".to_string(), figure.description.clone());
                    fp.title = figure.name.clone();
                    fp.language = Some(rbrain_core::page::Language::detect(&figure.description));
                    self.put_page(fp).await?;
                    println!("    Created figure page: {}", figure_slug);
                }
                
                // Link source -> figure (anchor to chunk if context can be located)
                let link_ctx = if figure.context.is_empty() { None } else { Some(figure.context.clone()) };
                let chunk_id = self.find_chunk_id_for_context(&slug, &figure.context).await;
                if let Err(e) = self.add_link(&slug, &figure_slug, "related", link_ctx.as_deref(), chunk_id).await {
                    eprintln!("    WARN: failed to create link from {} to {}: {}", slug, figure_slug, e);
                }
            }
            
            // 3. Save extracted events — route to figure page when figure_slug is set,
            //    otherwise fall back to the source article page.
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            for event in &knowledge.events {
                if event.description.trim().is_empty() {
                    continue;
                }
                let date_str = if event.date.trim().is_empty() {
                    &today
                } else {
                    &event.date
                };
                let event_src = Some(slug.as_str());

                // Determine target page: figure page if the LLM named one and it exists,
                // otherwise the source article itself.
                let target_slug = if !event.figure_slug.trim().is_empty() {
                    let fig = event.figure_slug.trim();
                    let fig_exists = sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM pages WHERE slug = ?1"
                    )
                    .bind(fig)
                    .fetch_one(&self.inner.db)
                    .await
                    .unwrap_or(0) > 0;
                    if fig_exists { fig.to_string() } else { slug.clone() }
                } else {
                    slug.clone()
                };

                if let Err(e) = self.add_timeline_entry(&target_slug, date_str, &event.description, event_src).await {
                    eprintln!("    WARN: failed to add timeline entry to {}: {}", target_slug, e);
                } else {
                    println!("    Added timeline event to {}: {} on {}", target_slug, event.description, date_str);
                }
            }
            
            // Update dream_metadata
            sqlx::query("INSERT OR REPLACE INTO dream_metadata (slug, last_extracted_at) VALUES (?1, datetime('now'))")
                .bind(&slug)
                .execute(&self.inner.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        }
        
        Ok(())
    }

    async fn dream_synthesize(&self) -> Result<()> {
        println!("\n[Dream Cycle] Phase 4: Synthesizing concept-based literature reviews...");

        // Collect all concept pages
        let all_pages = self.list_pages(None, None).await?;
        let concept_pages: Vec<Page> = all_pages.into_iter()
            .filter(|p| p.page_type == "concept")
            .collect();

        if concept_pages.is_empty() {
            println!("  No concept pages found. Run dream --stage extract first.");
            return Ok(());
        }

        let deepseek = self.inner.deepseek.as_ref();
        let mut synthesized = 0usize;

        for concept in &concept_pages {
            // Find source notes that link to this concept (page_type = 'note' only)
            let source_slugs: Vec<String> = sqlx::query_scalar(
                "SELECT DISTINCT l.source_slug FROM links l
                 JOIN pages p ON p.slug = l.source_slug
                 WHERE l.target_slug = ?1
                 AND p.page_type = 'note'"
            )
            .bind(&concept.slug)
            .fetch_all(&self.inner.db)
            .await
            .unwrap_or_default();

            // Require at least 3 distinct source articles to synthesize
            if source_slugs.len() < 3 {
                continue;
            }

            // Fetch the actual source pages
            let mut source_pages: Vec<Page> = Vec::new();
            for s in &source_slugs {
                if let Ok(p) = self.get_page(s).await {
                    source_pages.push(p);
                }
            }

            let synthesis_slug = format!("research/synthesis/{}", concept.slug.trim_start_matches("research/concepts/"));
            let existing_synth = self.get_page(&synthesis_slug).await.ok();

            // Staleness check: re-synthesize if any source is newer than the synthesis
            let mut is_stale = true;
            if let Some(ref synth) = existing_synth {
                let synth_updated = synth.updated_at;
                let max_source_updated = source_pages.iter()
                    .map(|p| p.updated_at)
                    .max()
                    .unwrap_or(synth_updated);
                if max_source_updated <= synth_updated {
                    is_stale = false;
                }
            }

            if !is_stale {
                println!("  Synthesis for '{}' is up to date.", concept.slug);
                continue;
            }

            println!("  Synthesizing '{}' ({} source articles)...", concept.slug, source_pages.len());

            let concept_desc = if concept.compiled_truth.is_empty() {
                concept.title.clone()
            } else {
                let mut idx = 500;
                while idx > 0 && !concept.compiled_truth.is_char_boundary(idx) { idx -= 1; }
                concept.compiled_truth[..idx.min(concept.compiled_truth.len())].to_string()
            };

            let synthesized_content = if let Some(client) = deepseek {

                let system = "You are an academic research synthesizer. \
                    Given a concept and a set of source articles that reference it, \
                    generate a structured literature synthesis page. \
                    You MUST cite source pages using Wikilinks [[slug]]. \
                    Structure with Markdown: H1 title, ## sections for themes/debates/evidence, \
                    a ## Working Judgment section with your synthesis, \
                    and a ## Open Questions section. \
                    Output in the language of the source materials (Simplified Chinese for Chinese sources).";

                let mut context_items = Vec::new();
                for p in &source_pages {
                    let snippet = {
                        let mut idx = 800;
                        while idx > 0 && !p.compiled_truth.is_char_boundary(idx) { idx -= 1; }
                        p.compiled_truth[..idx.min(p.compiled_truth.len())].to_string()
                    };
                    context_items.push(format!("Source: [[{}]] ({})\n{}", p.slug, p.title, snippet));
                }

                let user = format!(
                    "Concept: {} (slug: {})\nDescription: {}\n\nSource Articles:\n\n{}",
                    concept.title, concept.slug, concept_desc,
                    context_items.join("\n\n---\n\n")
                );

                match client.chat(system, &user).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        eprintln!("    WARN: LLM call failed for {}: {}. Using mock.", concept.slug, e);
                        self.generate_mock_concept_synthesis(concept, &source_pages)
                    }
                }
            } else {
                self.generate_mock_concept_synthesis(concept, &source_pages)
            };

            let mut synth_page = Page::new(synthesis_slug.clone(), "synthesis".to_string(), synthesized_content);
            synth_page.title = format!("综合分析：{}", concept.title);
            synth_page.tags = concept.tags.clone();
            synth_page.language = Some(rbrain_core::page::Language::detect(&synth_page.compiled_truth));

            self.put_page(synth_page.clone()).await?;
            println!("    Saved: {}", synthesis_slug);

            if self.has_embedder() {
                if let Err(e) = self.chunk_and_embed_page(&synth_page).await {
                    eprintln!("    WARN: failed to embed {}: {}", synthesis_slug, e);
                }
            }

            // Link the synthesis back to the concept and its sources
            let _ = self.add_link(&synthesis_slug, &concept.slug, "develops", Some(&concept_desc), None).await;
            for p in &source_pages {
                let ctx = format!("{} ({})", p.title, p.slug);
                let _ = self.add_link(&synthesis_slug, &p.slug, "evidence", Some(&ctx), None).await;
            }

            synthesized += 1;
        }

        if synthesized == 0 {
            println!("  No concepts had 3+ source articles. Nothing synthesized.");
        } else {
            println!("  Synthesized {} concept(s).", synthesized);
        }

        Ok(())
    }
    
    fn generate_mock_concept_synthesis(&self, concept: &Page, source_pages: &[Page]) -> String {
        let mut md = format!("# 综合分析：{}\n\n", concept.title);
        md.push_str("## 概念说明\n\n");
        md.push_str(&format!("{}\n\n", concept.compiled_truth));
        md.push_str("## 核心文献\n\n");
        for p in source_pages {
            md.push_str(&format!("- [[{}]] — **{}**\n", p.slug, p.title));
        }
        md.push_str("\n## 工作判断\n\n");
        md.push_str("（待 LLM 综合分析）\n\n");
        md.push_str("## 开放问题\n\n");
        md.push_str("（待补充）\n");
        md
    }

    fn mock_extract_knowledge(&self, page: &Page) -> ExtractedKnowledge {
        let mut concepts = Vec::new();
        let mut figures = Vec::new();
        let mut events = Vec::new();

        let text = page.compiled_truth.to_lowercase();
        if text.contains("预训练") || text.contains("pretrained") {
            concepts.push(ExtractedConcept {
                name: "预训练语言模型".to_string(),
                description: "Pre-trained Language Models (PLMs) that are trained on large scale corpora.".to_string(),
                context: "近年来，预训练语言模型在自然语言处理领域取得了显著的进展。".to_string(),
            });
            figures.push(ExtractedFigure {
                name: "BERT".to_string(),
                description: "A popular bidirectional encoder representation model from Google.".to_string(),
                context: "对比了基于BERT and RoBERTa等不同架构的模型在多个数据集上的表现。".to_string(),
            });
            events.push(ExtractedEvent {
                date: "2018-10-11".to_string(),
                description: "BERT model was officially released by Google researchers.".to_string(),
                context: "本研究针对中文文本分类任务，对比了基于BERT和RoBERTa等不同架构的模型表现。".to_string(),
                figure_slug: String::new(),
            });
        } else {
            let words: Vec<&str> = page.title.split_whitespace().collect();
            let name = if !words.is_empty() { words[0] } else { "Mock Concept" };
            concepts.push(ExtractedConcept {
                name: format!("Concept {}", name),
                description: format!("A mock academic concept extracted for {}", page.title),
                context: page.compiled_truth.chars().take(50).collect(),
            });
            figures.push(ExtractedFigure {
                name: "Dr. Mock Scholar".to_string(),
                description: "A hypothetical figure associated with this work.".to_string(),
                context: page.title.clone(),
            });
            events.push(ExtractedEvent {
                date: "2026-05-24".to_string(),
                description: format!("Mock milestone event for {}", page.title),
                context: page.title.clone(),
                figure_slug: String::new(),
            });
        }

        ExtractedKnowledge { concepts, figures, events }
    }
}

/// Rewrite the `tags:` line in a markdown file's YAML frontmatter.
fn update_frontmatter_tags(content: &str, tags: &[String]) -> String {
    let tags_yaml = if tags.is_empty() {
        "tags: []".to_string()
    } else {
        let items = tags.iter().map(|t| format!("  - {}", t)).collect::<Vec<_>>().join("\n");
        format!("tags:\n{}", items)
    };

    // Replace existing tags line(s)
    let re = regex::Regex::new(r"(?m)^tags:.*(\n  - .*)*").unwrap();
    if re.is_match(content) {
        re.replace(content, tags_yaml.as_str()).to_string()
    } else {
        content.to_string()
    }
}

fn compute_query_hash(query: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(query.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub struct GraphEdge {
    pub target: String,
    pub edge_type: String,
    pub depth: usize,
    pub context: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunk_id: i64,
    pub score: f64,
    pub text: String,
    pub page_slug: String,
}

#[derive(Debug, Clone)]
pub struct BrainStats {
    pub pages_by_type: HashMap<String, i64>,
    pub pages_by_language: HashMap<String, i64>,
    pub total_chunks: i64,
    pub embedding_coverage: f64,
    pub graph_density: f64,
    pub recent_activity: i64,
}

impl BrainStats {
    pub fn total_pages(&self) -> i64 {
        self.pages_by_type.values().sum()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedConcept {
    name: String,
    description: String,
    context: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedFigure {
    name: String,
    description: String,
    context: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedEvent {
    date: String,
    description: String,
    context: String,
    /// Slug of the figure page this event belongs to (e.g. "research/figures/张三").
    /// Empty string if the event is not attributed to a specific figure.
    #[serde(default)]
    figure_slug: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExtractedKnowledge {
    concepts: Vec<ExtractedConcept>,
    figures: Vec<ExtractedFigure>,
    events: Vec<ExtractedEvent>,
}

fn clean_json(s: &str) -> &str {
    let mut s = s.trim();
    if s.starts_with("```") {
        if let Some(end) = s.rfind("```").filter(|&e| e > 0) {
            s = &s[3..end];
            if s.starts_with("json") {
                s = &s[4..];
            }
        }
    }
    s.trim()
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

