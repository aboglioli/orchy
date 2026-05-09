use chrono::Utc;
use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::graph::RelationType;
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgePath, KnowledgeStore,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};
use orchy_core::resource_ref::ResourceKind;

use crate::MemoryState;

pub struct MemoryKnowledgeStore {
    state: Arc<MemoryState>,
}

impl MemoryKnowledgeStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl KnowledgeStore for MemoryKnowledgeStore {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        if let Some(pv) = entry.persisted_version() {
            if let Some(existing) = self.state.knowledge_entries.get(&entry.id()) {
                if existing.version() != pv {
                    return Err(Error::version_mismatch(
                        pv.as_u64(),
                        existing.version().as_u64(),
                    ));
                }
            }
        }

        entry.mark_persisted();
        self.state
            .knowledge_entries
            .insert(entry.id(), entry.clone());

        let events = entry.drain_events();
        self.state.append_events(events).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        Ok(self.state.knowledge_entries.get(id).map(|r| r.clone()))
    }

    async fn find_by_path(
        &self,
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &KnowledgePath,
    ) -> Result<Option<Knowledge>> {
        Ok(self.state.knowledge_entries.iter().find_map(|entry| {
            let e = entry.value();
            let project_match = project.is_none_or(|p| e.project() == Some(p));
            if e.org_id() == org
                && project_match
                && e.namespace() == namespace
                && e.path().as_str() == path.as_str()
            {
                Some(e.clone())
            } else {
                None
            }
        }))
    }

    async fn list(&self, filter: KnowledgeFilter, page: PageParams) -> Result<Page<Knowledge>> {
        let results: Vec<Knowledge> = self
            .state
            .knowledge_entries
            .iter()
            .filter(|entry| {
                let e = entry.value();
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
                    if !e.path().as_str().starts_with(prefix.as_str()) {
                        return false;
                    }
                }
                if !filter.include_expired.unwrap_or(false) {
                    if let Some(until) = e.valid_until() {
                        if until < Utc::now() {
                            return false;
                        }
                    }
                }
                if !filter.include_archived.unwrap_or(false) && e.is_archived() {
                    return false;
                }
                true
            })
            .map(|e| e.value().clone())
            .collect();

        let results = if let Some(orphaned) = filter.orphaned {
            results
                .into_iter()
                .filter(|entry| {
                    let id_str = entry.id().to_string();
                    let has_link = self.state.edges.iter().any(|edge_entry| {
                        let e = edge_entry.value();
                        e.to_kind() == &ResourceKind::Knowledge
                            && e.to_id() == id_str
                            && matches!(
                                e.rel_type(),
                                RelationType::Produces | RelationType::OwnedBy
                            )
                            && e.is_active()
                    });
                    if orphaned { !has_link } else { has_link }
                })
                .collect()
        } else {
            results
        };

        Ok(crate::apply_cursor_pagination(results, &page, |e| {
            e.id().to_string()
        }))
    }

    async fn search(
        &self,
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<(Knowledge, Option<f32>)>> {
        let query_lower = query.to_lowercase();
        let mut scored: Vec<(f32, Knowledge)> = self
            .state
            .knowledge_entries
            .iter()
            .filter(|entry| {
                let e = entry.value();
                if e.org_id() != org {
                    return false;
                }
                if let Some(ns) = namespace {
                    if !e.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if e.is_archived() {
                    return false;
                }
                true
            })
            .filter_map(|entry| {
                let e = entry.value();
                if let (Some(qe), Some(ee)) = (embedding, e.embedding()) {
                    let score = crate::cosine_similarity(qe, ee);
                    if score > 0.0 {
                        return Some((score, e.clone()));
                    }
                }
                let text = format!("{} {} {}", e.title(), e.content(), e.path()).to_lowercase();
                if text.contains(&query_lower) {
                    return Some((0.5, e.clone()));
                }
                None
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored
            .into_iter()
            .map(|(score, e)| (e, Some(score)))
            .collect())
    }

    async fn find_by_ids(&self, ids: &[KnowledgeId]) -> Result<Vec<Knowledge>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.state.knowledge_entries.get(id).map(|r| r.clone()))
            .collect())
    }

    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        self.state.knowledge_entries.remove(id);
        Ok(())
    }
}
