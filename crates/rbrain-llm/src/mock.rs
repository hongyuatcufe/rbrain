use async_trait::async_trait;
use rbrain_core::embedder::Embedder;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Deterministic mock embedder that produces vectors based on text content.
/// Uses bag-of-words model where each word maps to a small fixed vector,
/// ensuring similar texts produce similar vectors.
#[derive(Debug)]
#[allow(dead_code)]
pub struct MockEmbedder {
    dim: usize,
    word_vectors: HashMap<String, Vec<f32>>,
}

impl MockEmbedder {
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            word_vectors: HashMap::new(),
        }
    }

    fn hash_word(word: &str, dim: usize) -> Vec<f32> {
        let mut hasher = DefaultHasher::new();
        word.hash(&mut hasher);
        let hash = hasher.finish();

        (0..dim)
            .map(|i| {
                // Deterministic float from hash bits
                let bit = ((hash >> (i % 64)) & 1) as f32;
                bit * 2.0 - 1.0
            })
            .collect()
    }

    fn embed_text(&self, text: &str) -> Vec<f32> {
        let lower = text.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            return vec![0.0; self.dim];
        }

        // Average word vectors (bag-of-words)
        let mut result = vec![0.0; self.dim];
        for word in &words {
            let word_vec = Self::hash_word(word, self.dim);
            for (r, &w) in result.iter_mut().zip(&word_vec) {
                *r += w;
            }
        }

        // Normalize
        let n = words.len() as f32;
        for r in &mut result {
            *r /= n;
        }

        result
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    fn dimension(&self) -> usize {
        self.dim
    }

    async fn embed_one(&self, text: &str) -> std::result::Result<Vec<f32>, rbrain_core::error::BrainError> {
        Ok(self.embed_text(text))
    }

    async fn embed_batch(&self, texts: &[String]) -> std::result::Result<Vec<Vec<f32>>, rbrain_core::error::BrainError> {
        Ok(texts.iter().map(|t| self.embed_text(t)).collect())
    }

    fn verify_deterministic(&self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
pub struct MockLlm {
    pub responses: HashMap<String, String>,
}

impl MockLlm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_response(mut self, prompt: &str, response: &str) -> Self {
        self.responses.insert(prompt.to_string(), response.to_string());
        self
    }

    pub fn get_response(&self, prompt: &str) -> Option<&String> {
        self.responses.get(prompt)
    }
}
