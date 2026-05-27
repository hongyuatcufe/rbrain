mod common;

use common::TestBrain;
use rbrain_core::keyword_index::KeywordIndex;
use rbrain_core::markdown::MarkdownParser;
use rbrain_core::page::{Language, Page};
use rbrain_engine::Engine;
use rbrain_llm::mock::MockEmbedder;
use rbrain_search::TantivyIndex;
use rbrain_search::vector_store::UsearchStore;
use std::sync::Arc;

/// Open a full-stack engine backed by MockEmbedder. No API key needed.
async fn open_mock_engine(tb: &TestBrain) -> Engine {
    let embedder = Arc::new(MockEmbedder::new(tb.config.embedding_dim));
    let vector_store = Arc::new(
        UsearchStore::new(tb.config.vectors_path.clone(), tb.config.embedding_dim)
            .expect("UsearchStore::new"),
    );
    let keyword_index = Arc::new(
        TantivyIndex::new(tb.config.tantivy_dir.clone()).expect("TantivyIndex::new"),
    );
    Engine::open_with_search(
        tb.config.clone(),
        embedder,
        vector_store,
        keyword_index,
    )
    .await
    .expect("Engine::open_with_search")
}

#[tokio::test]
async fn test_put_and_get() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page::new("test-page".to_string(), "note".to_string(), "Hello world".to_string());
    engine.put_page(page).await.expect("put_page");

    let fetched = engine.get_page("test-page").await.expect("get_page");
    assert_eq!(fetched.slug, "test-page");
    assert_eq!(fetched.page_type, "note");
}

#[tokio::test]
async fn test_embed_and_keyword_search_en() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        language: Some(Language::En),
        ..Page::new(
            "tang-dynasty".to_string(),
            "book".to_string(),
            "The Tang dynasty was an imperial dynasty of China that ruled from 618 to 907.".to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine.chunk_and_embed_page(&page).await.expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("dynasty", &Language::En, 5)
        .await
        .expect("keyword_search");
    assert!(!results.is_empty(), "English keyword search should return results");
}

#[tokio::test]
async fn test_cjk_keyword_search_zh() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        language: Some(Language::ZhHans),
        ..Page::new(
            "china-history".to_string(),
            "book".to_string(),
            "中国历史上，唐朝是一个重要的朝代，文化繁荣，经济发达。".to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine.chunk_and_embed_page(&page).await.expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("唐朝", &Language::ZhHans, 5)
        .await
        .expect("keyword_search zh");
    assert!(!results.is_empty(), "Chinese keyword search for 唐朝 should return results with lindera CC-CEDICT");
}

#[tokio::test]
async fn test_cjk_keyword_search_ja() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        language: Some(Language::Ja),
        ..Page::new(
            "japan-history".to_string(),
            "book".to_string(),
            "日本の歴史において、江戸時代は重要な時代です。文化が発展し、経済も成長しました。".to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine.chunk_and_embed_page(&page).await.expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("江戸", &Language::Ja, 5)
        .await
        .expect("keyword_search ja");
    assert!(!results.is_empty(), "Japanese keyword search for 江戸 should return results with lindera IPADIC");
}

#[tokio::test]
async fn test_hybrid_search() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        language: Some(Language::En),
        ..Page::new(
            "rust-lang".to_string(),
            "note".to_string(),
            "Rust is a systems programming language focused on safety and performance.".to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine.chunk_and_embed_page(&page).await.expect("chunk_and_embed_page");

    let results = engine
        .hybrid_search("systems programming", &Language::En, 5)
        .await
        .expect("hybrid_search");

    assert!(!results.is_empty(), "hybrid_search should return results");
}

#[tokio::test]
async fn test_tantivy_no_lock_conflict_on_concurrent_open() {
    let tb = TestBrain::new().await;

    // Open two separate TantivyIndex instances on the same directory.
    // With lazy writer, neither holds a file lock at construction time.
    let idx1 = TantivyIndex::new(tb.config.tantivy_dir.clone())
        .expect("first TantivyIndex::new should succeed");
    let idx2 = TantivyIndex::new(tb.config.tantivy_dir.clone())
        .expect("second TantivyIndex::new should succeed without lock conflict");

    // Both can search without conflict
    let r1 = idx1.search("test", &Language::En, 5).await.expect("idx1 search");
    let r2 = idx2.search("test", &Language::En, 5).await.expect("idx2 search");

    assert_eq!(r1.len(), r2.len());
}

#[tokio::test]
async fn test_delete_page_cleans_up() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page::new(
        "to-delete".to_string(),
        "note".to_string(),
        "This page will be deleted.".to_string(),
    );
    engine.put_page(page.clone()).await.expect("put_page");
    engine.chunk_and_embed_page(&page).await.expect("chunk_and_embed_page");
    engine.delete_page("to-delete").await.expect("delete_page");

    let result = engine.get_page("to-delete").await;
    assert!(result.is_err(), "page should be gone after delete");
}

