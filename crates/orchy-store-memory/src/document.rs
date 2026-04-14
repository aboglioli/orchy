use orchy_core::document::{Document, DocumentFilter, DocumentId, DocumentStore};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{MemoryBackend, cosine_similarity};

impl DocumentStore for MemoryBackend {
    async fn save(&self, doc: &Document) -> Result<()> {
        let mut docs = self
            .documents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        docs.insert(doc.id(), doc.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &DocumentId) -> Result<Option<Document>> {
        let docs = self
            .documents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(docs.get(id).cloned())
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Document>> {
        let docs = self
            .documents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(docs
            .values()
            .find(|d| d.project() == project && d.namespace() == namespace && d.path() == path)
            .cloned())
    }

    async fn list(&self, filter: DocumentFilter) -> Result<Vec<Document>> {
        let docs = self
            .documents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let results: Vec<Document> = docs
            .values()
            .filter(|d| {
                if let Some(ref ns) = filter.namespace {
                    if !d.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if d.project() != project {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.tag {
                    if !d.tags().contains(tag) {
                        return false;
                    }
                }
                if let Some(ref prefix) = filter.path_prefix {
                    if !d.path().starts_with(prefix.as_str()) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        Ok(results)
    }

    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let docs = self
            .documents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let query_lower = query.to_lowercase();
        let mut scored: Vec<(f32, &Document)> = docs
            .values()
            .filter(|d| {
                if let Some(ns) = namespace {
                    if !d.namespace().starts_with(ns) {
                        return false;
                    }
                }
                true
            })
            .filter_map(|d| {
                if let (Some(emb), Some(doc_emb)) = (embedding, d.embedding()) {
                    let score = cosine_similarity(emb, doc_emb);
                    if score > 0.0 {
                        return Some((score, d));
                    }
                }
                let text = format!("{} {}", d.title(), d.content()).to_lowercase();
                if text.contains(&query_lower) {
                    return Some((0.5, d));
                }
                None
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, d)| d.clone()).collect())
    }

    async fn delete(&self, id: &DocumentId) -> Result<()> {
        let mut docs = self
            .documents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        docs.remove(id);
        Ok(())
    }
}
