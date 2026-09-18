use std::collections::BTreeMap;

use super::relation::{Arity, RelationDefinition, RelationType};
use crate::entity_ref::EntityKind;
use crate::error::{DomainError, Result};

pub trait RelationRegistry: Send + Sync {
    fn get(&self, relation: &RelationType) -> Option<&RelationDefinition>;
    fn all(&self) -> Vec<(&RelationType, &RelationDefinition)>;

    fn inverse_of(&self, relation: &RelationType) -> Option<RelationType> {
        self.get(relation).map(|d| d.inverse.clone())
    }

    fn require(&self, relation: &RelationType) -> Result<&RelationDefinition> {
        self.get(relation)
            .ok_or_else(|| DomainError::UnknownRelation(relation.to_string()))
    }

    fn validate(&self, relation: &RelationType, from: EntityKind, to: EntityKind) -> Result<()> {
        let def = self.require(relation)?;
        if !def.accepts(from, to) {
            return Err(DomainError::validation(format!(
                "`{relation}` does not connect {from} to {to}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticRelationRegistry(BTreeMap<RelationType, RelationDefinition>);

impl StaticRelationRegistry {
    pub fn new(entries: BTreeMap<RelationType, RelationDefinition>) -> Self {
        Self(entries)
    }

    pub fn builtin() -> Self {
        use EntityKind::*;
        let mut entries = BTreeMap::new();

        let mut add = |name: &str,
                       inverse: &str,
                       from: Vec<EntityKind>,
                       to: Vec<EntityKind>,
                       symmetric: bool,
                       arity: Arity| {
            entries.insert(
                RelationType::new(name).expect("builtin relation name"),
                RelationDefinition {
                    inverse: RelationType::new(inverse).expect("builtin inverse name"),
                    from,
                    to,
                    symmetric,
                    arity,
                },
            );
        };

        add(
            "supersedes",
            "superseded_by",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "derived_from",
            "derives",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "summarizes",
            "summarized_by",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "invalidates",
            "invalidated_by",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "confirms",
            "confirmed_by",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "supported_by",
            "supports",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "contradicted_by",
            "contradicts",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "merged_from",
            "merged_into",
            vec![],
            vec![],
            false,
            Arity::Many,
        );
        add(
            "related_to",
            "related_to",
            vec![],
            vec![],
            true,
            Arity::Many,
        );

        add(
            "depends_on",
            "blocks",
            vec![Task],
            vec![Task],
            false,
            Arity::Many,
        );
        add(
            "parent",
            "subtasks",
            vec![Task],
            vec![Task],
            false,
            Arity::One,
        );
        add(
            "spawned_by",
            "spawns",
            vec![Task],
            vec![Message],
            false,
            Arity::One,
        );

        add(
            "produces",
            "produced_by",
            vec![Task],
            vec![Document],
            false,
            Arity::Many,
        );
        add(
            "implements",
            "implemented_by",
            vec![Task],
            vec![Document],
            false,
            Arity::Many,
        );

        add("owned_by", "owns", vec![], vec![Actor], false, Arity::Many);
        add(
            "reviewed_by",
            "reviewed",
            vec![],
            vec![Actor],
            false,
            Arity::Many,
        );

        Self(entries)
    }
}

impl RelationRegistry for StaticRelationRegistry {
    fn get(&self, relation: &RelationType) -> Option<&RelationDefinition> {
        self.0.get(relation)
    }

    fn all(&self) -> Vec<(&RelationType, &RelationDefinition)> {
        self.0.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(s: &str) -> RelationType {
        RelationType::new(s).unwrap()
    }

    fn registry() -> StaticRelationRegistry {
        StaticRelationRegistry::builtin()
    }

    #[test]
    fn the_builtin_registry_carries_all_sixteen_relations() {
        assert_eq!(registry().all().len(), 16);
    }

    #[test]
    fn every_inverse_is_itself_resolvable_or_symmetric() {
        let registry = registry();
        for (name, def) in registry.all() {
            if def.symmetric {
                assert_eq!(
                    &def.inverse, name,
                    "{name} is symmetric so it is its own inverse"
                );
            } else {
                assert_ne!(&def.inverse, name, "{name} must not be its own inverse");
            }
        }
    }

    #[test]
    fn the_task_hierarchy_is_stored_on_the_child_and_is_single_valued() {
        let registry = registry();
        let parent = registry.require(&rel("parent")).unwrap();
        assert_eq!(parent.arity, Arity::One, "a task has at most one parent");
        assert_eq!(parent.inverse, rel("subtasks"));
        assert_eq!(parent.from, vec![EntityKind::Task]);
    }

    #[test]
    fn an_unregistered_relation_is_rejected_by_name() {
        let err = registry().require(&rel("invented")).unwrap_err();
        assert!(matches!(err, DomainError::UnknownRelation(_)), "{err:?}");
    }

    #[test]
    fn validate_enforces_the_declared_endpoints() {
        let registry = registry();
        assert!(
            registry
                .validate(&rel("parent"), EntityKind::Task, EntityKind::Task)
                .is_ok()
        );
        assert!(
            registry
                .validate(&rel("parent"), EntityKind::Document, EntityKind::Task)
                .is_err(),
            "a document is not a subtask"
        );
        assert!(
            registry
                .validate(
                    &rel("related_to"),
                    EntityKind::Document,
                    EntityKind::Message
                )
                .is_ok()
        );
    }

    #[test]
    fn inverse_of_round_trips_for_symmetric_relations() {
        let registry = registry();
        let related = rel("related_to");
        assert_eq!(registry.inverse_of(&related), Some(related));
    }
}
