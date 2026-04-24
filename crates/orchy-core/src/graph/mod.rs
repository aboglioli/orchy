pub mod events;
pub mod neighborhood;
pub mod relation_options;
pub mod rules;

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};
use crate::resource_ref::{ResourceKind, ResourceRef};

pub use events::*;
pub use neighborhood::{
    AgentSummary, EntityNeighborhood, KnowledgeSummary, LinkParam, MessageSummary, PeerEntity,
    Relation, TaskSummary,
};
pub use relation_options::{RelationOptions, RelationQuery};
pub use rules::check_no_cycle;

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait EdgeStore: Send + Sync {
    async fn save(&self, edge: &mut Edge) -> Result<()>;
    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>>;
    async fn delete(&self, id: &EdgeId) -> Result<()>;
    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>>;
    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>>;
    async fn exists_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<bool>;
    async fn find_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<Option<Edge>>;
    async fn list_by_org(
        &self,
        org: &OrganizationId,
        rel_type: Option<&RelationType>,
        page: PageParams,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Page<Edge>>;
    async fn find_neighbors(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        target_kinds: &[ResourceKind],
        direction: TraversalDirection,
        max_depth: u32,
        as_of: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TraversalHop>>;
    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()>;
    async fn delete_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
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
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| Error::invalid_input(format!("invalid edge id: {s}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    DerivedFrom,
    Produces,
    Supersedes,
    MergedFrom,
    Summarizes,
    Implements,
    Spawns,
    RelatedTo,
    DependsOn,
    Invalidates,
    SupportedBy,
    ContradictedBy,
    OwnedBy,
    ReviewedBy,
}

impl RelationType {
    pub fn all() -> &'static [RelationType] {
        &[
            RelationType::DerivedFrom,
            RelationType::Produces,
            RelationType::Supersedes,
            RelationType::MergedFrom,
            RelationType::Summarizes,
            RelationType::Implements,
            RelationType::Spawns,
            RelationType::RelatedTo,
            RelationType::DependsOn,
            RelationType::Invalidates,
            RelationType::SupportedBy,
            RelationType::ContradictedBy,
            RelationType::OwnedBy,
            RelationType::ReviewedBy,
        ]
    }

    pub fn aliases() -> &'static [(&'static str, RelationType)] {
        &[
            ("blocks", RelationType::DependsOn),
            ("creates", RelationType::Produces),
            ("fulfills", RelationType::Implements),
            ("child_of", RelationType::Spawns),
            ("based_on", RelationType::DerivedFrom),
        ]
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RelationType::DerivedFrom => write!(f, "derived_from"),
            RelationType::Produces => write!(f, "produces"),
            RelationType::Supersedes => write!(f, "supersedes"),
            RelationType::MergedFrom => write!(f, "merged_from"),
            RelationType::Summarizes => write!(f, "summarizes"),
            RelationType::Implements => write!(f, "implements"),
            RelationType::Spawns => write!(f, "spawns"),
            RelationType::RelatedTo => write!(f, "related_to"),
            RelationType::DependsOn => write!(f, "depends_on"),
            RelationType::Invalidates => write!(f, "invalidates"),
            RelationType::SupportedBy => write!(f, "supported_by"),
            RelationType::ContradictedBy => write!(f, "contradicted_by"),
            RelationType::OwnedBy => write!(f, "owned_by"),
            RelationType::ReviewedBy => write!(f, "reviewed_by"),
        }
    }
}