#[tokio::test]
async fn test_graph_links() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    // Page A links to page B via [[page-b]]
    let page_a = Page::new(
        "page-a".to_string(),
        "note".to_string(),
        "See also [[page-b]] for more details.".to_string(),
    );
    let page_b = Page::new(
        "page-b".to_string(),
        "note".to_string(),
        "Page B content.".to_string(),
    );

    engine.put_page(page_a).await.expect("put page_a");
    engine.put_page(page_b).await.expect("put page_b");

    let backlinks = engine.backlinks("page-b").await.expect("backlinks");
    assert!(
        backlinks.iter().any(|l| l.target_slug == "page-a"),
        "page-b should have a backlink from page-a"
    );
}

#[tokio::test]
async fn test_rejects_slug_path_traversal() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page::new("../escaped".to_string(), "note".to_string(), "bad".to_string());
    assert!(engine.put_page(page).await.is_err());
    assert!(!tb.config.repo_dir.join("../escaped.md").exists());
}

#[tokio::test]
async fn test_explicit_link_survives_page_update() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    engine.put_page(Page::new("source".into(), "note".into(), "body".into())).await.expect("put source");
    engine.put_page(Page::new("target".into(), "note".into(), "target".into())).await.expect("put target");
    engine.add_link("source", "target", "evidence", Some("manual"), None).await.expect("add link");

    let mut source = engine.get_page("source").await.expect("get source");
    source.compiled_truth = "updated body".to_string();
    engine.put_page(source).await.expect("update source");

    let outlinks = engine.outlinks("source").await.expect("outlinks");
    assert!(outlinks.iter().any(|link| link.target_slug == "target" && link.edge_type == "evidence"));
}

#[tokio::test]
async fn test_timeline_preserves_written_frontmatter() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    engine.put_page(Page::new("concept-page".into(), "concept".into(), "body".into())).await.expect("put");
    engine.add_tag("concept-page", "kept-tag").await.expect("tag");
    engine.add_timeline_entry("concept-page", "2026-05-27", "event", None).await.expect("timeline");

    let content = std::fs::read_to_string(tb.config.repo_dir.join("concept-page.md")).expect("read page");
    let parsed = MarkdownParser::parse(&content);
    assert_eq!(parsed.frontmatter.get("type").and_then(|value| value.as_str()), Some("concept"));
    assert_eq!(
        parsed.frontmatter.get("tags").and_then(|value| value.as_array()).and_then(|tags| tags[0].as_str()),
        Some("kept-tag")
    );
}

#[tokio::test]
async fn test_update_removes_previous_keyword_chunks() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let mut page = Page {
        language: Some(Language::En),
        ..Page::new("replace-page".into(), "note".into(), "obsoletekeyword only".into())
    };
    engine.put_page(page.clone()).await.expect("put original");
    engine.chunk_and_embed_page(&page).await.expect("embed original");

    page.compiled_truth = "replacementkeyword only".to_string();
    engine.put_page(page.clone()).await.expect("put replacement");
    engine.chunk_and_embed_page(&page).await.expect("embed replacement");

    let obsolete = engine.keyword_search("obsoletekeyword", &Language::En, 5).await.expect("search obsolete");
    assert!(obsolete.is_empty(), "updated pages must not retain prior keyword chunks");
}

#[tokio::test]
async fn test_sync_invalidates_searchable_chunks_and_preserves_timeline() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let mut page = Page {
        language: Some(Language::En),
        ..Page::new("synced-page".into(), "note".into(), "obsoletekeyword only".into())
    };
    page.timeline = "- 2026-05-26: original event".to_string();
    engine.put_page(page.clone()).await.expect("put original");
    engine.chunk_and_embed_page(&page).await.expect("embed original");

    let external = MarkdownParser::to_canonical(
        &page.frontmatter,
        "replacementkeyword only",
        "- 2026-05-27: edited event",
    );
    std::fs::write(tb.config.repo_dir.join("synced-page.md"), external).expect("external edit");

    let (_, updated, _) = engine.sync().await.expect("sync");
    assert_eq!(updated, vec!["synced-page".to_string()]);

    let synced = engine.get_page("synced-page").await.expect("get synced");
    assert_eq!(synced.compiled_truth, "replacementkeyword only");
    assert_eq!(synced.timeline, "- 2026-05-27: edited event");

    let stale = engine.list_stale_pages().await.expect("list stale");
    assert!(stale.iter().any(|candidate| candidate.slug == "synced-page"));
    let obsolete = engine
        .search_with_context("obsoletekeyword", &Language::En, 5, false)
        .await
        .expect("search obsolete");
    assert!(obsolete.is_empty(), "sync must not return pre-edit source text");
    let obsolete_keyword = engine
        .keyword_search("obsoletekeyword", &Language::En, 5)
        .await
        .expect("keyword search obsolete");
    assert!(obsolete_keyword.is_empty(), "keyword API must not expose invalidated chunks");
}

