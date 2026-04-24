pub mod agent;
pub mod api_key;
pub mod embeddings;
pub mod error;
pub mod graph;
pub mod knowledge;
pub mod message;
pub mod namespace;
pub mod organization;
pub mod pagination;
pub mod project;
pub mod resource_lock;
pub mod resource_ref;
pub mod task;
pub mod user;

pub use graph::{
    Edge, EdgeId, EdgeStore, EntityNeighborhood, LinkParam, PeerEntity, Relation,
    RelationDirection, RelationOptions, RelationQuery, RelationType, RestoreEdge,
    TraversalDirection, TraversalHop,
};

#[cfg(test)]
mod graph_tests {
    use super::*;

    #[test]
    fn graph_module_exports_edge() {
        let _ = std::any::type_name::<Edge>();
    }

    #[test]
    fn graph_module_exports_store_trait() {
        fn _check_edge_store(_: &dyn EdgeStore) {}
    }

    #[test]
    fn graph_module_exports_relation_types() {
        let _ = RelationType::DependsOn;
        let _ = RelationType::Spawns;
        let _ = RelationType::Produces;
    }
}
