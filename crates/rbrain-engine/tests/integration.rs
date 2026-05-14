mod common;

use common::TestBrain;
use rbrain_core::keyword_index::KeywordIndex;
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
