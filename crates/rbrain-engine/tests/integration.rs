mod common;

use async_trait::async_trait;
use common::TestBrain;
use rbrain_core::embedder::Embedder;
use rbrain_core::error::Result as BrainResult;
use rbrain_core::keyword_index::KeywordIndex;
use rbrain_core::page::{Language, Page};
use rbrain_core::schema::{TimelineEntry, TimelineSource};
use rbrain_engine::Engine;
use rbrain_llm::mock::MockEmbedder;
use rbrain_search::TantivyIndex;
use rbrain_search::vector_store::UsearchStore;
use std::sync::{Arc, Mutex};

/// Open a full-stack engine backed by MockEmbedder. No API key needed.
async fn open_mock_engine(tb: &TestBrain) -> Engine {
    let embedder = Arc::new(MockEmbedder::new(tb.config.embedding_dim));
    let vector_store = Arc::new(
        UsearchStore::new(tb.config.vectors_path.clone(), tb.config.embedding_dim)
            .expect("UsearchStore::new"),
    );
    let keyword_index =
        Arc::new(TantivyIndex::new(tb.config.tantivy_dir.clone()).expect("TantivyIndex::new"));
    Engine::open_with_search(tb.config.clone(), embedder, vector_store, keyword_index)
        .await
        .expect("Engine::open_with_search")
}

#[derive(Debug)]
struct RecordingEmbedder {
    dim: usize,
    inputs: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Embedder for RecordingEmbedder {
    fn dimension(&self) -> usize {
        self.dim
    }

    async fn embed_one(&self, text: &str) -> BrainResult<Vec<f32>> {
        Ok(self.embed_text(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> BrainResult<Vec<Vec<f32>>> {
        self.inputs.lock().unwrap().extend(texts.iter().cloned());
        Ok(texts.iter().map(|text| self.embed_text(text)).collect())
    }

    fn verify_deterministic(&self) -> bool {
        true
    }
}

impl RecordingEmbedder {
    fn embed_text(&self, text: &str) -> Vec<f32> {
        let seed = text.bytes().fold(0u8, |acc, b| acc.wrapping_add(b)) as f32;
        vec![seed; self.dim]
    }
}

#[tokio::test]
async fn test_put_and_get() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page::new(
        "test-page".to_string(),
        "note".to_string(),
        "Hello world".to_string(),
    );
    engine.put_page(page).await.expect("put_page");

    let fetched = engine.get_page("test-page").await.expect("get_page");
    assert_eq!(fetched.slug, "test-page");
    assert_eq!(fetched.page_type, "note");
}

#[tokio::test]
async fn test_embedding_input_is_title_prefixed_without_changing_stored_chunk() {
    let tb = TestBrain::new().await;
    let recorded_inputs = Arc::new(Mutex::new(Vec::new()));
    let embedder = Arc::new(RecordingEmbedder {
        dim: tb.config.embedding_dim,
        inputs: Arc::clone(&recorded_inputs),
    });
    let vector_store = Arc::new(
        UsearchStore::new(tb.config.vectors_path.clone(), tb.config.embedding_dim)
            .expect("UsearchStore::new"),
    );
    let keyword_index =
        Arc::new(TantivyIndex::new(tb.config.tantivy_dir.clone()).expect("TantivyIndex::new"));
    let engine = Engine::open_with_search(tb.config.clone(), embedder, vector_store, keyword_index)
        .await
        .expect("Engine::open_with_search");

    let page = Page {
        title: "Contextual Title".to_string(),
        language: Some(Language::En),
        ..Page::new(
            "title-prefix".to_string(),
            "note".to_string(),
            "Pronoun-heavy passage depends on its source.".to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

    let inputs = recorded_inputs.lock().unwrap();
    assert_eq!(inputs.len(), 1);
    assert!(
        inputs[0].starts_with("[Contextual Title]\n\n"),
        "embedding input should include page title prefix: {:?}",
        inputs[0]
    );

    let chunk = engine
        .fetch_chunks_text(&[1])
        .await
        .expect("fetch_chunks_text")
        .pop()
        .expect("stored chunk");
    assert_eq!(chunk.1, "Pronoun-heavy passage depends on its source.");
}

#[tokio::test]
async fn test_structured_frontmatter_validation_rejects_missing_fields() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        frontmatter: serde_json::json!({
            "type": "citation_record",
            "title": "Incomplete citation"
        }),
        ..Page::new(
            "citations/incomplete".to_string(),
            "citation_record".to_string(),
            "Incomplete citation metadata.".to_string(),
        )
    };

    let err = engine.put_page(page).await.unwrap_err().to_string();
    assert!(err.contains("missing required field"), "{err}");
}

#[tokio::test]
async fn test_structured_frontmatter_validation_accepts_citation_record() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page {
        title: "Complete citation".to_string(),
        frontmatter: serde_json::json!({
            "type": "citation_record",
            "title": "Complete citation",
            "authors": ["A. Scholar"],
            "year": 2026,
            "journal": "Journal of Tests",
            "abstract": "A test abstract.",
            "source": "csv",
            "record_hash": "sha256:test"
        }),
        ..Page::new(
            "citations/complete".to_string(),
            "citation_record".to_string(),
            "Complete citation metadata.".to_string(),
        )
    };

    engine.put_page(page).await.expect("put citation_record");
    let fetched = engine
        .get_page("citations/complete")
        .await
        .expect("get citation_record");
    assert_eq!(fetched.page_type, "citation_record");
    assert_eq!(
        fetched.schema_version,
        rbrain_core::schema::PageSchema::CURRENT_VERSION
    );
}

#[tokio::test]
async fn test_put_page_rejects_path_traversal_slug() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let err = engine
        .put_page(Page::new(
            "../escaped".to_string(),
            "note".to_string(),
            "This must not be written outside repo_dir.".to_string(),
        ))
        .await
        .unwrap_err()
        .to_string();

    assert!(err.contains("invalid slug"), "{err}");
    let escaped = tb
        .config
        .repo_dir
        .parent()
        .expect("repo_dir parent")
        .join("escaped.md");
    assert!(
        !escaped.exists(),
        "path traversal slug must not create {}",
        escaped.display()
    );
}

#[tokio::test]
async fn test_sync_imports_without_rewriting_raw_markdown() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let raw = "---\ntype: note\ntitle: Raw Note\ntags:\n  - raw\n---\nBody from raw file.\n";
    let raw_path = tb.config.repo_dir.join("raw-note.md");
    std::fs::write(&raw_path, raw).expect("write raw markdown");

