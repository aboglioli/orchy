mod relation;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use relation::{Arity, Relation};

use crate::entity_ref::EntityRef;
use crate::error::{DomainError, Result};

#[async_trait]
pub trait EdgeStore: Send + Sync {
    async fn add(&self, edge: &Edge) -> Result<()>;
    async fn remove(&self, edge: &Edge) -> Result<()>;
    async fn out(&self, from: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>>;
    async fn incoming(&self, to: &EntityRef, relation: Option<&Relation>) -> Result<Vec<Edge>>;
    async fn neighbourhood(&self, of: &EntityRef, depth: u8) -> Result<Vec<TraversalHop>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    from: EntityRef,
    to: EntityRef,
    relation: Relation,
}

impl Edge {
    /// Holding an `Edge` is proof the link is legal: nothing above has to re-check it, and
    /// nothing above can forget to.
    pub fn new(from: EntityRef, to: EntityRef, relation: Relation) -> Result<Self> {
        if from == to {
            return Err(DomainError::validation(format!(
                "`{relation}` cannot point {from} at itself"
            )));
        }
        relation.validate(from.kind(), to.kind())?;
        Ok(Self { from, to, relation })
    }

    pub fn from(&self) -> &EntityRef {
        &self.from
    }

    pub fn to(&self) -> &EntityRef {
        &self.to
    }

    pub fn relation(&self) -> &Relation {
        &self.relation
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraversalHop {
    pub edge: Edge,
    pub depth: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Out,
    In,
    #[default]
    Both,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_ref::EntityKind;
    use crate::id::Id;

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    fn doc(id: &str) -> EntityRef {
        EntityRef::document(Id::new(id).unwrap())
    }

    fn task(id: &str) -> EntityRef {
        EntityRef::task(Id::new(id).unwrap())
    }

    fn message(id: &str) -> EntityRef {
        EntityRef::message(Id::new(id).unwrap())
    }

    #[test]
    fn an_edge_the_relation_forbids_cannot_be_built() {
        assert!(
            Edge::new(doc(A), task(B), Relation::Parent).is_err(),
            "a document is not a subtask, so no Edge should exist saying it is"
        );
    }

    #[test]
    fn a_permitted_edge_is_built() {
        assert!(Edge::new(task(A), task(B), Relation::Parent).is_ok());
        assert!(Edge::new(doc(A), message(B), Relation::DerivedFrom).is_ok());
    }

    #[test]
    fn nothing_links_to_itself() {
        let err = Edge::new(doc(A), doc(A), Relation::RelatedTo).unwrap_err();
        assert!(err.to_string().contains("itself"), "{err}");
        assert!(
            Edge::new(task(A), task(A), Relation::Parent).is_err(),
            "not even the relations that join like with like"
        );
    }

    #[test]
    fn an_edge_is_identified_by_all_three_parts() {
        let a = Edge::new(doc(A), doc(B), Relation::Supersedes).unwrap();
        let b = Edge::new(doc(A), doc(B), Relation::RelatedTo).unwrap();
        assert_ne!(a, b, "the same endpoints under a different relation differ");
    }

    #[test]
    fn the_endpoints_survive_construction_unchanged() {
        let edge = Edge::new(task(A), message(B), Relation::SpawnedBy).unwrap();
        assert_eq!(edge.from().kind(), EntityKind::Task);
        assert_eq!(edge.to().kind(), EntityKind::Message);
        assert_eq!(edge.relation(), &Relation::SpawnedBy);
    }
}
