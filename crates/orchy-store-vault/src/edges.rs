use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    DomainError, Edge, EdgeStore, EntityKind, EntityRef, Id, RelationRegistry, RelationType,
    Result, TraversalHop,
};
use serde_json::Value;

use crate::vault::Vault;

/// Only the forward direction is stored, in the source entity's own frontmatter, so adding a
/// link writes exactly one file. Inverses are derived at read time.
pub struct VaultEdgeStore {
    vault: Arc<Vault>,
    relations: Arc<dyn RelationRegistry>,
}

impl VaultEdgeStore {
    pub fn new(vault: Arc<Vault>, relations: Arc<dyn RelationRegistry>) -> Self {
        Self { vault, relations }
    }

    async fn edges_from(&self, entity: &EntityRef) -> Result<Vec<Edge>> {
        let Some((_, file)) = self.vault.read_by_id(entity.id()).await? else {
            return Ok(Vec::new());
        };
        let mut edges = Vec::new();
        for (field, value) in file.frontmatter.iter() {
            let Ok(relation) = RelationType::new(field) else {
                continue;
            };
            if self.relations.get(&relation).is_none() {
                continue;
            }
            for target in ids_in(value) {
                let Ok(id) = Id::new(&target) else {
                    continue;
                };
                let kind = self
                    .vault
                    .locate(&id)
                    .map_or(EntityKind::Document, |l| l.kind);
                edges.push(Edge::new(
                    entity.clone(),
                    EntityRef::new(kind, id),
                    relation.clone(),
                ));
            }
        }
        Ok(edges)
    }

    async fn all_edges(&self) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        for kind in [
            EntityKind::Document,
            EntityKind::Task,
            EntityKind::Message,
            EntityKind::Actor,
        ] {
            for id in self.vault.ids_of(kind) {
                edges.extend(self.edges_from(&EntityRef::new(kind, id)).await?);
            }
        }
        Ok(edges)
    }
}

fn ids_in(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Document => "document",
        EntityKind::Task => "task",
        EntityKind::Message => "message",
        EntityKind::Actor => "actor",
    }
}

#[async_trait]
impl EdgeStore for VaultEdgeStore {
    async fn add(&self, edge: &Edge) -> Result<()> {
        let id = edge.from().id();
        let Some((key, mut file)) = self.vault.read_by_id(id).await? else {
            return Err(DomainError::not_found(kind_name(edge.from().kind()), id));
        };
        let field = edge.relation().as_str().to_owned();
        let mut targets = ids_in(file.frontmatter.get(&field).unwrap_or(&Value::Null));
        let target = edge.to().id().to_string();
        if !targets.contains(&target) {
            targets.push(target);
            targets.sort();
        }
        file.frontmatter.set(
            field,
            Value::Array(targets.into_iter().map(Value::String).collect()),
        );
        self.vault.write(&key, &file, id, edge.from().kind()).await
    }

    async fn remove(&self, edge: &Edge) -> Result<()> {
        let id = edge.from().id();
        let Some((key, mut file)) = self.vault.read_by_id(id).await? else {
            return Ok(());
        };
        let field = edge.relation().as_str().to_owned();
        let target = edge.to().id().to_string();
        let mut targets = ids_in(file.frontmatter.get(&field).unwrap_or(&Value::Null));
        targets.retain(|t| t != &target);

        if targets.is_empty() {
            file.frontmatter.remove(&field);
        } else {
            file.frontmatter.set(
                field,
                Value::Array(targets.into_iter().map(Value::String).collect()),
            );
        }
        self.vault.write(&key, &file, id, edge.from().kind()).await
    }

    async fn out(&self, from: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>> {
        Ok(self
            .edges_from(from)
            .await?
            .into_iter()
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn incoming(&self, to: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>> {
        Ok(self
            .all_edges()
            .await?
            .into_iter()
            .filter(|e| e.to() == to)
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn neighbourhood(&self, of: &EntityRef, depth: u8) -> Result<Vec<TraversalHop>> {
        let edges = self.all_edges().await?;
        let mut seen = vec![of.to_string()];
        let mut frontier = vec![of.clone()];
        let mut hops: Vec<TraversalHop> = Vec::new();

        for level in 1..=depth {
            let mut next = Vec::new();
            for node in &frontier {
                for edge in edges.iter().filter(|e| e.from() == node || e.to() == node) {
                    if hops.iter().any(|h| &h.edge == edge) {
                        continue;
                    }
                    hops.push(TraversalHop {
                        edge: edge.clone(),
                        depth: level,
                    });
                    let other = if edge.from() == node {
                        edge.to()
                    } else {
                        edge.from()
                    };
                    if !seen.contains(&other.to_string()) {
                        seen.push(other.to_string());
                        next.push(other.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        Ok(hops)
    }
}
