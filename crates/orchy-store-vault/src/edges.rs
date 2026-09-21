use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    DomainError, Edge, EdgeStore, EntityKind, EntityRef, Relation, Result, TraversalHop,
};
use serde_json::Value;

use crate::vault::Vault;

/// Only the forward direction is stored, in the source entity's own frontmatter, so adding a
/// link writes exactly one file. Inverses are derived at read time.
pub struct VaultEdgeStore {
    vault: Arc<Vault>,
}

impl VaultEdgeStore {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }

    async fn edges_from(&self, entity: &EntityRef) -> Result<Vec<Edge>> {
        let Some((_, file)) = self.vault.read_by_id(entity.id()).await? else {
            return Ok(Vec::new());
        };
        let mut edges = Vec::new();
        for (field, value) in file.frontmatter.iter() {
            let Ok(relation) = field.parse::<Relation>() else {
                continue;
            };
            for target in refs_in(value) {
                // the text says the kind; the relation supplies it when the text does not.
                // Neither guesses, so a target that has been deleted keeps its type.
                let assumed = relation.sole_target_kind();
                let Ok(to) = EntityRef::parse_or_assume(&target, assumed) else {
                    continue;
                };
                edges.push(Edge::new(entity.clone(), to, relation));
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

/// How a reference is written: `kind:id` unless the relation leaves no doubt, in which case
/// the bare id reads better and means the same thing.
fn reference(edge: &Edge) -> String {
    match edge.relation().sole_target_kind() {
        Some(_) => edge.to().id().to_string(),
        None => edge.to().to_string(),
    }
}

/// Two references name the same entity when their ids match, whether or not both spell out
/// the kind.
fn same_entity(a: &str, b: &str) -> bool {
    let id_of = |s: &str| s.rsplit(':').next().unwrap_or(s).to_owned();
    id_of(a) == id_of(b)
}

fn refs_in(value: &Value) -> Vec<String> {
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
        let mut targets = refs_in(file.frontmatter.get(&field).unwrap_or(&Value::Null));
        let target = reference(edge);
        if !targets.iter().any(|t| same_entity(t, &target)) {
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
        let target = reference(edge);
        let mut targets = refs_in(file.frontmatter.get(&field).unwrap_or(&Value::Null));
        targets.retain(|t| !same_entity(t, &target));

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

    async fn out(&self, from: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>> {
        Ok(self
            .edges_from(from)
            .await?
            .into_iter()
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn incoming(&self, to: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::{BlobStore, MemoryBlobStore};
    use crate::markdown::MarkdownFile;
    use orchy_core::{Body, Frontmatter, Id};
    use serde_json::json;
    use std::sync::Arc;

    const TASK: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const MESSAGE: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
    const DOC: &str = "01CX5ZZKBKACTAV9WEVGEMMVRZ";

    async fn vault_with(entries: &[(&str, &str, &str)]) -> Arc<Vault> {
        let blobs = Arc::new(MemoryBlobStore::new());
        for (key, id, kind) in entries {
            let mut fm = Frontmatter::new();
            fm.set("id", json!(id));
            fm.set("type", json!(kind));
            let file = MarkdownFile {
                frontmatter: fm,
                body: Body::new("x"),
            };
            blobs
                .put(key, file.render().unwrap().as_bytes())
                .await
                .unwrap();
        }
        Arc::new(Vault::open(blobs as Arc<dyn BlobStore>).await.unwrap())
    }

    #[tokio::test]
    async fn an_ambiguous_relation_records_the_kind_in_the_file() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("docs/b.md", DOC, "decision"),
        ])
        .await;
        let store = VaultEdgeStore::new(Arc::clone(&vault));

        store
            .add(&Edge::new(
                EntityRef::task(Id::new(TASK).unwrap()),
                EntityRef::document(Id::new(DOC).unwrap()),
                Relation::RelatedTo,
            ))
            .await
            .unwrap();

        let (_, file) = vault
            .read_by_id(&Id::new(TASK).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            file.frontmatter.strings("related_to"),
            vec![format!("document:{DOC}")],
            "related_to can point anywhere, so the file must say where"
        );
    }

    #[tokio::test]
    async fn an_unambiguous_relation_stays_terse() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("messages/m/b.md", MESSAGE, "message"),
        ])
        .await;
        let store = VaultEdgeStore::new(Arc::clone(&vault));

        store
            .add(&Edge::new(
                EntityRef::task(Id::new(TASK).unwrap()),
                EntityRef::message(Id::new(MESSAGE).unwrap()),
                Relation::SpawnedBy,
            ))
            .await
            .unwrap();

        let (_, file) = vault
            .read_by_id(&Id::new(TASK).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            file.frontmatter.strings("spawned_by"),
            vec![MESSAGE.to_owned()],
            "spawned_by can only mean a message, so the prefix would be noise"
        );
    }

    #[tokio::test]
    async fn a_deleted_target_keeps_its_type() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("docs/b.md", DOC, "decision"),
        ])
        .await;
        let store = VaultEdgeStore::new(Arc::clone(&vault));
        let from = EntityRef::task(Id::new(TASK).unwrap());

        store
            .add(&Edge::new(
                from.clone(),
                EntityRef::document(Id::new(DOC).unwrap()),
                Relation::RelatedTo,
            ))
            .await
            .unwrap();

        // the target leaves the vault, as it would in a partial clone
        vault.remove(&Id::new(DOC).unwrap()).await.unwrap();

        let edges = store.out(&from, None).await.unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(
            edges[0].to().kind(),
            EntityKind::Document,
            "the kind came from the file, not from a lookup that would now fail"
        );
    }

    #[tokio::test]
    async fn a_missing_message_is_not_reported_as_a_document() {
        let vault = vault_with(&[("tasks/open/a.md", TASK, "task")]).await;
        let store = VaultEdgeStore::new(Arc::clone(&vault));
        let from = EntityRef::task(Id::new(TASK).unwrap());

        store
            .add(&Edge::new(
                from.clone(),
                EntityRef::message(Id::new(MESSAGE).unwrap()),
                Relation::SpawnedBy,
            ))
            .await
            .unwrap();

        let edges = store.out(&from, None).await.unwrap();
        assert_eq!(
            edges[0].to().kind(),
            EntityKind::Message,
            "spawned_by declares a message; a vanished target cannot turn it into a document"
        );
    }

    #[tokio::test]
    async fn a_hand_written_bare_id_is_still_understood() {
        let vault = vault_with(&[("tasks/open/a.md", TASK, "task")]).await;
        let (key, mut file) = vault
            .read_by_id(&Id::new(TASK).unwrap())
            .await
            .unwrap()
            .unwrap();
        file.frontmatter.set("related_to", json!([DOC]));
        vault
            .write(&key, &file, &Id::new(TASK).unwrap(), EntityKind::Task)
            .await
            .unwrap();

        let store = VaultEdgeStore::new(Arc::clone(&vault));
        let edges = store
            .out(&EntityRef::task(Id::new(TASK).unwrap()), None)
            .await
            .unwrap();
        assert!(
            edges.is_empty(),
            "related_to spans kinds, so an untyped id is skipped rather than guessed at"
        );
    }

    #[tokio::test]
    async fn removing_matches_whichever_spelling_the_file_uses() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("docs/b.md", DOC, "decision"),
        ])
        .await;
        let store = VaultEdgeStore::new(Arc::clone(&vault));
        let edge = Edge::new(
            EntityRef::task(Id::new(TASK).unwrap()),
            EntityRef::document(Id::new(DOC).unwrap()),
            Relation::RelatedTo,
        );

        store.add(&edge).await.unwrap();
        store.remove(&edge).await.unwrap();

        let (_, file) = vault
            .read_by_id(&Id::new(TASK).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(!file.frontmatter.contains("related_to"));
    }
}