    let (imported, updated, orphaned) = engine.sync().await.expect("sync");
    assert_eq!(imported, vec!["raw-note".to_string()]);
    assert!(updated.is_empty());
    assert!(orphaned.is_empty());

    let after = std::fs::read_to_string(&raw_path).expect("read raw markdown");
    assert_eq!(
        after, raw,
        "sync must not canonicalize or rewrite raw files"
    );

    let page = engine.get_page("raw-note").await.expect("get synced page");
    assert_eq!(page.title, "Raw Note");
    assert_eq!(page.tags, vec!["raw".to_string()]);
    assert_eq!(page.compiled_truth, "Body from raw file.");
}

#[tokio::test]
async fn test_timeline_entries_are_structured_but_render_compatibly() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page = Page::new(
        "timeline-page".to_string(),
        "note".to_string(),
        "Timeline test content.".to_string(),
    );
    engine.put_page(page).await.expect("put_page");

    engine
        .add_timeline_entry(
            "timeline-page",
            "2026-06-14",
            "Read source material",
            Some("source/a"),
        )
        .await
        .expect("add_timeline_entry");
    engine
        .add_take("timeline-page", "This needs follow-up.", "question")
        .await
        .expect("add_take");

    let fetched = engine.get_page("timeline-page").await.expect("get_page");
    let entries = TimelineEntry::parse_compat(&fetched.timeline).expect("parse timeline");
    assert_eq!(entries.len(), 2);

    let rendered = TimelineEntry::render_compat(&fetched.timeline);
    assert!(rendered.contains("Read source material"));
    assert!(rendered.contains("[take/question]"));

    let takes = TimelineEntry::take_lines_compat(&fetched.timeline);
    assert_eq!(takes.len(), 1);
    assert!(takes[0].contains("This needs follow-up."));
}

