use async_trait::async_trait;
use crate::error::Result;
use crate::page::Language;

#[async_trait]
pub trait KeywordIndex: Send + Sync {
    async fn upsert(&self, chunk_id: i64, page_slug: &str, text: &str, lang: &Language) -> Result<()>;
    async fn delete(&self, chunk_id: i64) -> Result<()>;
    async fn search(&self, query: &str, lang: &Language, k: usize) -> Result<Vec<(i64, f32)>>;
    async fn commit(&self) -> Result<()>;
}
