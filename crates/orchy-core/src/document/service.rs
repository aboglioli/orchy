use std::sync::Arc;

use super::{Document, DocumentFilter, DocumentId, DocumentStore, WriteDocument};
use crate::embeddings::EmbeddingsProvider;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

pub struct DocumentService<S: DocumentStore, E: EmbeddingsProvider> {
    store: Arc<S>,
    embeddings: Option<Arc<E>>,
}

impl<S: DocumentStore, E: EmbeddingsProvider> DocumentService<S, E> {
    pub fn new(store: Arc<S>, embeddings: Option<Arc<E>>) -> Self {
        Self { store, embeddings }
    }

    pub async fn write(&self, cmd: WriteDocument) -> Result<Document> {
        let existing = self
            .store
            .find_by_path(&cmd.project, &cmd.namespace, &cmd.path)
            .await?;

        let mut doc = if let Some(mut existing) = existing {
            if let Some(expected) = cmd.expected_version.filter(|v| existing.version() != *v) {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: existing.version().as_u64(),
                });
            }
            existing.update(cmd.title, cmd.content, cmd.written_by);
            for tag in &cmd.tags {
                existing.add_tag(tag.clone());
            }
            existing
        } else {
            if let Some(expected) = cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }
            Document::new(
                cmd.project,
                cmd.namespace,
                cmd.path,
                cmd.title,
                cmd.content,
                cmd.tags,
                cmd.written_by,
            )?
        };

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", doc.title(), doc.content());
            let vector = emb.embed(&text).await?;
            doc.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut doc).await?;
        Ok(doc)
    }

    pub async fn read_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Document>> {
        self.store.find_by_path(project, namespace, path).await
    }

    pub async fn get(&self, id: &DocumentId) -> Result<Document> {
        self.store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("document {id}")))
    }

    pub async fn list(&self, filter: DocumentFilter) -> Result<Vec<Document>> {
        self.store.list(filter).await
    }

    pub async fn search(
        &self,
        query: &str,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(query).await?)
        } else {
            None
        };
        self.store
            .search(query, embedding.as_deref(), namespace, limit)
            .await
    }

    pub async fn delete(&self, id: &DocumentId) -> Result<()> {
        self.get(id).await?;
        self.store.delete(id).await
    }

    pub async fn move_doc(&self, id: &DocumentId, new_namespace: Namespace) -> Result<Document> {
        let mut doc = self.get(id).await?;
        doc.move_to(new_namespace);
        self.store.save(&mut doc).await?;
        Ok(doc)
    }

    pub async fn rename(&self, id: &DocumentId, new_path: String) -> Result<Document> {
        let mut doc = self.get(id).await?;
        doc.rename(new_path)?;
        self.store.save(&mut doc).await?;
        Ok(doc)
    }

    pub async fn tag(&self, id: &DocumentId, tag: String) -> Result<Document> {
        let mut doc = self.get(id).await?;
        doc.add_tag(tag);
        self.store.save(&mut doc).await?;
        Ok(doc)
    }

    pub async fn untag(&self, id: &DocumentId, tag: &str) -> Result<Document> {
        let mut doc = self.get(id).await?;
        doc.remove_tag(tag);
        self.store.save(&mut doc).await?;
        Ok(doc)
    }
}