#[tokio::test]
async fn test_timeline_is_not_indexed_for_search() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let mut page = Page {
        language: Some(Language::En),
        ..Page::new(
            "timeline-not-indexed".to_string(),
            "note".to_string(),
            "Compiled truth contains stable searchable evidence.".to_string(),
        )
    };
    page.timeline = "auditor-only-token should never be searchable".to_string();

    engine.put_page(page.clone()).await.expect("put_page");
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

    let truth_results = engine
        .keyword_search("stable searchable evidence", &Language::En, 5)
        .await
        .expect("keyword_search truth");
    assert!(
        !truth_results.is_empty(),
        "compiled truth should be indexed"
    );

    let timeline_results = engine
        .keyword_search("auditor-only-token", &Language::En, 5)
        .await
        .expect("keyword_search timeline");
    assert!(
        timeline_results.is_empty(),
        "timeline entries must not be indexed for retrieval"
    );
}

#[tokio::test]
async fn test_timeline_links_do_not_create_graph_edges() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    engine
        .put_page(Page::new(
            "target-page".to_string(),
            "note".to_string(),
            "Target page.".to_string(),
        ))
        .await
        .expect("put target");

    let mut source = Page::new(
        "timeline-link-source".to_string(),
        "note".to_string(),
        "Compiled truth has no wiki links.".to_string(),
    );
    source.timeline = "Audit note mentions [[target-page]] only.".to_string();

    engine.put_page(source).await.expect("put source");

    let backlinks = engine.backlinks("target-page").await.expect("backlinks");
    assert!(
        backlinks.is_empty(),
        "timeline links must not create provenance graph edges"
    );
}

#[tokio::test]
async fn test_tag_filter_matches_exact_json_tag() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let page_ai = Page {
        tags: vec!["ai".to_string()],
        language: Some(Language::En),
        ..Page::new(
            "tag-ai".to_string(),
            "note".to_string(),
            "Shared retrieval phrase for exact tag matching.".to_string(),
        )
    };
    let page_fairness = Page {
        tags: vec!["fairness-ai".to_string()],
        language: Some(Language::En),
        ..Page::new(
            "tag-fairness-ai".to_string(),
            "note".to_string(),
            "Shared retrieval phrase for exact tag matching.".to_string(),
        )
    };

    engine.put_page(page_ai.clone()).await.expect("put ai");
    engine
        .put_page(page_fairness.clone())
        .await
        .expect("put fairness-ai");
    engine
        .chunk_and_embed_page(&page_ai)
        .await
        .expect("embed ai");
    engine
        .chunk_and_embed_page(&page_fairness)
        .await
        .expect("embed fairness-ai");

    let chunks = engine
        .search_with_context_filtered(
            "Shared retrieval phrase",
            &Language::En,
            5,
            false,
            None,
            Some("ai"),
        )
        .await
        .expect("filtered search");

    assert!(!chunks.is_empty(), "exact tag search should return tag-ai");
    assert!(
        chunks.iter().all(|c| c.page_slug == "tag-ai"),
        "tag filter must not match partial JSON string tags: {:?}",
        chunks
    );
}

