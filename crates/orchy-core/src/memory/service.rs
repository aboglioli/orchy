use std::sync::Arc;

use super::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore, WriteMemory};
use crate::agent::AgentId;
use crate::embeddings::EmbeddingsProvider;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

pub struct MemoryService<S: MemoryStore, E: EmbeddingsProvider> {
    store: Arc<S>,
    embeddings: Option<Arc<E>>,
}

impl<S: MemoryStore, E: EmbeddingsProvider> MemoryService<S, E> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<E>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn write(&self, cmd: WriteMemory) -> Result<MemoryEntry> {
        let existing = self
            .store
            .find_by_key(&cmd.project, &cmd.namespace, &cmd.key)
            .await?;

        let mut entry = if let Some(mut existing) = existing {
            if let Some(expected) = cmd.expected_version.filter(|v| existing.version() != *v) {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: existing.version().as_u64(),
                });
            }
            existing.update(cmd.value, cmd.written_by);
            existing
        } else {
            if let Some(expected) = cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }
            MemoryEntry::new(
                cmd.project,
                cmd.namespace,
                cmd.key,
                cmd.value,
                cmd.written_by,
            )?
        };

        if let Some(emb) = &self.embeddings {
            let vector = emb.embed(entry.value()).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&entry).await?;
        Ok(entry)
    }

    pub async fn read(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<Option<MemoryEntry>> {
        self.store.find_by_key(project, namespace, key).await
    }

    pub async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        self.store.list(filter).await
    }

    pub async fn move_entry(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
        new_namespace: Namespace,
    ) -> Result<MemoryEntry> {
        let mut entry = self
            .store
            .find_by_key(project, namespace, key)
            .await?
            .ok_or_else(|| Error::NotFound(format!("memory {namespace}/{key}")))?;

        let old_namespace = entry.namespace().clone();
        let old_key = entry.key().to_string();
        entry.move_to(new_namespace);
        self.store.save(&entry).await?;
        self.store.delete(project, &old_namespace, &old_key).await?;
        Ok(entry)
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
            .search(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<()> {
        self.store.delete(project, namespace, key).await
    }
}

pub struct ContextService<S: ContextStore, E: EmbeddingsProvider> {
    store: Arc<S>,
    embeddings: Option<Arc<E>>,
}

impl<S: ContextStore, E: EmbeddingsProvider> ContextService<S, E> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<E>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn save(
        &self,
        project: ProjectId,
        agent_id: AgentId,
        namespace: Namespace,
        summary: String,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<ContextSnapshot> {
        let mut snapshot = ContextSnapshot::new(project, agent_id, namespace, summary, metadata);

        if let Some(emb) = &self.embeddings {
            let vector = emb.embed(snapshot.summary()).await?;
            snapshot.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&snapshot).await?;
        Ok(snapshot)
    }

    pub async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        self.store.find_latest(agent).await
    }

    pub async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        self.store.list(agent, namespace).await
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
            .search(query, embedding.as_deref(), namespace, agent_id, limit)
            .await
    }
}
