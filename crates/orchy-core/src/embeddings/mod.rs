pub mod search;

use crate::error::Result;

pub trait EmbeddingsProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model(&self) -> &str;
    fn dimensions(&self) -> u32;
}
