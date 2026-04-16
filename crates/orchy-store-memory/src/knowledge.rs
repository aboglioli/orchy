use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeStore};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

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
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(entries
            .values()
            .find(|e| {
                e.org_id() == org
                    && e.project() == project
                    && e.namespace() == namespace
                    && e.path() == path
            })
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
                if let Some(ref org_id) = filter.org_id {
                    if e.org_id() != org_id {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    let project_matches = e.project() == Some(project);
                    let org_level = e.project().is_none();
                    if !(project_matches || filter.include_org_level && org_level) {
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
                    if e.agent_id().map(|id| id.as_str()) != Some(agent_id.as_str()) {
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
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        let entries = self
            .knowledge_entries
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let query_lower = query.to_lowercase();
        let mut scored: Vec<(f32, &Knowledge)> = entries
            .values()
            .filter(|e| {
                if e.org_id() != org {
                    return false;
                }
                if let Some(ns) = namespace {
                    if !e.namespace().starts_with(ns) {
                        return false;
                    }
                }
                true
            })
            .filter_map(|e| {
                if let (Some(qe), Some(ee)) = (embedding, e.embedding()) {
                    let score = crate::cosine_similarity(qe, ee);
                    if score > 0.0 {
                        return Some((score, e));
                    }
                }
                let text = format!("{} {} {}", e.title(), e.content(), e.path()).to_lowercase();
                if text.contains(&query_lower) {
                    return Some((0.5, e));
                }
                None
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored.into_iter().map(|(_, e)| e.clone()).collect())
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
