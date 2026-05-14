use async_trait::async_trait;
use ferrous_opencc::OpenCC;
use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use rbrain_core::error::{BrainError, Result};
use rbrain_core::keyword_index::KeywordIndex;
use rbrain_core::page::Language;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

/// Per-language segmenters for CJK pre-tokenisation.
/// Segments text into morphemes before handing off to tantivy's whitespace tokeniser,
/// enabling exact morpheme matching without a custom tantivy Tokenizer impl.
struct CjkSegmenters {
    ja: Option<Segmenter>,
    zh: Option<Segmenter>,
    ko: Option<Segmenter>,
}

impl CjkSegmenters {
    fn new() -> Self {
        let ja = load_dictionary("embedded://ipadic")
            .ok()
            .map(|d| Segmenter::new(Mode::Normal, d, None));
        let zh = load_dictionary("embedded://cc-cedict")
            .ok()
            .map(|d| Segmenter::new(Mode::Normal, d, None));
        let ko = load_dictionary("embedded://ko-dic")
            .ok()
            .map(|d| Segmenter::new(Mode::Normal, d, None));

        if ja.is_none() || zh.is_none() || ko.is_none() {
            tracing::warn!(
                "One or more CJK dictionaries failed to load \
                 (ja={}, zh={}, ko={}); CJK keyword search will be degraded",
                ja.is_some(), zh.is_some(), ko.is_some()
            );
        }

        Self { ja, zh, ko }
    }

    /// Segment CJK text into space-joined morphemes.
    /// Returns the original text unchanged for English or on failure.
    fn segment<'a>(&'a self, text: &str, lang_code: &str) -> String {
        let seg = match lang_code {
            "ja" => self.ja.as_ref(),
            "zh" => self.zh.as_ref(),
            "ko" => self.ko.as_ref(),
            _ => return text.to_string(),
        };

        let Some(segmenter) = seg else {
            return text.to_string();
        };

        match segmenter.segment(Cow::Borrowed(text)) {
            Ok(tokens) => tokens
                .iter()
                .map(|t| t.surface.as_ref())
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            Err(_) => text.to_string(),
        }
    }
}

struct IndexHandle {
    index: Index,
    /// Lazy writer: None when idle (no file lock held), Some when buffering writes.
    /// Created on first upsert/delete, dropped (and file lock released) after commit().
    writer: Mutex<Option<IndexWriter>>,
    reader: IndexReader,
    schema: Schema,
}

pub struct TantivyIndex {
    indices: Mutex<HashMap<String, IndexHandle>>,
    root_dir: PathBuf,
    segmenters: CjkSegmenters,
}

impl std::fmt::Debug for TantivyIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TantivyIndex")
            .field("root_dir", &self.root_dir)
            .field("num_indices", &self.indices.lock().unwrap().len())
            .finish()
    }
}

impl TantivyIndex {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        let mut indices = HashMap::new();

        let langs = ["ja", "zh", "ko", "en"];

        for lang_code in &langs {
            let dir = root_dir.join(lang_code);
            std::fs::create_dir_all(&dir).map_err(BrainError::Io)?;

            let mut schema_builder = Schema::builder();
            schema_builder.add_i64_field("chunk_id", STORED | INDEXED | FAST);
            schema_builder.add_text_field("text", TEXT | STORED);
            schema_builder.add_text_field("page_slug", STRING | STORED);
            let schema = schema_builder.build();

            let index = if std::fs::read_dir(&dir)
                .map_err(BrainError::Io)?
                .next()
                .is_some()
            {
                Index::open_in_dir(&dir).map_err(|e| {
                    BrainError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("tantivy open: {}", e),
                    ))
                })?
            } else {
                Index::create_in_dir(&dir, schema.clone()).map_err(|e| {
                    BrainError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("tantivy create: {}", e),
                    ))
                })?
            };

            let reader = index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into()
                .map_err(|e| {
                    BrainError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("tantivy reader: {}", e),
                    ))
                })?;

            indices.insert(
                lang_code.to_string(),
                IndexHandle {
                    index,
                    writer: Mutex::new(None), // no writer until first write
                    reader,
                    schema,
                },
            );
        }

        Ok(Self {
            indices: Mutex::new(indices),
            root_dir,
            segmenters: CjkSegmenters::new(),
        })
    }

    fn lang_code(lang: &Language) -> String {
        match lang {
            Language::Ja => "ja".to_string(),
            Language::ZhHans | Language::ZhHant => "zh".to_string(),
            Language::Ko => "ko".to_string(),
            _ => "en".to_string(),
        }
    }

    fn simplify_chinese(text: &str) -> String {
        use ferrous_opencc::config::BuiltinConfig;
        match OpenCC::from_config(BuiltinConfig::T2s) {
            Ok(oc) => oc.convert(text),
            Err(_) => text.to_string(),
        }
    }

    /// Acquire the lazy writer for a handle, creating it if not yet open.
    /// Caller holds the outer `indices` lock; do not call commit() inside this closure.
    fn ensure_writer(handle: &IndexHandle) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        let mut guard = handle.writer.lock().unwrap();
        if guard.is_none() {
            let w = handle.index.writer(50_000_000).map_err(|e| {
                BrainError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("tantivy writer: {}", e),
                ))
            })?;
            *guard = Some(w);
        }
        Ok(guard)
    }
}

