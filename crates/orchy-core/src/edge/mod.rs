use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::Result;
use crate::organization::OrganizationId;
use crate::resource_ref::ResourceKind;

#[async_trait]
pub trait EdgeStore: Send + Sync {
    async fn save(&self, edge: &Edge) -> Result<()>;
    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>>;
    async fn delete(&self, id: &EdgeId) -> Result<()>;
    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>>;
    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_type: Option<&RelationType>,
    ) -> Result<Vec<Edge>>;
    async fn traverse(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        max_depth: u32,
        rel_types: Option<&[RelationType]>,
        direction: TraversalDirection,
    ) -> Result<Vec<TraversalEdge>>;
    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EdgeId(Uuid);

impl EdgeId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for EdgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EdgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EdgeId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    /// This was derived/created from that (knowledge from task result, task from knowledge)
    DerivedFrom,
    /// Completing/processing this produced that as output
    Produces,
    /// This references that (generic weak link)
    References,
    /// This replaces/supersedes that (new task replacing old, refined knowledge replacing original)
    Supersedes,
    /// This was merged/consolidated from that (knowledge merged into summary)
    MergedFrom,
    /// This summarizes that (summary knowledge entry covering other entries)
    Summarizes,
    /// This implements that (task implementing a plan/decision)
    Implements,
    /// This spawned/triggered that (knowledge or message spawning a task)
    Spawns,
    /// General symmetric relationship
    RelatedTo,
}

impl RelationType {
    pub fn all() -> &'static [RelationType] {
        &[
            RelationType::DerivedFrom,
            RelationType::Produces,
            RelationType::References,
            RelationType::Supersedes,
            RelationType::MergedFrom,
            RelationType::Summarizes,
            RelationType::Implements,
            RelationType::Spawns,
            RelationType::RelatedTo,
        ]
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RelationType::DerivedFrom => "derived_from",
            RelationType::Produces => "produces",
            RelationType::References => "references",
            RelationType::Supersedes => "supersedes",
            RelationType::MergedFrom => "merged_from",
            RelationType::Summarizes => "summarizes",
            RelationType::Implements => "implements",
            RelationType::Spawns => "spawns",
            RelationType::RelatedTo => "related_to",
        };
        write!(f, "{s}")
    }
}

impl FromStr for RelationType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "derived_from" => Ok(RelationType::DerivedFrom),
            "produces" => Ok(RelationType::Produces),
            "references" => Ok(RelationType::References),
            "supersedes" => Ok(RelationType::Supersedes),
            "merged_from" => Ok(RelationType::MergedFrom),
            "summarizes" => Ok(RelationType::Summarizes),
            "implements" => Ok(RelationType::Implements),
            "spawns" => Ok(RelationType::Spawns),
            "related_to" => Ok(RelationType::RelatedTo),
            other => Err(format!("unknown relation type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    #[default]
    Outgoing,
    Incoming,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    id: EdgeId,
    org_id: OrganizationId,
    from_kind: ResourceKind,
    from_id: String,
    to_kind: ResourceKind,
    to_id: String,
    rel_type: RelationType,
    display: Option<String>,
    created_at: DateTime<Utc>,
    created_by: Option<AgentId>,
}

impl Edge {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        org_id: OrganizationId,
        from_kind: ResourceKind,
        from_id: String,
        to_kind: ResourceKind,
        to_id: String,
        rel_type: RelationType,
        display: Option<String>,
        created_by: Option<AgentId>,
    ) -> Self {
        Self {
            id: EdgeId::new(),
            org_id,
            from_kind,
            from_id,
            to_kind,
            to_id,
            rel_type,
            display,
            created_at: Utc::now(),
            created_by,
        }
    }

    pub fn restore(r: RestoreEdge) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            from_kind: r.from_kind,
            from_id: r.from_id,
            to_kind: r.to_kind,
            to_id: r.to_id,
            rel_type: r.rel_type,
            display: r.display,
            created_at: r.created_at,
            created_by: r.created_by,
        }
    }

    pub fn id(&self) -> EdgeId {
        self.id
    }

    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }

    pub fn from_kind(&self) -> &ResourceKind {
        &self.from_kind
    }

    pub fn from_id(&self) -> &str {
        &self.from_id
    }

    pub fn to_kind(&self) -> &ResourceKind {
        &self.to_kind
    }

    pub fn to_id(&self) -> &str {
        &self.to_id
    }

    pub fn rel_type(&self) -> &RelationType {
        &self.rel_type
    }

    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn created_by(&self) -> Option<&AgentId> {
        self.created_by.as_ref()
    }
}

pub struct RestoreEdge {
    pub id: EdgeId,
    pub org_id: OrganizationId,
    pub from_kind: ResourceKind,
    pub from_id: String,
    pub to_kind: ResourceKind,
    pub to_id: String,
    pub rel_type: RelationType,
    pub display: Option<String>,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<AgentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalEdge {
    pub id: EdgeId,
    pub from_kind: ResourceKind,
    pub from_id: String,
    pub to_kind: ResourceKind,
    pub to_id: String,
    pub rel_type: RelationType,
    pub display: Option<String>,
    pub depth: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_type_roundtrip() {
        for rt in RelationType::all() {
            let s = rt.to_string();
            let parsed: RelationType = s.parse().unwrap();
            assert_eq!(*rt, parsed);
        }
    }

    #[test]
    fn edge_new_sets_fields() {
        let org = OrganizationId::new("test").unwrap();
        let edge = Edge::new(
            org,
            ResourceKind::Task,
            "task-id-1".to_string(),
            ResourceKind::Knowledge,
            "know-id-1".to_string(),
            RelationType::Produces,
            None,
            None,
        );
        assert_eq!(edge.from_kind(), &ResourceKind::Task);
        assert_eq!(edge.to_kind(), &ResourceKind::Knowledge);
        assert_eq!(edge.rel_type(), &RelationType::Produces);
    }

    #[test]
    fn traversal_direction_default_is_outgoing() {
        assert_eq!(TraversalDirection::default(), TraversalDirection::Outgoing);
    }
}
