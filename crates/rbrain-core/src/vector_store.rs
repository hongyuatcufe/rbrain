use async_trait::async_trait;
use crate::error::Result;

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, chunk_id: i64, embedding: &[f32]) -> Result<()>;
    async fn upsert_batch(&self, items: &[(i64, Vec<f32>)]) -> Result<()>;
    async fn delete(&self, chunk_id: i64) -> Result<()>;
    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>>;
    async fn save(&self) -> Result<()>;
    fn load(&self) -> Result<()>;
}
