use async_trait::async_trait;

use crate::error::ApplicationResult;

#[async_trait]
pub trait EmbeddingsProvider: Send + Sync {
    async fn embed(&self, text: &str) -> ApplicationResult<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> ApplicationResult<Vec<Vec<f32>>>;
    fn model(&self) -> &str;
    fn dimensions(&self) -> u32;
}
