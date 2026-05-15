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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use strsim::levenshtein;
use walkdir::WalkDir;
use whatlang::{detect, Lang};

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
        let repo_path = self
            .inner
            .config
            .repo_dir
            .join(format!("{}.md", page.slug));

        if repo_path.exists() {
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

        let canonical = MarkdownParser::to_canonical(
            &page.frontmatter,
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
                 (source_slug, target_slug, edge_type, context, created_at) \
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))",
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

        if let Some(vector_store) = &self.inner.vector_store {
            for chunk_id in &chunk_ids {
                vector_store.delete(*chunk_id).await?;
            }
            vector_store.save().await?;
        }

        if let Some(keyword_index) = &self.inner.keyword_index {
            for chunk_id in &chunk_ids {
                keyword_index.delete(*chunk_id).await?;
            }
            keyword_index.commit().await?;
        }

        sqlx::query("DELETE FROM chunks WHERE page_slug = ?1")
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

        Ok(())
    }

    pub async fn list_pages(
        &self,
        page_type: Option<&str>,
        tag: Option<&str>,
    ) -> Result<Vec<Page>> {
        let mut query = String::from(
            "SELECT slug, page_type, title, tags, frontmatter, compiled_truth, timeline, language, content_hash, created_at, updated_at FROM pages",
        );
        let mut conditions = Vec::new();

        if let Some(t) = page_type {
            conditions.push(format!("page_type = '{}'", t.replace('\'', "''")));
        }
        if let Some(t) = tag {
            conditions.push(format!("tags LIKE '%{}%'", t.replace('\'', "''")));
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(" ORDER BY updated_at DESC");

        let rows = sqlx::query(&query)
            .fetch_all(&self.inner.db)
            .await
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

    fn detect_language(text: &str) -> Option<rbrain_core::page::Language> {
        if let Some(info) = detect(text) {
            match info.lang() {
                Lang::Jpn => Some(rbrain_core::page::Language::Ja),
                Lang::Kor => Some(rbrain_core::page::Language::Ko),
                Lang::Eng => Some(rbrain_core::page::Language::En),
                Lang::Cmn => {
                    let has_traditional = text.chars().any(Self::is_traditional_chinese);
                    if has_traditional {
                        Some(rbrain_core::page::Language::ZhHant)
                    } else {
                        Some(rbrain_core::page::Language::ZhHans)
                    }
                }
                _ => Some(rbrain_core::page::Language::Other(info.lang().code().to_string())),
            }
        } else {
            None
        }
    }

    fn is_traditional_chinese(c: char) -> bool {
        matches!(c,
            '國' | '學' | '會' | '體' | '電' | '動' | '門' | '間' | '開' | '關'
            | '時' | '從' | '東' | '業' | '長' | '風' | '發' | '雲' | '馬' | '魚'
        )
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
                    let parse_result = MarkdownParser::parse(&content);
                    let page = Page::new(
                        MarkdownParser::normalize_slug(&slug),
                        parse_result.frontmatter.get("type").and_then(|v| v.as_str()).unwrap_or("note").to_string(),
                        parse_result.compiled_truth.clone(),
                    );
                    self.put_page(page).await?;
                    updated.push(slug.clone());
                    db_slug_set.remove(&slug);
                }
                None => {
                    let parse_result = MarkdownParser::parse(&content);
                    let page = Page::new(
                        MarkdownParser::normalize_slug(&slug),
                        parse_result.frontmatter.get("type").and_then(|v| v.as_str()).unwrap_or("note").to_string(),
                        parse_result.compiled_truth.clone(),
                    );
                    self.put_page(page).await?;
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

    pub async fn import_dir(&self, dir: &str) -> Result<Vec<String>> {
        let mut imported = Vec::new();

        for entry in WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        {
            let path = entry.path();
            let content = std::fs::read_to_string(path)?;
            let hash = MarkdownParser::content_hash(&content);

            let relative = path.strip_prefix(dir)
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

            let full_text = format!("{} {}", page.compiled_truth, page.timeline);
            page.language = Self::detect_language(&full_text);

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
        let keyword_index = self
            .inner
            .keyword_index
            .as_ref()
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "search".to_string(),
                message: "Keyword index not configured".to_string(),
            })?;

        keyword_index.search(query, lang, k).await
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
            "SELECT source_slug, edge_type, context FROM links WHERE target_slug = ?1",
        )
        .bind(&normalized)
        .fetch_all(&self.inner.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let mut links = Vec::new();
        for row in rows {
            links.push(LinkRef {
                target_slug: row.get("source_slug"),
                edge_type: row.get("edge_type"),
                context: row.get("context"),
            });
        }

        Ok(links)
    }

    /// Add an explicit typed link between two pages (does not require [[wikilink]] syntax).
    /// Edge type defaults to "related" if not specified.
    /// Valid types: evidence, related, person, period, supports, contrasts, develops, mentions, references
    pub async fn add_link(
        &self,
        source_slug: &str,
        target_slug: &str,
        edge_type: &str,
        context: Option<&str>,
    ) -> Result<()> {
        let source = MarkdownParser::normalize_slug(source_slug);
        let target = MarkdownParser::normalize_slug(target_slug);
        sqlx::query(
            "INSERT OR REPLACE INTO links (source_slug, target_slug, edge_type, context, created_at) \
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        )
        .bind(&source)
        .bind(&target)
        .bind(edge_type)
        .bind(context)
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
            });
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
            .map(|c| format!("[来源: {}]\n{}", c.page_slug, c.text))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system = "你是知识库编辑。根据提供的原文材料，生成一篇简洁的Markdown wiki页面。\
            要求：包含一级标题、2-4个核心观点（用##小节）、简短结语。\
            严格基于原文，不添加原文没有的内容。输出简体中文。";
        let user = format!(
            "主题：【{topic}】\n\n原文材料：\n\n{context}\n\n请生成wiki页面。"
        );

        deepseek.chat(system, &user).await
    }

    pub fn get_db(&self) -> &SqlitePool {
        &self.inner.db
    }

    pub fn get_config(&self) -> &Config {
        &self.inner.config
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