#[tokio::test]
async fn test_research_recording_api_writes_pages_and_provenance() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let run = engine
        .create_research_run(
            "run-001",
            "Journal Brief Pilot",
            Some("What themes are emerging across the corpus?"),
        )
        .await
        .expect("create_research_run");
    assert_eq!(run.slug, "research/runs/run-001");
    assert_eq!(run.page_type, "research_run");
    assert_eq!(run.frontmatter["run_kind"], "custom");
    assert_eq!(run.frontmatter["profile"], "custom");
    assert_eq!(run.frontmatter["created_by"], "user");

    let dataset = engine
        .register_input(
            &run.slug,
            "datasets/citation-snapshot",
            "Citation Snapshot",
            "dataset",
            "Citation metadata exported for the pilot run.",
            Some(serde_json::json!({
                "source": "csv",
                "record_count": 42
            })),
        )
        .await
        .expect("register_input");
    assert_eq!(dataset.page_type, "dataset");

    let artifact = engine
        .record_artifact(
            &run.slug,
            "artifacts/brief-2026-06",
            "June Brief Draft",
            "brief_draft",
            "outputs/brief-2026-06.md",
            "Draft brief generated from the citation corpus.",
        )
        .await
        .expect("record_artifact");
    assert_eq!(artifact.page_type, "artifact");

    let finding = engine
        .record_finding(
            &run.slug,
            "findings/topic-shift",
            "Topic Shift",
            "draft",
            "The corpus shows a visible shift toward AI-supported research workflows.",
            &[artifact.slug.clone()],
        )
        .await
        .expect("record_finding");
    assert_eq!(finding.page_type, "finding");

    let report = engine
        .record_validation_report(
            &run.slug,
            "validation/topic-shift-citations",
            "Topic Shift Citation Check",
            "citation_support_check",
            "needs_revision",
            "The finding needs more citation-level support before publication.",
            &[finding.slug.clone(), dataset.slug.clone()],
            Some(serde_json::json!([
                {
                    "action_kind": "add_citation_record",
                    "target": finding.slug
                }
            ])),
        )
        .await
        .expect("record_validation_report");
    assert_eq!(report.page_type, "validation_report");

    let action = engine
        .record_action_item(
            &run.slug,
            "actions/add-topic-shift-citations",
            "Add citation support",
            "add_citation_record",
            "open",
            "Add representative citation records supporting the topic shift finding.",
            &[finding.slug.clone()],
        )
        .await
        .expect("record_action_item");
    assert_eq!(action.page_type, "action_item");

    let run_outlinks = engine.outlinks(&run.slug).await.expect("run outlinks");
    assert!(
        run_outlinks
            .iter()
            .any(|link| { link.target_slug == dataset.slug && link.edge_type == "uses_dataset" })
    );
    assert!(
        run_outlinks
            .iter()
            .any(|link| { link.target_slug == artifact.slug && link.edge_type == "produces" })
    );
    assert!(
        run_outlinks
            .iter()
            .any(|link| { link.target_slug == finding.slug && link.edge_type == "produces" })
    );
    assert!(
        run_outlinks
            .iter()
            .any(|link| { link.target_slug == report.slug && link.edge_type == "produces" })
    );
    assert!(
        run_outlinks
            .iter()
            .any(|link| { link.target_slug == action.slug && link.edge_type == "produces" })
    );

    let finding_outlinks = engine
        .outlinks(&finding.slug)
        .await
        .expect("finding outlinks");
    assert!(
        finding_outlinks
            .iter()
            .any(|link| { link.target_slug == artifact.slug && link.edge_type == "supports" })
    );

    let report_outlinks = engine
        .outlinks(&report.slug)
        .await
        .expect("report outlinks");
    assert!(
        report_outlinks
            .iter()
            .any(|link| { link.target_slug == finding.slug && link.edge_type == "validates" })
    );
    assert!(
        report_outlinks
            .iter()
            .any(|link| { link.target_slug == dataset.slug && link.edge_type == "validates" })
    );

    let action_outlinks = engine
        .outlinks(&action.slug)
        .await
        .expect("action outlinks");
    assert!(
        action_outlinks
            .iter()
            .any(|link| { link.target_slug == finding.slug && link.edge_type == "recommends" })
    );

    let provenance = engine
        .provenance_of(&finding.slug, 2)
        .await
        .expect("provenance_of");
    assert!(provenance.nodes.iter().any(|node| node.slug == run.slug));
    assert!(
        provenance
            .nodes
            .iter()
            .any(|node| node.slug == artifact.slug)
    );
    assert!(
        provenance
            .nodes
            .iter()
            .any(|node| node.slug == dataset.slug)
    );
    assert!(provenance.nodes.iter().any(|node| node.slug == report.slug));
    assert!(provenance.nodes.iter().any(|node| node.slug == action.slug));
    assert!(provenance.edges.iter().any(|edge| {
        edge.source_slug == run.slug
            && edge.target_slug == finding.slug
            && edge.edge_type == "produces"
    }));
    assert!(provenance.edges.iter().any(|edge| {
        edge.source_slug == finding.slug
            && edge.target_slug == artifact.slug
            && edge.edge_type == "supports"
    }));
    assert!(provenance.edges.iter().any(|edge| {
        edge.source_slug == report.slug
            && edge.target_slug == finding.slug
            && edge.edge_type == "validates"
    }));
    assert!(provenance.edges.iter().any(|edge| {
        edge.source_slug == action.slug
            && edge.target_slug == finding.slug
            && edge.edge_type == "recommends"
    }));

    let validation_results = engine
        .validate_research_run(&run.slug)
        .await
        .expect("validate_research_run");
    assert!(validation_results.iter().any(|result| {
        result.validator == "research_run_has_input" && result.status.as_str() == "pass"
    }));
    assert!(validation_results.iter().any(|result| {
        result.validator == "finding_has_supporting_evidence" && result.status.as_str() == "pass"
    }));
    assert!(validation_results.iter().any(|result| {
        result.validator == "artifact_hash_present" && result.status.as_str() == "warn"
    }));

    let validated_run = engine.get_page(&run.slug).await.expect("get validated run");
    let timeline =
        TimelineEntry::parse_compat(&validated_run.timeline).expect("parse run timeline");
    assert!(timeline.iter().any(|entry| {
        entry.source == TimelineSource::Validator
            && entry.kind == "validator_run"
            && entry.payload["validator"] == "artifact_hash_present"
    }));

    let protocol = engine
        .get_research_protocol(&run.slug)
        .await
        .expect("get_research_protocol");
    assert_eq!(protocol.run.slug, run.slug);
    assert!(protocol.inputs.iter().any(|page| page.slug == dataset.slug));
    assert!(
        protocol
            .artifacts
            .iter()
            .any(|page| page.slug == artifact.slug)
    );
    assert!(
        protocol
            .findings
            .iter()
            .any(|page| page.slug == finding.slug)
    );
    assert!(
        protocol
            .validation_reports
            .iter()
            .any(|page| page.slug == report.slug)
    );
    assert!(
        protocol
            .action_items
            .iter()
            .any(|page| page.slug == action.slug)
    );
    assert!(
        protocol
            .validation_reports
            .iter()
            .any(|page| { page.slug == format!("{}/validation/artifact_hash_present", run.slug) })
    );
    assert!(
        protocol
            .action_items
            .iter()
            .any(|page| { page.slug == format!("{}/actions/artifact_hash_present-1", run.slug) })
    );
    assert!(protocol.edges.iter().any(|edge| {
        edge.source_slug == report.slug
            && edge.target_slug == finding.slug
            && edge.edge_type == "validates"
    }));
    assert!(protocol.edges.iter().any(|edge| {
        edge.source_slug == action.slug
            && edge.target_slug == finding.slug
            && edge.edge_type == "recommends"
    }));
}

