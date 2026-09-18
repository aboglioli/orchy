mod registry;
mod relation;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use registry::{RelationRegistry, StaticRelationRegistry};
pub use relation::{Arity, RelationDefinition, RelationType};

use crate::entity_ref::EntityRef;
use crate::error::Result;

#[async_trait]
pub trait EdgeStore: Send + Sync {
    async fn add(&self, edge: &Edge) -> Result<()>;
    async fn remove(&self, edge: &Edge) -> Result<()>;
    async fn out(&self, from: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>>;
    async fn incoming(&self, to: &EntityRef, relation: Option<&RelationType>) -> Result<Vec<Edge>>;
    async fn neighbourhood(&self, of: &EntityRef, depth: u8) -> Result<Vec<TraversalHop>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Edge {
    from: EntityRef,
    to: EntityRef,
    relation: RelationType,
}

impl Edge {
    pub fn new(from: EntityRef, to: EntityRef, relation: RelationType) -> Self {
        Self { from, to, relation }
    }

    pub fn inverted(&self, inverse: RelationType) -> Self {
        Self {
            from: self.to.clone(),
            to: self.from.clone(),
            relation: inverse,
        }
    }

    pub fn from(&self) -> &EntityRef {
        &self.from
    }

    pub fn to(&self) -> &EntityRef {
        &self.to
    }

    pub fn relation(&self) -> &RelationType {
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
    use crate::id::Id;

    fn doc(id: &str) -> EntityRef {
        EntityRef::document(Id::new(id).unwrap())
    }

    fn rel(s: &str) -> RelationType {
        RelationType::new(s).unwrap()
    }

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
    const B: &str = "01BX5ZZKBKACTAV9WEVGEMMVRZ";

    #[test]
    fn inverting_swaps_the_endpoints_and_the_name() {
        let edge = Edge::new(doc(A), doc(B), rel("supersedes"));
        let back = edge.inverted(rel("superseded_by"));
        assert_eq!(back.from(), edge.to());
        assert_eq!(back.to(), edge.from());
        assert_eq!(back.relation(), &rel("superseded_by"));
    }

    #[test]
    fn inverting_twice_returns_the_original() {
        let edge = Edge::new(doc(A), doc(B), rel("supersedes"));
        let round_trip = edge
            .inverted(rel("superseded_by"))
            .inverted(rel("supersedes"));
        assert_eq!(round_trip, edge);
    }

    #[test]
    fn an_edge_is_identified_by_all_three_parts() {
        let a = Edge::new(doc(A), doc(B), rel("supersedes"));
        let b = Edge::new(doc(A), doc(B), rel("related_to"));
        assert_ne!(a, b, "the same endpoints under a different relation differ");
    }
}
