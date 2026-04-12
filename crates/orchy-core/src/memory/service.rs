use std::sync::Arc;

use super::{ContextSnapshot, CreateSnapshot, MemoryEntry, MemoryFilter, WriteMemory};
use crate::agent::AgentId;
use crate::embeddings::EmbeddingsBackend;
use crate::error::Result;
use crate::namespace::Namespace;
use crate::store::Store;

pub struct MemoryService<S: Store> {
    store: Arc<S>,
    embeddings: Option<Arc<EmbeddingsBackend>>,
}

impl<S: Store> MemoryService<S> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<EmbeddingsBackend>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn write(&self, mut entry: WriteMemory) -> Result<MemoryEntry> {
        if let Some(emb) = &self.embeddings {
            let vector = emb.embed(&entry.value).await?;
            entry.embedding = Some(vector);
            entry.embedding_model = Some(emb.model().to_string());
            entry.embedding_dimensions = Some(emb.dimensions());
        }
        self.store.write_memory(entry).await
    }

    pub async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        self.store.read_memory(namespace, key).await
    }

    pub async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        self.store.list_memory(filter).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(query).await?)
        } else {
            None
        };
        self.store
            .search_memory(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        self.store.delete_memory(namespace, key).await
    }
}

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
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        self.store.list_contexts(agent, namespace).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: &Namespace,
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
