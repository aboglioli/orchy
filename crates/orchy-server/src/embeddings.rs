use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};

pub struct OpenAiEmbeddingsProvider {
    client: Client,
    url: String,
    model: String,
    dimensions: u32,
}

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: serde_json::Value,
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbeddingsProvider {
    pub fn new(url: String, model: String, dimensions: u32) -> Self {
        Self {
            client: Client::new(),
            url,
            model,
            dimensions,
        }
    }
}

#[async_trait]
impl EmbeddingsProvider for OpenAiEmbeddingsProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbeddingsRequest {
            model: &self.model,
            input: serde_json::Value::String(text.to_string()),
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        let body: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        body.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| Error::Embeddings("empty response".into()))
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let input: Vec<serde_json::Value> = texts
            .iter()
            .map(|t| serde_json::Value::String(t.to_string()))
            .collect();

        let request = EmbeddingsRequest {
            model: &self.model,
            input: serde_json::Value::Array(input),
        };

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        let body: EmbeddingsResponse = response
            .json()
            .await
            .map_err(|e| Error::Embeddings(e.to_string()))?;

        Ok(body.data.into_iter().map(|d| d.embedding).collect())
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> u32 {
        self.dimensions
    }
}

pub enum EmbeddingsBackend {
    OpenAi(OpenAiEmbeddingsProvider),
}

#[async_trait]
impl EmbeddingsProvider for EmbeddingsBackend {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed(text).await,
        }
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.embed_batch(texts).await,
        }
    }

    fn model(&self) -> &str {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.model(),
        }
    }

    fn dimensions(&self) -> u32 {
        match self {
            EmbeddingsBackend::OpenAi(p) => p.dimensions(),
        }
    }
}
