use async_trait::async_trait;
use crate::error::Result;

#[async_trait]
pub trait Embedder: Send + Sync {
    fn dimension(&self) -> usize;
    async fn embed_one(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn verify_deterministic(&self) -> bool;
}
