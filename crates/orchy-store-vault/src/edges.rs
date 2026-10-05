use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    Clock, DomainError, Edge, EdgeAdded, EdgeRemoved, EdgeStore, EntityKind, EntityRef, EventLog,
    Kind, Relation, Result, TraversalHop,
};

use crate::markdown::MarkdownFile;
use crate::transaction::atomically;
use crate::vault::{Amended, Vault, refs_in};

pub struct VaultEdgeStore {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
    clock: Arc<dyn Clock>,
}

impl VaultEdgeStore {
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>, clock: Arc<dyn Clock>) -> Self {
        Self { vault, log, clock }
    }

    async fn amend(
        &self,
        entity: &EntityRef,
        field: &str,
        edit: impl Fn(&mut Vec<String>) + Send + Sync + 'static,
    ) -> Result<Amended> {
        let Some(id) = entity.id() else {
            return Ok(Amended::Missing);
        };
        self.vault.amend_refs(id, field, edit).await
    }

    /// The target's file shows who points at it for the relations whose inverse orchy
    /// projects, so `cat` answers "what replaced this"; a missing target has nothing to show.
    async fn render_inverse(&self, edge: &Edge, present: bool) -> Result<()> {
        let inverse = edge.relation().inverse();
        if !Kind::is_projected_field(inverse) {
            return Ok(());
        }
        let source = edge.from().to_string();
        self.amend(edge.to(), inverse, move |refs| {
            refs.retain(|r| r != &source);
            if present {
                refs.push(source.clone());
            }
        })
        .await
        .map(drop)
    }

    async fn add_now(&self, edge: &Edge) -> Result<()> {
        let target = reference(edge);
        let amended = self
            .amend(edge.from(), edge.relation().as_str(), move |targets| {
                if !targets.iter().any(|t| same_entity(t, &target)) {
                    targets.push(target.clone());
                }
            })
            .await?;
        match amended {
            Amended::Missing => Err(DomainError::not_found(
                kind_name(edge.from().kind()),
                edge.source(),
            )),
            Amended::Unchanged => Ok(()),
            Amended::Changed => {
                self.render_inverse(edge, true).await?;
                self.log
                    .append(&[Box::new(EdgeAdded::of(edge, self.clock.now()))])
                    .await
            }
        }
    }

    async fn remove_now(&self, edge: &Edge) -> Result<()> {
        let target = reference(edge);
        let amended = self
            .amend(edge.from(), edge.relation().as_str(), move |targets| {
                targets.retain(|t| !same_entity(t, &target));
            })
            .await?;
        if amended != Amended::Changed {
            return Ok(());
        }
        self.render_inverse(edge, false).await?;
        self.log
            .append(&[Box::new(EdgeRemoved::of(edge, self.clock.now()))])
            .await
    }

    /// Reads without refreshing what the vault remembers of the file, so looking at links
    /// never weakens the compare-and-swap of a save that loaded the entity earlier.
    async fn edges_from(&self, entity: &EntityRef) -> Result<Vec<Edge>> {
        let Some(id) = entity.id() else {
            return Ok(Vec::new());
        };
        let Some((_, file)) = self.vault.peek_by_id(id).await? else {
            return Ok(Vec::new());
        };
        Ok(edges_in(entity, &file))
    }

    async fn all_edges(&self) -> Result<Vec<Edge>> {
        let mut edges = Vec::new();
        for (id, located, file) in self.vault.scan().await?.entries {
            edges.extend(edges_in(&EntityRef::new(located.kind, id), &file));
        }
        Ok(edges)
    }
}

pub(crate) fn edges_in(entity: &EntityRef, file: &MarkdownFile) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (field, value) in file.frontmatter.iter() {
        let Ok(relation) = field.parse::<Relation>() else {
            continue;
        };
        for target in refs_in(value) {
            let assumed = relation.sole_target_kind();
            let Ok(to) = EntityRef::parse_or_assume(&target, assumed) else {
                continue;
            };
            let Ok(edge) = Edge::new(entity.clone(), to, relation) else {
                continue;
            };
            edges.push(edge);
        }
    }
    edges
}

fn reference(edge: &Edge) -> String {
    edge.to().to_string()
}

fn same_entity(a: &str, b: &str) -> bool {
    let id_of = |s: &str| s.rsplit(':').next().unwrap_or(s).to_owned();
    id_of(a) == id_of(b)
}

fn kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Document => "document",
        EntityKind::Task => "task",
        EntityKind::Message => "message",
        EntityKind::Skill => "skill",
        EntityKind::Actor => "actor",
    }
}

#[async_trait]
impl EdgeStore for VaultEdgeStore {
    async fn add(&self, edge: &Edge) -> Result<()> {
        atomically(
            &self.vault,
            Some(self.log.as_ref()),
            Box::pin(self.add_now(edge)),
        )
        .await
    }

