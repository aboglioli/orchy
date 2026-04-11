use std::sync::Arc;

use crate::embeddings::EmbeddingsBackend;
use crate::entities::{MemoryEntry, MemoryFilter, WriteMemory};
use crate::error::Result;
use crate::store::Store;
use crate::value_objects::Namespace;

pub struct MemoryService {
    store: Arc<Store>,
    embeddings: Option<Arc<EmbeddingsBackend>>,
}

impl MemoryService {
    pub fn new(store: Arc<Store>, embeddings: Option<Arc<EmbeddingsBackend>>) -> Self {
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
