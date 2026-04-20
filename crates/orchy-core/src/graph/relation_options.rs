use serde::{Deserialize, Serialize};

use crate::edge::{RelationType, TraversalDirection};
use crate::resource_ref::ResourceKind;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationOptions {
    pub rel_types: Option<Vec<RelationType>>,
    #[serde(default)]
    pub target_kinds: Vec<ResourceKind>,
    #[serde(default = "default_direction")]
    pub direction: TraversalDirection,
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_direction() -> TraversalDirection {
    TraversalDirection::Both
}

fn default_max_depth() -> u32 {
    1
}

fn default_limit() -> u32 {
    50
}

impl Default for RelationOptions {
    fn default() -> Self {
        Self {
            rel_types: None,
            target_kinds: vec![],
            direction: TraversalDirection::Both,
            max_depth: 1,
            limit: 50,
        }
    }
}

impl RelationOptions {
    pub fn resolve_rel_types<'a>(&'a self, anchor_kind: &ResourceKind) -> &'a [RelationType] {
        match &self.rel_types {
            Some(types) => types.as_slice(),
            None => match anchor_kind {
                ResourceKind::Task => &[
                    RelationType::DependsOn,
                    RelationType::Spawns,
                    RelationType::Implements,
                ],
                ResourceKind::Knowledge => &[
                    RelationType::Produces,
                    RelationType::SupportedBy,
                    RelationType::DerivedFrom,
                    RelationType::Invalidates,
                ],
                ResourceKind::Agent => &[RelationType::OwnedBy, RelationType::ReviewedBy],
                ResourceKind::Message => &[],
            },
        }
    }

    pub fn effective_max_depth(&self) -> u32 {
        self.max_depth.max(1)
    }

    pub fn effective_limit(&self) -> u32 {
        self.limit.max(1)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationQuery {
    pub anchor: crate::resource_ref::ResourceRef,
    #[serde(flatten)]
    pub options: RelationOptions,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<chrono::DateTime<chrono::Utc>>,
}