    async fn remove(&self, edge: &Edge) -> Result<()> {
        atomically(
            &self.vault,
            Some(self.log.as_ref()),
            Box::pin(self.remove_now(edge)),
        )
        .await
    }
    async fn out(&self, from: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>> {
        Ok(self
            .edges_from(from)
            .await?
            .into_iter()
            .filter(|e| relation.is_none_or(|r| e.relation() == r))
            .collect())
    }

    async fn of_relation(&self, relation: &Relation) -> Result<Vec<Edge>> {
        Ok(self
            .all_edges()
            .await?
            .into_iter()
            .filter(|e| e.relation() == relation)
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

    fn store(vault: &Arc<Vault>) -> VaultEdgeStore {
        VaultEdgeStore::new(
            Arc::clone(vault),
            Arc::new(orchy_store_memory::MemoryEventLog::new()),
            Arc::new(orchy_store_memory::FixedClock::at(1_700_000_000)),
        )
    }

    const SKILL: &str = "01DX5ZZKBKACTAV9WEVGEMMVRZ";

    #[tokio::test]
    async fn an_edge_from_any_kind_is_seen_from_both_ends() {
        let vault = vault_with(&[
            ("tasks/open/t.md", TASK, "task"),
            ("messages/m/m.md", MESSAGE, "message"),
            ("docs/d.md", DOC, "note"),
            ("skills/s.md", SKILL, "skill"),
        ])
        .await;
        let store = store(&vault);
        let target = EntityRef::document(Id::new(DOC).unwrap());

        for (kind, id) in [
            (EntityKind::Skill, SKILL),
            (EntityKind::Task, TASK),
            (EntityKind::Message, MESSAGE),
        ] {
            let from = EntityRef::new(kind, Id::new(id).unwrap());
            let edge = Edge::new(from.clone(), target.clone(), Relation::RelatedTo).unwrap();
            store.add(&edge).await.unwrap();

            assert_eq!(store.out(&from, None).await.unwrap(), vec![edge.clone()]);
            assert!(
                store.incoming(&target, None).await.unwrap().contains(&edge),
                "{kind:?} edge invisible from its target"
            );
            assert!(
                store
                    .neighbourhood(&from, 1)
                    .await
                    .unwrap()
                    .iter()
                    .any(|hop| hop.edge == edge),
                "{kind:?} edge missing from its neighbourhood"
            );
        }
    }

    #[tokio::test]
    async fn an_ambiguous_relation_records_the_kind_in_the_file() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("docs/b.md", DOC, "decision"),
        ])
        .await;
        let store = store(&vault);

        store
            .add(
                &Edge::new(
                    EntityRef::task(Id::new(TASK).unwrap()),
                    EntityRef::document(Id::new(DOC).unwrap()),
                    Relation::RelatedTo,
                )
                .unwrap(),
            )
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
    async fn an_unambiguous_relation_still_names_its_target_kind() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("messages/m/b.md", MESSAGE, "message"),
        ])
        .await;
        let store = store(&vault);

        store
            .add(
                &Edge::new(
                    EntityRef::task(Id::new(TASK).unwrap()),
                    EntityRef::message(Id::new(MESSAGE).unwrap()),
                    Relation::SpawnedBy,
                )
                .unwrap(),
            )
            .await
            .unwrap();

        let (_, file) = vault
            .read_by_id(&Id::new(TASK).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            file.frontmatter.strings("spawned_by"),
            vec![format!("message:{MESSAGE}")],
            "every reference names the kind it points at, unambiguous relation or not"
        );
    }

    #[tokio::test]
    async fn a_deleted_target_keeps_its_type() {
        let vault = vault_with(&[
            ("tasks/open/a.md", TASK, "task"),
            ("docs/b.md", DOC, "decision"),
        ])
        .await;
        let store = store(&vault);
        let from = EntityRef::task(Id::new(TASK).unwrap());

        store
            .add(
                &Edge::new(
                    from.clone(),
                    EntityRef::document(Id::new(DOC).unwrap()),
                    Relation::RelatedTo,
                )
                .unwrap(),
            )
            .await
            .unwrap();

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
        let store = store(&vault);
        let from = EntityRef::task(Id::new(TASK).unwrap());

        store
            .add(
                &Edge::new(
                    from.clone(),
                    EntityRef::message(Id::new(MESSAGE).unwrap()),
                    Relation::SpawnedBy,
                )
                .unwrap(),
            )
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

        let store = store(&vault);
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
        let store = store(&vault);
        let edge = Edge::new(
            EntityRef::task(Id::new(TASK).unwrap()),
            EntityRef::document(Id::new(DOC).unwrap()),
            Relation::RelatedTo,
        )
        .unwrap();

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
