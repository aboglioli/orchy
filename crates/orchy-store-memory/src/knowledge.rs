use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeStore};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::MemoryBackend;

impl KnowledgeStore for MemoryBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        {
            let mut entries = self
                .knowledge_entries
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            entries.insert(entry.id(), entry.clone());
        }

        let events = entry.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }
        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(entries.get(id).cloned())
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(entries
            .values()
            .find(|e| e.project() == project && e.namespace() == namespace && e.path() == path)
            .cloned())
    }

    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let results: Vec<Knowledge> = entries
            .values()
            .filter(|e| {
                if let Some(ref project) = filter.project {
                    if e.project() != project {
                        return false;
                    }
                }
                if let Some(ref ns) = filter.namespace {
                    if !e.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref kind) = filter.kind {
                    if e.kind() != *kind {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.tag {
                    if !e.tags().contains(tag) {
                        return false;
                    }
                }
                if let Some(ref prefix) = filter.path_prefix {
                    if !e.path().starts_with(prefix.as_str()) {
                        return false;
                    }
                }
                if let Some(ref agent_id) = filter.agent_id {
                    if e.agent_id().as_ref() != Some(agent_id) {
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
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let query_lower = query.to_lowercase();
        let mut results: Vec<Knowledge> = entries
            .values()
            .filter(|e| {
                if let Some(ns) = namespace {
                    if !e.namespace().starts_with(ns) {
                        return false;
                    }
                }
                e.title().to_lowercase().contains(&query_lower)
                    || e.content().to_lowercase().contains(&query_lower)
                    || e.path().to_lowercase().contains(&query_lower)
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.updated_at().cmp(&a.updated_at()));
        results.truncate(limit);
        Ok(results)
    }

    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        let mut entries = self
            .knowledge_entries
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        entries.remove(id);
        Ok(())
    }
}
