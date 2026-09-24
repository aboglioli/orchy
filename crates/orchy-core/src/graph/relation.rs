use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entity_ref::EntityKind;
use crate::error::{DomainError, Result};

/// Every rule is an exhaustive match, so adding a variant without deciding its endpoints,
/// inverse and arity does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Supersedes,
    DerivedFrom,
    Summarizes,
    Invalidates,
    Confirms,
    SupportedBy,
    ContradictedBy,
    MergedFrom,
    RelatedTo,
    DependsOn,
    Parent,
    SpawnedBy,
    Produces,
    Implements,
    OwnedBy,
    ReviewedBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arity {
    One,
    Many,
}

impl Relation {
    pub const ALL: [Self; 16] = [
        Self::Supersedes,
        Self::DerivedFrom,
        Self::Summarizes,
        Self::Invalidates,
        Self::Confirms,
        Self::SupportedBy,
        Self::ContradictedBy,
        Self::MergedFrom,
        Self::RelatedTo,
        Self::DependsOn,
        Self::Parent,
        Self::SpawnedBy,
        Self::Produces,
        Self::Implements,
        Self::OwnedBy,
        Self::ReviewedBy,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Supersedes => "supersedes",
            Self::DerivedFrom => "derived_from",
            Self::Summarizes => "summarizes",
            Self::Invalidates => "invalidates",
            Self::Confirms => "confirms",
            Self::SupportedBy => "supported_by",
            Self::ContradictedBy => "contradicted_by",
            Self::MergedFrom => "merged_from",
            Self::RelatedTo => "related_to",
            Self::DependsOn => "depends_on",
            Self::Parent => "parent",
            Self::SpawnedBy => "spawned_by",
            Self::Produces => "produces",
            Self::Implements => "implements",
            Self::OwnedBy => "owned_by",
            Self::ReviewedBy => "reviewed_by",
        }
    }

    /// The name this relation projects under on the far side. Projections are rendered into
    /// frontmatter and never stored, so they are a name rather than a `Relation`.
    pub fn inverse(&self) -> &'static str {
        match self {
            Self::Supersedes => "superseded_by",
            Self::DerivedFrom => "derives",
            Self::Summarizes => "summarized_by",
            Self::Invalidates => "invalidated_by",
            Self::Confirms => "confirmed_by",
            Self::SupportedBy => "supports",
            Self::ContradictedBy => "contradicts",
            Self::MergedFrom => "merged_into",
            Self::RelatedTo => "related_to",
            Self::DependsOn => "blocks",
            Self::Parent => "subtasks",
            Self::SpawnedBy => "spawns",
            Self::Produces => "produced_by",
            Self::Implements => "implemented_by",
            Self::OwnedBy => "owns",
            Self::ReviewedBy => "reviewed",
        }
    }

    pub fn is_symmetric(&self) -> bool {
        matches!(self, Self::RelatedTo)
    }

    pub fn arity(&self) -> Arity {
        match self {
            Self::Parent | Self::SpawnedBy => Arity::One,
            _ => Arity::Many,
        }
    }

    pub fn accepts(&self, from: EntityKind, to: EntityKind) -> bool {
        use EntityKind::*;
        match self {
            Self::DependsOn | Self::Parent => from == Task && to == Task,
            Self::SpawnedBy => from == Task && to == Message,
            // a task can write down a skill as readily as a document
            Self::Produces | Self::Implements => from == Task && (to == Document || to == Skill),
            Self::OwnedBy | Self::ReviewedBy => to == Actor,
            // replacing means replacing like with like: a task does not supersede a document
            Self::Supersedes | Self::MergedFrom => from == to && from.is_content(),

            // claims about content, which a message carries as much as a document does
            Self::DerivedFrom
            | Self::Summarizes
            | Self::Invalidates
            | Self::Confirms
            | Self::SupportedBy
            | Self::ContradictedBy => from.is_content() && to.is_content(),

            // the escape hatch: anything may simply be related to anything
            Self::RelatedTo => true,
        }
    }

    /// The only kind this relation can point at, when it has one. Lets a hand-written bare id
    /// be typed without an index lookup.
    pub fn sole_target_kind(&self) -> Option<EntityKind> {
        const KINDS: [EntityKind; 5] = [
            EntityKind::Document,
            EntityKind::Task,
            EntityKind::Message,
            EntityKind::Skill,
            EntityKind::Actor,
        ];
        let accepted: Vec<EntityKind> = KINDS
            .into_iter()
            .filter(|to| KINDS.iter().any(|from| self.accepts(*from, *to)))
            .collect();

        match accepted.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }

    /// Relations whose creation carries consequences beyond the edge itself, and so must go
    /// through the command that applies them rather than through `orchy link`.
    pub fn managed_by(&self) -> Option<&'static str> {
        match self {
            Self::Parent => Some("orchy task update --parent"),
            Self::DependsOn => Some("orchy task dep --add"),
            Self::Supersedes => Some("orchy supersede / orchy task replace"),
            Self::SpawnedBy => Some("orchy msg promote"),
            _ => None,
        }
    }

    pub fn validate(&self, from: EntityKind, to: EntityKind) -> Result<()> {
        if self.accepts(from, to) {
            return Ok(());
        }
        Err(DomainError::validation(format!(
            "`{self}` does not connect {from} to {to}"
        )))
    }
}

