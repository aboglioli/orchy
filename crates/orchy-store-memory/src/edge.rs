use std::collections::{HashSet, VecDeque};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::error::Result;
use orchy_core::graph::{
    Edge, EdgeId, EdgeStore, RelationDirection, RelationType, TraversalDirection, TraversalHop,
};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};
use orchy_core::resource_ref::{ResourceKind, ResourceRef};

use crate::MemoryBackend;

#[async_trait]
impl EdgeStore for MemoryBackend {
    async fn save(&self, edge: &mut Edge) -> Result<()> {
        {
            let mut store = self.edges.write().await;
            let mut by_from = self.edges_by_from.write().await;
            let mut by_to = self.edges_by_to.write().await;

            if let Some(old) = store.get(&edge.id()) {
                let from_key = (old.from_kind().clone(), old.from_id().to_string());
                if let Some(ids) = by_from.get_mut(&from_key) {
                    ids.retain(|id| id != &old.id());
                }
                let to_key = (old.to_kind().clone(), old.to_id().to_string());
                if let Some(ids) = by_to.get_mut(&to_key) {
                    ids.retain(|id| id != &old.id());
                }
            }

            let from_key = (edge.from_kind().clone(), edge.from_id().to_string());
            by_from.entry(from_key).or_default().push(edge.id());

            let to_key = (edge.to_kind().clone(), edge.to_id().to_string());
            by_to.entry(to_key).or_default().push(edge.id());

            store.insert(edge.id(), edge.clone());
        }

        let events = edge.drain_events();
        if !events.is_empty() {
            if let Err(e) = orchy_events::io::Writer::write_all(self, &events).await {
                tracing::error!("failed to persist events: {e}");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        let store = self.edges.read().await;
        Ok(store.get(id).cloned())
    }

    async fn delete(&self, id: &EdgeId) -> Result<()> {
        let mut store = self.edges.write().await;
        let mut by_from = self.edges_by_from.write().await;
        let mut by_to = self.edges_by_to.write().await;

        if let Some(old) = store.remove(id) {
            let from_key = (old.from_kind().clone(), old.from_id().to_string());
            if let Some(ids) = by_from.get_mut(&from_key) {
                ids.retain(|eid| eid != id);
            }
            let to_key = (old.to_kind().clone(), old.to_id().to_string());
            if let Some(ids) = by_to.get_mut(&to_key) {
                ids.retain(|eid| eid != id);
            }
        }
        Ok(())
    }

    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let store = self.edges.read().await;
        let mut edges: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && e.from_kind() == kind
                    && e.from_id() == id
                    && (rel_types.is_empty() || rel_types.contains(e.rel_type()))
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        e.is_active()
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
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        let store = self.edges.read().await;
        let mut edges: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && e.to_kind() == kind
                    && e.to_id() == id
                    && (rel_types.is_empty() || rel_types.contains(e.rel_type()))
                    && if let Some(ts) = as_of {
                        e.is_active_at(ts)
                    } else {
                        e.is_active()
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
                && e.is_active()
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

    async fn find_neighbors(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        target_kinds: &[ResourceKind],
        direction: TraversalDirection,
        max_depth: u32,
        as_of: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TraversalHop>> {
        let store = self.edges.read().await;
        let by_from = self.edges_by_from.read().await;
        let by_to = self.edges_by_to.read().await;

        let max_depth = max_depth.max(1);
        let mut result: Vec<TraversalHop> = vec![];
        let mut visited_edges: HashSet<EdgeId> = HashSet::new();
        let mut queue: VecDeque<(ResourceKind, String, u32, Option<ResourceRef>)> = VecDeque::new();
        queue.push_back((kind.clone(), id.to_string(), 0, None));

        'outer: while let Some((cur_kind, cur_id, depth, via)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            let candidate_ids: Vec<EdgeId> = match direction {
                TraversalDirection::Outgoing => by_from
                    .get(&(cur_kind.clone(), cur_id.clone()))
                    .cloned()
                    .unwrap_or_default(),
                TraversalDirection::Incoming => by_to
                    .get(&(cur_kind.clone(), cur_id.clone()))
                    .cloned()
                    .unwrap_or_default(),
                TraversalDirection::Both => {
                    let mut ids = by_from
                        .get(&(cur_kind.clone(), cur_id.clone()))
                        .cloned()
                        .unwrap_or_default();
                    ids.extend(
                        by_to
                            .get(&(cur_kind.clone(), cur_id.clone()))
                            .cloned()
                            .unwrap_or_default(),
                    );
                    ids
                }
            };

            for edge_id in candidate_ids {
                if result.len() >= limit as usize {
                    break 'outer;
                }
                if visited_edges.contains(&edge_id) {
                    continue;
                }

                let Some(edge) = store.get(&edge_id) else {
                    continue;
                };

                if edge.org_id() != org {
                    continue;
                }

                let is_active = if let Some(ts) = as_of {
                    edge.is_active_at(ts)
                } else {
                    edge.is_active()
                };
                if !is_active {
                    continue;
                }

                if !rel_types.is_empty() && !rel_types.contains(edge.rel_type()) {
                    continue;
                }

                let hop_direction_and_peer = match direction {
                    TraversalDirection::Outgoing => Some((
                        RelationDirection::Outgoing,
                        edge.to_kind().clone(),
                        edge.to_id().to_string(),
                    )),
                    TraversalDirection::Incoming => Some((
                        RelationDirection::Incoming,
                        edge.from_kind().clone(),
                        edge.from_id().to_string(),
                    )),
                    TraversalDirection::Both => {
                        if edge.from_kind() == &cur_kind && edge.from_id() == cur_id {
                            Some((
                                RelationDirection::Outgoing,
                                edge.to_kind().clone(),
                                edge.to_id().to_string(),
                            ))
                        } else {
                            Some((
                                RelationDirection::Incoming,
                                edge.from_kind().clone(),
                                edge.from_id().to_string(),
                            ))
                        }
                    }
                };

                let Some((hop_direction, peer_kind, peer_id)) = hop_direction_and_peer else {
                    continue;
                };

                if !target_kinds.is_empty() && !target_kinds.contains(&peer_kind) {
                    continue;
                }

                visited_edges.insert(edge_id);
                let next_via = Some(ResourceRef::new(peer_kind.clone(), peer_id.clone()));
                queue.push_back((peer_kind, peer_id, depth + 1, next_via));
                result.push(TraversalHop {
                    edge: edge.clone(),
                    depth: depth + 1,
                    direction: hop_direction,
                    via: via.clone(),
                });
            }
        }
        Ok(result)
    }

    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()> {
        let mut store = self.edges.write().await;
        let mut by_from = self.edges_by_from.write().await;
        let mut by_to = self.edges_by_to.write().await;

        let to_remove: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && ((e.from_kind() == kind && e.from_id() == id)
                        || (e.to_kind() == kind && e.to_id() == id))
            })
            .cloned()
            .collect();

        for old in &to_remove {
            store.remove(&old.id());
            let from_key = (old.from_kind().clone(), old.from_id().to_string());
            if let Some(ids) = by_from.get_mut(&from_key) {
                ids.retain(|eid| eid != &old.id());
            }
            let to_key = (old.to_kind().clone(), old.to_id().to_string());
            if let Some(ids) = by_to.get_mut(&to_key) {
                ids.retain(|eid| eid != &old.id());
            }
        }

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
        let mut by_from = self.edges_by_from.write().await;
        let mut by_to = self.edges_by_to.write().await;

        let to_remove: Vec<Edge> = store
            .values()
            .filter(|e| {
                e.org_id() == org
                    && e.from_kind() == from_kind
                    && e.from_id() == from_id
                    && e.to_kind() == to_kind
                    && e.to_id() == to_id
                    && e.rel_type() == rel_type
            })
            .cloned()
            .collect();

        for old in &to_remove {
            store.remove(&old.id());
            let fk = (old.from_kind().clone(), old.from_id().to_string());
            if let Some(ids) = by_from.get_mut(&fk) {
                ids.retain(|eid| eid != &old.id());
            }
            let tk = (old.to_kind().clone(), old.to_id().to_string());
            if let Some(ids) = by_to.get_mut(&tk) {
                ids.retain(|eid| eid != &old.id());
            }
        }

        Ok(())
    }
}
