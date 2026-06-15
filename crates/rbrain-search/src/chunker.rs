use rbrain_core::page::Language;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub page_slug: String,
    pub chunk_idx: usize,
    pub text: String,
    pub is_compiled_truth: bool,
    pub language: Language,
}

#[derive(Debug)]
pub struct Chunker {
    pub target_chars: usize,
    pub overlap_chars: usize,
}

impl Chunker {
    #[must_use]
    pub fn new(lang: &Language) -> Self {
        match lang {
            Language::Ja | Language::ZhHans | Language::ZhHant | Language::Ko => Self {
                target_chars: 600,
                overlap_chars: 100,
            },
            Language::En => Self {
                target_chars: 1600,
                overlap_chars: 200,
            },
            Language::Other(_) => Self {
                target_chars: 600,
                overlap_chars: 100,
            },
        }
    }

    #[must_use]
    pub fn chunk(&self, text: &str, page_slug: &str, lang: &Language) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let sentences = Self::split_sentences(text, lang);
        let mut current_text = String::new();
        let mut idx: usize = 0;
        let mut is_compiled_truth: bool = true; // First pass: compiled truth

        for sentence in sentences {
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Timeline separator: create chunk for compiled truth, reset for timeline
            // Must be a standalone "---" line with existing content before it
            if trimmed == "---" && !current_text.trim().is_empty() {
                if !current_text.trim().is_empty() {
                    let chunk_text = current_text.trim().to_string();
                    if chunk_text.len() >= 10 {
                        chunks.push(Chunk {
                            page_slug: page_slug.to_string(),
                            chunk_idx: idx,
                            text: chunk_text,
                            is_compiled_truth: true,
                            language: lang.clone(),
                        });
                        idx += 1;
                    }
                }
                current_text.clear();
                is_compiled_truth = false;
                continue;
            }

            // Check if adding this sentence would exceed target
            if current_text.len() + trimmed.len() > self.target_chars && !current_text.is_empty() {
                let overlap = if current_text.len() > self.overlap_chars {
                    // Use char_indices to safely find character boundary
                    let chars_to_skip = current_text
                        .chars()
                        .count()
                        .saturating_sub(self.overlap_chars);
                    current_text.chars().skip(chars_to_skip).collect::<String>()
                } else {
                    current_text.clone()
                };

                let chunk_text = current_text.trim().to_string();
                if chunk_text.len() >= 10 {
                    chunks.push(Chunk {
                        page_slug: page_slug.to_string(),
                        chunk_idx: idx,
                        text: chunk_text,
                        is_compiled_truth,
                        language: lang.clone(),
                    });
                    idx += 1;
                }
                current_text = overlap;
            }

            if !current_text.is_empty() {
                current_text.push(' ');
            }
            current_text.push_str(trimmed);
        }

        // Add remaining text as final chunk
        if !current_text.trim().is_empty() && current_text.trim().len() >= 10 {
            chunks.push(Chunk {
                page_slug: page_slug.to_string(),
                chunk_idx: idx,
                text: current_text.trim().to_string(),
                is_compiled_truth,
                language: lang.clone(),
            });
        }

        chunks
    }

    fn split_sentences(text: &str, _lang: &Language) -> Vec<String> {
        let mut sentences: Vec<String> = Vec::new();
        let mut current = String::new();

        for ch in text.chars() {
            current.push(ch);
            if matches!(ch, '。' | '！' | '？' | '.' | '!' | '?') {
                // Check if it's end of sentence (not abbreviation like Mr.)
                let is_end_of_sentence = if ch == '.' {
                    current.len() <= 3
                        || current.chars().filter(|c| c.is_whitespace()).count() >= 1
                        || !current.chars().any(|c| c.is_lowercase())
                } else {
                    true
                };

                if is_end_of_sentence {
                    sentences.push(current.clone());
                    current.clear();
                }
            }
            if ch == '\n' && !current.is_empty() && !current.ends_with('\n') {
                sentences.push(current.clone());
                current.clear();
            }
        }

        if !current.is_empty() && !current.trim().is_empty() {
            sentences.push(current);
        }

        // Handle paragraph breaks
        let mut result: Vec<String> = Vec::new();
        for s in sentences {
            if s.contains("\n\n") {
                for part in s.split("\n\n") {
                    if !part.trim().is_empty() {
                        result.push(part.trim().to_string());
                    }
                }
            } else if !s.trim().is_empty() {
                result.push(s);
            }
        }

        if result.is_empty() && !text.trim().is_empty() {
            vec![text.trim().to_string()]
        } else {
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker_new_cjk() {
        let chunker = Chunker::new(&Language::Ja);
        assert_eq!(chunker.target_chars, 600);
        assert_eq!(chunker.overlap_chars, 100);

        let chunker = Chunker::new(&Language::ZhHans);
        assert_eq!(chunker.target_chars, 600);
        assert_eq!(chunker.overlap_chars, 100);

        let chunker = Chunker::new(&Language::ZhHant);
        assert_eq!(chunker.target_chars, 600);
        assert_eq!(chunker.overlap_chars, 100);

        let chunker = Chunker::new(&Language::Ko);
        assert_eq!(chunker.target_chars, 600);
        assert_eq!(chunker.overlap_chars, 100);
    }

    #[test]
    fn test_chunker_new_english() {
        let chunker = Chunker::new(&Language::En);
        assert_eq!(chunker.target_chars, 1600);
        assert_eq!(chunker.overlap_chars, 200);
    }

    #[test]
    fn test_chunker_new_other() {
        let chunker = Chunker::new(&Language::Other("fr".to_string()));
        assert_eq!(chunker.target_chars, 600);
        assert_eq!(chunker.overlap_chars, 100);
    }

    #[test]
    fn test_chunk_simple_text() {
        let chunker = Chunker::new(&Language::Ja);
        let text = "これはテストです。次の文もあります。もう一つ文を追加します。";
        let chunks = chunker.chunk(text, "test-page", &Language::Ja);

        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].page_slug, "test-page");
        assert_eq!(chunks[0].chunk_idx, 0);
        assert!(chunks[0].is_compiled_truth);
    }

    #[test]
    fn test_chunk_with_timeline_separator() {
        let chunker = Chunker::new(&Language::Ja);
        let text = "これはコンパイルドトゥルースです。\n\n---\n\nこれはタイムラインのセクションで、もっと長いテキストが含まれています。さらに追加の文があります。";
        let chunks = chunker.chunk(text, "test-page", &Language::Ja);

        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].is_compiled_truth);
        assert!(!chunks[1].is_compiled_truth);
    }

    #[test]
    fn test_split_sentences_ja() {
        let text = "これはテストです。次の文もあります！最後の文です？";
        let sentences = Chunker::split_sentences(text, &Language::Ja);

        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_split_sentences_en() {
        let text = "This is a test. Here is another sentence! Is this the last one?";
        let sentences = Chunker::split_sentences(text, &Language::En);

        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn test_chunk_structure() {
        let chunker = Chunker::new(&Language::Ja);
        let text = "短いテキスト。";
        let chunks = chunker.chunk(text, "my-page", &Language::Ja);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].page_slug, "my-page");
        assert_eq!(chunks[0].chunk_idx, 0);
        assert_eq!(chunks[0].language, Language::Ja);
    }
}