impl fmt::Display for Relation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Relation {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let name = s.trim().to_lowercase();
        if let Some(relation) = Self::ALL.into_iter().find(|r| r.as_str() == name) {
            return Ok(relation);
        }
        if let Some(stored) = Self::ALL
            .into_iter()
            .find(|r| !r.is_symmetric() && r.inverse() == name)
        {
            return Err(DomainError::validation(format!(
                "`{name}` is the projected side of `{stored}`; store `{stored}` on the other entity instead"
            )));
        }
        Err(DomainError::UnknownRelation(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EntityKind::*;

    #[test]
    fn every_relation_round_trips_through_its_wire_name() {
        for relation in Relation::ALL {
            assert_eq!(relation.as_str().parse::<Relation>().unwrap(), relation);
        }
    }

    #[test]
    fn names_and_inverses_are_all_distinct() {
        let mut seen: Vec<&str> = Vec::new();
        for relation in Relation::ALL {
            assert!(!seen.contains(&relation.as_str()), "duplicate {relation}");
            seen.push(relation.as_str());
        }
        for relation in Relation::ALL.into_iter().filter(|r| !r.is_symmetric()) {
            assert!(
                !Relation::ALL
                    .iter()
                    .any(|r| r.as_str() == relation.inverse()),
                "{relation}'s inverse `{}` collides with a stored relation",
                relation.inverse()
            );
        }
    }

    #[test]
    fn only_related_to_is_its_own_inverse() {
        for relation in Relation::ALL {
            let self_inverse = relation.inverse() == relation.as_str();
            assert_eq!(self_inverse, relation.is_symmetric(), "{relation}");
        }
    }

    #[test]
    fn asking_for_a_projected_name_says_which_side_to_store() {
        let err = "subtasks".parse::<Relation>().unwrap_err();
        assert!(err.to_string().contains("store `parent`"), "{err}");

        let err = "blocks".parse::<Relation>().unwrap_err();
        assert!(err.to_string().contains("store `depends_on`"), "{err}");
    }

    #[test]
    fn an_invented_relation_is_unknown_not_a_projection() {
        assert!(matches!(
            "invented".parse::<Relation>().unwrap_err(),
            DomainError::UnknownRelation(_)
        ));
    }

    #[test]
    fn the_task_hierarchy_is_single_valued_and_task_to_task() {
        assert_eq!(Relation::Parent.arity(), Arity::One);
        assert!(Relation::Parent.accepts(Task, Task));
        assert!(!Relation::Parent.accepts(Document, Task));
        assert!(!Relation::Parent.accepts(Task, Document));
    }

    #[test]
    fn ownership_points_at_an_actor_from_anywhere() {
        assert!(Relation::OwnedBy.accepts(Document, Actor));
        assert!(Relation::OwnedBy.accepts(Task, Actor));
        assert!(!Relation::OwnedBy.accepts(Document, Document));
    }

    #[test]
    fn a_message_is_only_ever_the_target_of_spawned_by() {
        assert!(Relation::SpawnedBy.accepts(Task, Message));
        assert!(!Relation::SpawnedBy.accepts(Message, Task));
    }

    #[test]
    fn replacing_joins_like_with_like() {
        assert!(Relation::Supersedes.accepts(Document, Document));
        assert!(Relation::Supersedes.accepts(Task, Task));
        assert!(Relation::Supersedes.accepts(Message, Message));
        assert!(
            !Relation::Supersedes.accepts(Task, Document),
            "a task does not replace a document"
        );
        assert!(
            !Relation::Supersedes.accepts(Actor, Actor),
            "an actor is not content and is never replaced by one"
        );
    }

    #[test]
    fn a_claim_about_content_may_cross_between_documents_tasks_and_messages() {
        for relation in [
            Relation::DerivedFrom,
            Relation::Summarizes,
            Relation::Invalidates,
            Relation::Confirms,
            Relation::SupportedBy,
            Relation::ContradictedBy,
        ] {
            assert!(
                relation.accepts(Document, Message),
                "{relation}: a document must be able to cite a thread"
            );
            assert!(
                relation.accepts(Message, Document),
                "{relation}: and a thread to answer a document"
            );
            assert!(
                !relation.accepts(Document, Actor),
                "{relation}: an actor is a participant, not evidence"
            );
        }
    }

    #[test]
    fn related_to_joins_anything_because_that_is_what_it_is_for() {
        for from in [Document, Task, Message, Actor] {
            for to in [Document, Task, Message, Actor] {
                assert!(Relation::RelatedTo.accepts(from, to), "{from} -> {to}");
            }
        }
    }

    #[test]
    fn exactly_the_four_derived_relations_are_command_managed() {
        let managed: Vec<Relation> = Relation::ALL
            .into_iter()
            .filter(|r| r.managed_by().is_some())
            .collect();
        assert_eq!(
            managed,
            vec![
                Relation::Supersedes,
                Relation::DependsOn,
                Relation::Parent,
                Relation::SpawnedBy
            ],
            "only relations with consequences beyond the edge are managed"
        );
    }

    #[test]
    fn validate_names_both_ends_when_it_refuses() {
        let err = Relation::Parent.validate(Document, Task).unwrap_err();
        assert!(err.to_string().contains("document"), "{err}");
        assert!(err.to_string().contains("task"), "{err}");
    }
}

#[cfg(test)]
mod target_kind_tests {
    use super::*;

    #[test]
    fn relations_with_one_possible_target_report_it() {
        assert_eq!(Relation::Parent.sole_target_kind(), Some(EntityKind::Task));
        assert_eq!(
            Relation::DependsOn.sole_target_kind(),
            Some(EntityKind::Task)
        );
        assert_eq!(
            Relation::SpawnedBy.sole_target_kind(),
            Some(EntityKind::Message)
        );
        assert_eq!(
            Relation::OwnedBy.sole_target_kind(),
            Some(EntityKind::Actor)
        );
    }

    #[test]
    fn relations_that_span_kinds_report_nothing_rather_than_a_favourite() {
        for relation in [
            Relation::Supersedes,
            Relation::RelatedTo,
            Relation::DerivedFrom,
            Relation::MergedFrom,
            // a task writes documents and skills alike, so a bare id has to say which
            Relation::Produces,
            Relation::Implements,
        ] {
            assert_eq!(
                relation.sole_target_kind(),
                None,
                "{relation} can point at more than one kind, so it must not claim one"
            );
        }
    }

    #[test]
    fn a_sole_target_is_always_one_the_relation_actually_accepts() {
        for relation in Relation::ALL {
            if let Some(target) = relation.sole_target_kind() {
                assert!(
                    [
                        EntityKind::Document,
                        EntityKind::Task,
                        EntityKind::Message,
                        EntityKind::Actor
                    ]
                    .iter()
                    .any(|from| relation.accepts(*from, target)),
                    "{relation} claims {target} but accepts nothing into it"
                );
            }
        }
    }
}
