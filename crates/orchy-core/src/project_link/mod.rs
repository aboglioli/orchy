pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};
use crate::namespace::ProjectId;

use self::events as link_events;

pub trait ProjectLinkStore: Send + Sync {
    fn save(&self, link: &mut ProjectLink) -> impl Future<Output = Result<()>> + Send;
    fn delete(&self, id: &ProjectLinkId) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(
        &self,
        id: &ProjectLinkId,
    ) -> impl Future<Output = Result<Option<ProjectLink>>> + Send;
    fn list_by_target(
        &self,
        target: &ProjectId,
    ) -> impl Future<Output = Result<Vec<ProjectLink>>> + Send;
    fn find_link(
        &self,
        source: &ProjectId,
        target: &ProjectId,
    ) -> impl Future<Output = Result<Option<ProjectLink>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectLinkId(Uuid);

impl ProjectLinkId {
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

impl Default for ProjectLinkId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProjectLinkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ProjectLinkId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharedResourceType {
    Knowledge,
    Tasks,
}

impl fmt::Display for SharedResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharedResourceType::Knowledge => write!(f, "knowledge"),
            SharedResourceType::Tasks => write!(f, "tasks"),
        }
    }
}

impl FromStr for SharedResourceType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "knowledge" => Ok(SharedResourceType::Knowledge),
            "tasks" => Ok(SharedResourceType::Tasks),
            other => Err(format!("unknown resource type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLink {
    id: ProjectLinkId,
    source_project: ProjectId,
    target_project: ProjectId,
    resource_types: Vec<SharedResourceType>,
    created_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl ProjectLink {
    pub fn new(
        source_project: ProjectId,
        target_project: ProjectId,
        resource_types: Vec<SharedResourceType>,
    ) -> Result<Self> {
        if source_project == target_project {
            return Err(Error::InvalidInput(
                "cannot link a project to itself".into(),
            ));
        }
        if resource_types.is_empty() {
            return Err(Error::InvalidInput(
                "resource_types must not be empty".into(),
            ));
        }

        let mut link = Self {
            id: ProjectLinkId::new(),
            source_project,
            target_project,
            resource_types,
            created_at: Utc::now(),
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            link.source_project.as_ref(),
            link_events::NAMESPACE,
            link_events::TOPIC_CREATED,
            Payload::from_json(&link_events::ProjectLinkCreatedPayload {
                link_id: link.id.to_string(),
                source_project: link.source_project.to_string(),
                target_project: link.target_project.to_string(),
                resource_types: link.resource_types.iter().map(|r| r.to_string()).collect(),
            })
            .unwrap(),
        )
        .map(|e| link.collector.collect(e));

        Ok(link)
    }

    pub fn restore(r: RestoreProjectLink) -> Self {
        Self {
            id: r.id,
            source_project: r.source_project,
            target_project: r.target_project,
            resource_types: r.resource_types,
            created_at: r.created_at,
            collector: EventCollector::new(),
        }
    }

    pub fn mark_deleted(&mut self) {
        let _ = Event::create(
            self.source_project.as_ref(),
            link_events::NAMESPACE,
            link_events::TOPIC_DELETED,
            Payload::from_json(&link_events::ProjectLinkDeletedPayload {
                link_id: self.id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> ProjectLinkId {
        self.id
    }

    pub fn source_project(&self) -> &ProjectId {
        &self.source_project
    }

    pub fn target_project(&self) -> &ProjectId {
        &self.target_project
    }

    pub fn resource_types(&self) -> &[SharedResourceType] {
        &self.resource_types
    }

    pub fn has_resource_type(&self, rt: SharedResourceType) -> bool {
        self.resource_types.contains(&rt)
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

pub struct RestoreProjectLink {
    pub id: ProjectLinkId,
    pub source_project: ProjectId,
    pub target_project: ProjectId,
    pub resource_types: Vec<SharedResourceType>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(s: &str) -> ProjectId {
        ProjectId::try_from(s).unwrap()
    }

    #[test]
    fn new_link_succeeds() {
        let link = ProjectLink::new(
            project("source"),
            project("target"),
            vec![SharedResourceType::Knowledge],
        );
        assert!(link.is_ok());
    }

    #[test]
    fn cannot_link_to_self() {
        let link = ProjectLink::new(
            project("same"),
            project("same"),
            vec![SharedResourceType::Knowledge],
        );
        assert!(link.is_err());
    }

    #[test]
    fn empty_resource_types_fails() {
        let link = ProjectLink::new(project("a"), project("b"), vec![]);
        assert!(link.is_err());
    }

    #[test]
    fn has_resource_type_checks() {
        let link =
            ProjectLink::new(project("a"), project("b"), vec![SharedResourceType::Knowledge]).unwrap();
        assert!(link.has_resource_type(SharedResourceType::Knowledge));
        assert!(!link.has_resource_type(SharedResourceType::Tasks));
    }

    #[test]
    fn parse_resource_type() {
        assert_eq!(
            "knowledge".parse::<SharedResourceType>().unwrap(),
            SharedResourceType::Knowledge
        );
        assert_eq!(
            "tasks".parse::<SharedResourceType>().unwrap(),
            SharedResourceType::Tasks
        );
        assert!("invalid".parse::<SharedResourceType>().is_err());
    }
}
