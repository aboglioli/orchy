use std::collections::{HashMap, VecDeque};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::edge::{Edge, EdgeId, EdgeStore, RelationType, TraversalDirection, TraversalEdge};
use orchy_core::error::Result;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};
use orchy_core::resource_ref::ResourceKind;

use crate::MemoryBackend;

#[async_trait]
impl EdgeStore for MemoryBackend {
    async fn save(&self, edge: &Edge) -> Result<()> {
        let mut store = self.edges.write().await;
        store.insert(edge.id(), edge.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let store = self.edges.read().await;
        Ok(store.get(id).cloned())
    }

    async fn delete(&self, id: &EdgeId) -> Result<()> {
        let mut store = self.edges.write().await;
        store.remove(id);
        Ok(())
    }

    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let store = self.edges.read().await;
        let mut edges: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && e.from_kind() == kind
                    && e.from_id() == id
                    && rel_type.is_none_or(|rt| e.rel_type() == rt)
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        !only_active || e.is_active()
                    }
            })
            .cloned()
            .collect();
        edges.sort_by_key(|e| e.created_at());
        Ok(edges)
    }

    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let store = self.edges.read().await;
        let mut edges: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && e.to_kind() == kind
                    && e.to_id() == id
                    && rel_type.is_none_or(|rt| e.rel_type() == rt)
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        !only_active || e.is_active()
                    }
            })
            .cloned()
            .collect();
        edges.sort_by_key(|e| e.created_at());
        Ok(edges)
    }

    async fn exists_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<bool> {
        let store = self.edges.read().await;
        Ok(store.values().any(|e| {
            e.org_id() == org
                && e.from_kind() == from_kind
                && e.from_id() == from_id
                && e.to_kind() == to_kind
                && e.to_id() == to_id
                && e.rel_type() == rel_type
        }))
    }

    async fn list_by_org(
        &self,
        org: &OrganizationId,
        rel_type: Option<&RelationType>,
        page: PageParams,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Page<Edge>> {
        let store = self.edges.read().await;
        let mut edges: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && rel_type.is_none_or(|rt| e.rel_type() == rt)
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        !only_active || e.is_active()
                    }
            })
            .cloned()
            .collect();
        edges.sort_by_key(|e| e.created_at());
        Ok(crate::apply_cursor_pagination(edges, &page, |e| {
            e.id().to_string()
        }))
    }

    async fn traverse(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        max_depth: u32,
        rel_types: Option<&[RelationType]>,
        direction: TraversalDirection,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<TraversalEdge>> {
        let store = self.edges.read().await;
        let all_edges: Vec<&Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        !only_active || e.is_active()
                    }
            })
            .collect();

        // BFS from the starting node. Track visited edge IDs to avoid duplicates, keeping minimum depth.
        let mut result: HashMap<EdgeId, TraversalEdge> = HashMap::new();
        // Queue entries: (current_kind, current_id, depth)
        let mut queue: VecDeque<(ResourceKind, String, u32)> = VecDeque::new();
        queue.push_back((kind.clone(), id.to_string(), 0));

        while let Some((cur_kind, cur_id, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let next_depth = depth + 1;

            for edge in &all_edges {
                let rel_ok = rel_types.is_none_or(|rts| rts.contains(edge.rel_type()));
                if !rel_ok {
                    continue;
                }

                let (matches, next_kind, next_id) = match direction {
                    TraversalDirection::Outgoing => {
                        if edge.from_kind() == &cur_kind && edge.from_id() == cur_id {
                            (true, edge.to_kind().clone(), edge.to_id().to_string())
                        } else {
                            (false, cur_kind.clone(), cur_id.clone())
                        }
                    }
                    TraversalDirection::Incoming => {
                        if edge.to_kind() == &cur_kind && edge.to_id() == cur_id {
                            (true, edge.from_kind().clone(), edge.from_id().to_string())
                        } else {
                            (false, cur_kind.clone(), cur_id.clone())
                        }
                    }
                    TraversalDirection::Both => {
                        if edge.from_kind() == &cur_kind && edge.from_id() == cur_id {
                            (true, edge.to_kind().clone(), edge.to_id().to_string())
                        } else if edge.to_kind() == &cur_kind && edge.to_id() == cur_id {
                            (true, edge.from_kind().clone(), edge.from_id().to_string())
                        } else {
                            (false, cur_kind.clone(), cur_id.clone())
                        }
                    }
                };

                if !matches {
                    continue;
                }

                let te = TraversalEdge {
                    id: edge.id(),
                    from_kind: edge.from_kind().clone(),
                    from_id: edge.from_id().to_string(),
                    to_kind: edge.to_kind().clone(),
                    to_id: edge.to_id().to_string(),
                    rel_type: *edge.rel_type(),
                    display: edge.display().map(String::from),
                    depth: next_depth,
                };

                result.entry(edge.id()).or_insert_with(|| {
                    queue.push_back((next_kind, next_id, next_depth));
                    te
                });
            }
        }

        let mut traversal: Vec<TraversalEdge> = result.into_values().collect();
        traversal.sort_by_key(|a| a.depth);

        Ok(traversal)
    }

    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()> {
        let mut store = self.edges.write().await;
        store.retain(|_, e| {
            !(e.org_id() == org
                && ((e.from_kind() == kind && e.from_id() == id)
                    || (e.to_kind() == kind && e.to_id() == id)))
        });
        Ok(())
    }

    async fn delete_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<()> {
        let mut store = self.edges.write().await;
        store.retain(|_, e| {
            !(e.org_id() == org
                && e.from_kind() == from_kind
                && e.from_id() == from_id
                && e.to_kind() == to_kind
                && e.to_id() == to_id
                && e.rel_type() == rel_type)
        });
        Ok(())
    }
}