impl FromStr for RelationType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(rt) = Self::aliases().iter().find(|(alias, _)| *alias == s) {
            return Ok(rt.1);
        }
        match s {
            "derived_from" => Ok(RelationType::DerivedFrom),
            "produces" => Ok(RelationType::Produces),
            "supersedes" => Ok(RelationType::Supersedes),
            "merged_from" => Ok(RelationType::MergedFrom),
            "summarizes" => Ok(RelationType::Summarizes),
            "implements" => Ok(RelationType::Implements),
            "spawns" => Ok(RelationType::Spawns),
            "related_to" => Ok(RelationType::RelatedTo),
            "depends_on" => Ok(RelationType::DependsOn),
            "invalidates" => Ok(RelationType::Invalidates),
            "supported_by" => Ok(RelationType::SupportedBy),
            "contradicted_by" => Ok(RelationType::ContradictedBy),
            "owned_by" => Ok(RelationType::OwnedBy),
            "reviewed_by" => Ok(RelationType::ReviewedBy),
            other => Err(format!(
                "unknown relation type: {other}. valid: derived_from, produces, supersedes, merged_from, summarizes, implements, spawns, related_to, depends_on, invalidates, supported_by, contradicted_by, owned_by, reviewed_by"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    #[default]
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone)]
pub struct TraversalHop {
    pub edge: Edge,
    pub depth: u32,
    pub direction: RelationDirection,
    pub via: Option<ResourceRef>,
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
    created_at: DateTime<Utc>,
    created_by: Option<AgentId>,
    source_kind: Option<ResourceKind>,
    source_id: Option<String>,
    valid_until: Option<DateTime<Utc>>,
    #[serde(skip)]
    collector: EventCollector,
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
        created_by: Option<AgentId>,
    ) -> Result<Self> {
        let id = EdgeId::new();
        let mut edge = Self {
            id,
            org_id,
            from_kind,
            from_id,
            to_kind,
            to_id,
            rel_type,
            created_at: Utc::now(),
            created_by,
            source_kind: None,
            source_id: None,
            valid_until: None,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&events::EdgeCreatedPayload {
            org_id: edge.org_id.to_string(),
            edge_id: edge.id.to_string(),
            from_kind: edge.from_kind.to_string(),
            from_id: edge.from_id.clone(),
            to_kind: edge.to_kind.to_string(),
            to_id: edge.to_id.clone(),
            rel_type: edge.rel_type.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            edge.org_id.as_str(),
            events::NAMESPACE,
            events::TOPIC_CREATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        edge.collector.collect(event);

        Ok(edge)
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
            created_at: r.created_at,
            created_by: r.created_by,
            source_kind: r.source_kind,
            source_id: r.source_id,
            valid_until: r.valid_until,
            collector: EventCollector::new(),
        }
    }

    pub fn with_source(mut self, kind: ResourceKind, id: String) -> Self {
        self.source_kind = Some(kind);
        self.source_id = Some(id);
        self
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

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn created_by(&self) -> Option<&AgentId> {
        self.created_by.as_ref()
    }

    pub fn source_kind(&self) -> Option<&ResourceKind> {
        self.source_kind.as_ref()
    }

    pub fn source_id(&self) -> Option<&str> {
        self.source_id.as_deref()
    }

    pub fn invalidate(&mut self) -> Result<()> {
        self.valid_until = Some(Utc::now());

        let payload = Payload::from_json(&events::EdgeInvalidatedPayload {
            org_id: self.org_id.to_string(),
            edge_id: self.id.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            events::NAMESPACE,
            events::TOPIC_INVALIDATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn is_active(&self) -> bool {
        self.valid_until.is_none()
    }

    pub fn is_active_at(&self, ts: DateTime<Utc>) -> bool {
        self.created_at <= ts && self.valid_until.is_none_or(|vu| vu > ts)
    }

    pub fn valid_until(&self) -> Option<DateTime<Utc>> {
        self.valid_until
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
    pub created_at: DateTime<Utc>,
    pub created_by: Option<AgentId>,
    pub source_kind: Option<ResourceKind>,
    pub source_id: Option<String>,
    pub valid_until: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_type_aliases() {
        assert_eq!(
            "blocks".parse::<RelationType>().unwrap(),
            RelationType::DependsOn
        );
        assert_eq!(
            "creates".parse::<RelationType>().unwrap(),
            RelationType::Produces
        );
        assert_eq!(
            "child_of".parse::<RelationType>().unwrap(),
            RelationType::Spawns
        );
    }

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
        )
        .unwrap();
        assert_eq!(edge.from_kind(), &ResourceKind::Task);
        assert_eq!(edge.to_kind(), &ResourceKind::Knowledge);
        assert_eq!(edge.rel_type(), &RelationType::Produces);
    }

    #[test]
    fn traversal_direction_default_is_both() {
        assert_eq!(TraversalDirection::default(), TraversalDirection::Both);
    }

    #[test]
    fn edge_is_active_at() {
        use chrono::Duration;
        let org = OrganizationId::new("test").unwrap();
        let mut edge = Edge::new(
            org,
            ResourceKind::Task,
            "t1".to_string(),
            ResourceKind::Knowledge,
            "k1".to_string(),
            RelationType::Produces,
            None,
        )
        .unwrap();
        let before = edge.created_at() - Duration::seconds(1);
        let after_create = edge.created_at() + Duration::seconds(1);

        assert!(!edge.is_active_at(before));
        assert!(edge.is_active_at(after_create));

        edge.invalidate().unwrap();
        let valid_until = edge.valid_until().unwrap();
        let after_invalidate = valid_until + Duration::seconds(1);
        assert!(edge.is_active_at(edge.created_at()));
        assert!(!edge.is_active_at(after_invalidate));
    }

    #[test]
    fn edge_invalidate_sets_valid_until() {
        let org = OrganizationId::new("test").unwrap();
        let mut edge = Edge::new(
            org,
            ResourceKind::Task,
            "t1".to_string(),
            ResourceKind::Knowledge,
            "k1".to_string(),
            RelationType::Produces,
            None,
        )
        .unwrap();
        assert!(edge.is_active());
        assert!(edge.valid_until().is_none());
        edge.invalidate().unwrap();
        assert!(!edge.is_active());
        assert!(edge.valid_until().is_some());
    }
}