#[tokio::test]
async fn test_context_search_caps_chunks_per_page_for_diversity() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    let repeated = (0..80)
        .map(|i| format!("maxpoolterm appears in long source sentence number {}.", i))
        .collect::<Vec<_>>()
        .join(" ");

    let long_page = Page {
        language: Some(Language::En),
        ..Page::new("maxpool-long".to_string(), "note".to_string(), repeated)
    };
    let short_page = Page {
        language: Some(Language::En),
        ..Page::new(
            "maxpool-short".to_string(),
            "note".to_string(),
            "maxpoolterm appears in a separate source.".to_string(),
        )
    };

    engine.put_page(long_page.clone()).await.expect("put long");
    engine
        .put_page(short_page.clone())
        .await
        .expect("put short");
    engine
        .chunk_and_embed_page(&long_page)
        .await
        .expect("embed long");
    engine
        .chunk_and_embed_page(&short_page)
        .await
        .expect("embed short");

    let chunks = engine
        .search_with_context("maxpoolterm", &Language::En, 2, false)
        .await
        .expect("search_with_context");

    let pages = chunks
        .iter()
        .map(|chunk| chunk.page_slug.as_str())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(
        chunks.len(),
        2,
        "should keep enough diverse results: {:?}",
        chunks
    );
    assert_eq!(
        pages.len(),
        2,
        "should return one chunk per page: {:?}",
        chunks
    );
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
            "The Tang dynasty was an imperial dynasty of China that ruled from 618 to 907."
                .to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("dynasty", &Language::En, 5)
        .await
        .expect("keyword_search");
    assert!(
        !results.is_empty(),
        "English keyword search should return results"
    );
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
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("唐朝", &Language::ZhHans, 5)
        .await
        .expect("keyword_search zh");
    assert!(
        !results.is_empty(),
        "Chinese keyword search for 唐朝 should return results with lindera CC-CEDICT"
    );
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
            "日本の歴史において、江戸時代は重要な時代です。文化が発展し、経済も成長しました。"
                .to_string(),
        )
    };
    engine.put_page(page.clone()).await.expect("put_page");
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

    let results = engine
        .keyword_search("江戸", &Language::Ja, 5)
        .await
        .expect("keyword_search ja");
    assert!(
        !results.is_empty(),
        "Japanese keyword search for 江戸 should return results with lindera IPADIC"
    );
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
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");

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
    let r1 = idx1
        .search("test", &Language::En, 5)
        .await
        .expect("idx1 search");
    let r2 = idx2
        .search("test", &Language::En, 5)
        .await
        .expect("idx2 search");

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
    engine
        .chunk_and_embed_page(&page)
        .await
        .expect("chunk_and_embed_page");
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
async fn test_dream_cycle_flow() {
    let tb = TestBrain::new().await;
    let engine = open_mock_engine(&tb).await;

    // Create page 1 with tags and mentions of a concept (e.g. 预训练语言模型)
    let page1 = Page {
        tags: vec!["nlp".to_string()],
        ..Page::new(
            "paper-1".to_string(),
            "paper".to_string(),
            "摘要：预训练语言模型是现代自然语言处理的基础。我们将研究BERT的性能表现。".to_string(),
        )
    };

    // Create page 2 with same tag
    let page2 = Page {
        tags: vec!["nlp".to_string()],
        ..Page::new(
            "paper-2".to_string(),
            "paper".to_string(),
            "摘要：基于预训练语言模型，我们实现了多种下游NLP任务的性能突破。".to_string(),
        )
    };

    engine.put_page(page1).await.expect("put page1");
    engine.put_page(page2).await.expect("put page2");

    // Run dream cycle (all stages)
    engine.run_dream_cycle(None).await.expect("run_dream_cycle");

    // 1. Verify that "concepts/预训练语言模型" was automatically created
    let concept_page = engine
        .get_page("concepts/预训练语言模型")
        .await
        .expect("get concept page");
    assert_eq!(concept_page.title, "预训练语言模型");

    // 2. Verify that "figures/bert" was automatically created
    let figure_page = engine
        .get_page("figures/bert")
        .await
        .expect("get figure page");
    assert_eq!(figure_page.title, "BERT");

    // 3. Verify that LLM-extracted timeline events were not written back to source pages
    let fetched_p1 = engine.get_page("paper-1").await.expect("get page1");
    assert!(
        !fetched_p1
            .timeline
            .contains("BERT model was officially released"),
        "dream extraction must not mutate source timeline: {}",
        fetched_p1.timeline
    );

    // 4. Verify that "synthesis/tag-nlp" was automatically created
    let synth_page = engine
        .get_page("synthesis/tag-nlp")
        .await
        .expect("get synthesis page");
    assert!(
        synth_page.title.contains("nlp"),
        "Synthesis page title incorrect: {}",
        synth_page.title
    );
    assert!(
        synth_page.compiled_truth.contains("paper-1"),
        "Synthesis content should refer to paper-1"
    );
    assert!(
        synth_page.compiled_truth.contains("paper-2"),
        "Synthesis content should refer to paper-2"
    );
}