#[tokio::test]
async fn test_vector_search_keeps_nearest_result_first() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let exact = Page { language: Some(Language::En), ..Page::new("exact".into(), "note".into(), "exact query".into()) };
    let other = Page { language: Some(Language::En), ..Page::new("other".into(), "note".into(), "unrelated material".into()) };
    engine.put_page(exact.clone()).await.expect("put exact");
    engine.put_page(other.clone()).await.expect("put other");
    engine.chunk_and_embed_page(&exact).await.expect("embed exact");
    engine.chunk_and_embed_page(&other).await.expect("embed other");

    let results = engine.vector_search("exact query", 2).await.expect("vector search");
    let (_, first_slug) = engine.fetch_chunk_by_id(results[0].0).await.expect("fetch").expect("first chunk");
    assert_eq!(first_slug, "exact");
}

#[tokio::test]
async fn test_indegree_stats_are_updated_by_links() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    engine.put_page(Page::new("source".into(), "note".into(), "source".into())).await.expect("put source");
    engine.put_page(Page::new("target".into(), "note".into(), "target".into())).await.expect("put target");
    engine.add_link("source", "target", "related", None, None).await.expect("link");

    let indegree: i64 = sqlx::query_scalar("SELECT indegree FROM page_stats WHERE slug = 'target'")
        .fetch_one(engine.get_db())
        .await
        .expect("indegree");
    assert_eq!(indegree, 1);
}

#[tokio::test]
async fn test_dream_cycle_flow() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    // Extraction processes source notes and synthesis requires three sources.
    let page1 = Page {
        tags: vec!["nlp".to_string()],
        ..Page::new(
            "paper-1".to_string(),
            "note".to_string(),
            "摘要：预训练语言模型是现代自然语言处理的基础。我们将研究BERT的性能表现。".to_string(),
        )
    };

    // Create page 2 with same tag
    let page2 = Page {
        tags: vec!["nlp".to_string()],
        ..Page::new(
            "paper-2".to_string(),
            "note".to_string(),
            "摘要：基于预训练语言模型，我们实现了多种下游NLP任务的性能突破。".to_string(),
        )
    };
    let page3 = Page {
        tags: vec!["nlp".to_string()],
        ..Page::new(
            "paper-3".to_string(),
            "note".to_string(),
            "摘要：预训练语言模型与BERT为语言理解研究提供了新基础。".to_string(),
        )
    };

    engine.put_page(page1).await.expect("put page1");
    engine.put_page(page2).await.expect("put page2");
    engine.put_page(page3).await.expect("put page3");

    // Run dream cycle (all stages)
    engine.run_dream_cycle(None).await.expect("run_dream_cycle");

    let concepts = engine.list_pages(Some("concept"), None).await.expect("list concepts");
    assert_eq!(concepts.len(), 1, "one extracted concept should be created");
    let concept_page = &concepts[0];
    assert!(concept_page.slug.starts_with("research/concepts/"));

    let figures = engine.list_pages(Some("figure"), None).await.expect("list figures");
    assert_eq!(figures.len(), 1, "one extracted figure should be created");
    assert!(figures[0].slug.starts_with("research/figures/"));

    // Timeline events belong to extracted figures; source notes remain immutable.
    let fetched_p1 = engine.get_page("paper-1").await.expect("get page1");
    assert!(fetched_p1.timeline.is_empty(), "dream extraction must not alter source notes");
    let bert = engine.get_page("research/figures/bert").await.expect("get BERT figure");
    assert!(
        bert.timeline.contains("BERT model was officially released"),
        "Timeline event not found on figure: {}",
        bert.timeline
    );

    let synthesis_slug = concept_page
        .slug
        .replacen("research/concepts/", "research/synthesis/", 1);
    let synth_page = engine.get_page(&synthesis_slug).await.expect("get synthesis page");
    assert!(synth_page.compiled_truth.contains("paper-1"), "Synthesis content should refer to paper-1");
    assert!(synth_page.compiled_truth.contains("paper-2"), "Synthesis content should refer to paper-2");
}

#[tokio::test]
async fn test_dream_unassigned_events_are_saved_as_evidence_without_mutating_raw() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;
    let source_slug = "raw/articles/source";

    engine
        .put_page(Page::new(
            source_slug.to_string(),
            "note".to_string(),
            "A study of school practice with a dated milestone.".to_string(),
        ))
        .await
        .expect("put raw source");
    let source_path = tb.config.repo_dir.join("raw/articles/source.md");
    let before = std::fs::read_to_string(&source_path).expect("read source before dream");

    engine.run_dream_cycle(Some("extract")).await.expect("extract dream");

    let source = engine.get_page(source_slug).await.expect("get raw source");
    let after = std::fs::read_to_string(&source_path).expect("read source after dream");
    assert!(source.timeline.is_empty(), "unassigned events must not be added to raw pages");
    assert_eq!(after, before, "dream extraction must not rewrite raw source files");

    let evidence_slug = "research/evidence/events/raw/articles/source";
    let evidence = engine.get_page(evidence_slug).await.expect("get derived evidence page");
    assert!(evidence.timeline.contains("Mock milestone event"));
    let outlinks = engine.outlinks(evidence_slug).await.expect("event evidence outlinks");
    assert!(outlinks.iter().any(|link| {
        link.target_slug == source_slug && link.edge_type == "evidence"
    }));
}