#[async_trait]
impl KeywordIndex for TantivyIndex {
    async fn upsert(&self, chunk_id: i64, page_slug: &str, text: &str, lang: &Language) -> Result<()> {
        let code = Self::lang_code(lang);
        // Simplify Traditional→Simplified before segmentation
        let base = if matches!(lang, Language::ZhHans | Language::ZhHant) {
            Self::simplify_chinese(text)
        } else {
            text.to_string()
        };
        // Pre-segment CJK so tantivy's whitespace tokeniser finds morphemes
        let normalized = self.segmenters.segment(&base, &code);

        let indices = self.indices.lock().unwrap();
        let handle = indices.get(&code).ok_or_else(|| BrainError::DictionaryNotFound {
            lang: code.clone(),
        })?;

        let schema = &handle.schema;
        let chunk_id_field = schema.get_field("chunk_id").map_err(|e| {
            BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("field lookup: {}", e)))
        })?;
        let text_field = schema.get_field("text").map_err(|e| {
            BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("field lookup: {}", e)))
        })?;
        let page_slug_field = schema.get_field("page_slug").map_err(|e| {
            BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("field lookup: {}", e)))
        })?;

        let mut wguard = Self::ensure_writer(handle)?;
        let writer = wguard.as_mut().unwrap();

        writer.delete_term(tantivy::Term::from_field_i64(chunk_id_field, chunk_id));
        writer.add_document(doc! {
            chunk_id_field => chunk_id,
            text_field => normalized,
            page_slug_field => page_slug,
        }).map_err(|e| {
            BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy add: {}", e)))
        })?;

        Ok(())
    }

    async fn delete(&self, chunk_id: i64) -> Result<()> {
        let indices = self.indices.lock().unwrap();
        for (_, handle) in indices.iter() {
            let schema = &handle.schema;
            if let Ok(chunk_id_field) = schema.get_field("chunk_id") {
                let mut wguard = Self::ensure_writer(handle)?;
                let writer = wguard.as_mut().unwrap();
                writer.delete_term(tantivy::Term::from_field_i64(chunk_id_field, chunk_id));
            }
        }
        Ok(())
    }

    async fn search(&self, query: &str, lang: &Language, k: usize) -> Result<Vec<(i64, f32)>> {
        let code = Self::lang_code(lang);
        let base = if matches!(lang, Language::ZhHans | Language::ZhHant) {
            Self::simplify_chinese(query)
        } else {
            query.to_string()
        };
        // Apply the same segmentation as upsert so query terms match indexed terms
        let normalized = self.segmenters.segment(&base, &code);

        let indices = self.indices.lock().unwrap();
        let mut results: HashMap<i64, Vec<f32>> = HashMap::new();

        for target_code in [code.as_str(), "en"] {
            if let Some(handle) = indices.get(target_code) {
                let schema = &handle.schema;
                let text_field = schema.get_field("text").map_err(|e| {
                    BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("field lookup: {}", e)))
                })?;
                let chunk_id_field = schema.get_field("chunk_id").map_err(|e| {
                    BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("field lookup: {}", e)))
                })?;

                let query_parser = QueryParser::for_index(&handle.index, vec![text_field]);
                let query_obj = query_parser.parse_query(&normalized).map_err(|e| {
                    BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy query parse: {}", e)))
                })?;

                let searcher = handle.reader.searcher();
                let top_docs: Vec<(f32, tantivy::DocAddress)> = searcher
                    .search(&query_obj, &TopDocs::with_limit(k).order_by_score())
                    .map_err(|e| {
                        BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy search: {}", e)))
                    })?;

                for (score, addr) in top_docs {
                    let doc: TantivyDocument = searcher.doc(addr).map_err(|e| {
                        BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy doc: {}", e)))
                    })?;
                    if let Some(val) = doc.get_first(chunk_id_field) {
                        if let Some(cid) = val.as_i64() {
                            results.entry(cid).or_default().push(score);
                        }
                    }
                }
            }
        }

        let mut final_results: Vec<(i64, f32)> = results
            .into_iter()
            .map(|(id, scores)| {
                let max_score = scores.into_iter().fold(0.0f32, f32::max);
                (id, max_score)
            })
            .collect();

        final_results
            .sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        Ok(final_results)
    }

    async fn commit(&self) -> Result<()> {
        let indices = self.indices.lock().unwrap();
        for (_, handle) in indices.iter() {
            let mut wguard = handle.writer.lock().unwrap();
            if let Some(mut writer) = wguard.take() {
                // commit() flushes buffered documents
                writer.commit().map_err(|e| {
                    BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy commit: {}", e)))
                })?;
                // writer is dropped here, releasing the file lock
                handle.reader.reload().map_err(|e| {
                    BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("tantivy reload: {}", e)))
                })?;
            }
        }
        Ok(())
    }
}
