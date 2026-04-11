use std::sync::Arc;

use crate::embeddings::EmbeddingsBackend;
use crate::entities::{ContextSnapshot, CreateSnapshot};
use crate::error::Result;
use crate::store::Store;
use crate::value_objects::{AgentId, Namespace};

pub struct ContextService<S: Store> {
    store: Arc<S>,
    embeddings: Option<Arc<EmbeddingsBackend>>,
}

impl<S: Store> ContextService<S> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<EmbeddingsBackend>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn save(&self, mut snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        if let Some(emb) = &self.embeddings {
            let vector = emb.embed(&snapshot.summary).await?;
            snapshot.embedding = Some(vector);
            snapshot.embedding_model = Some(emb.model().to_string());
            snapshot.embedding_dimensions = Some(emb.dimensions());
        }
        self.store.save_context(snapshot).await
    }

    pub async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        self.store.load_context(agent).await
    }

    pub async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        self.store.list_contexts(agent, namespace).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(query).await?)
        } else {
            None
        };
        self.store
            .search_contexts(query, embedding.as_deref(), namespace, agent_id, limit)
            .await
    }
}
