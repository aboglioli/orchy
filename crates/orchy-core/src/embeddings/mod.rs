pub mod openai;
pub mod search;

use crate::error::Result;
pub use openai::OpenAiEmbeddingsProvider;

pub trait EmbeddingsProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn model(&self) -> &str;
    fn dimensions(&self) -> u32;
}

pub enum EmbeddingsBackend {
    OpenAi(OpenAiEmbeddingsProvider),
}

impl EmbeddingsBackend {
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed(text).await,
        }
    }

    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed_batch(texts).await,
        }
    }

    pub fn model(&self) -> &str {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.model(),
        }
    }

    pub fn dimensions(&self) -> u32 {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.dimensions(),
        }
    }
}
