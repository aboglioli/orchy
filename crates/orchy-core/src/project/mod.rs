pub mod events;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};
use crate::namespace::ProjectId;
use crate::organization::OrganizationId;

use self::events as project_events;

#[async_trait::async_trait]
pub trait ProjectStore: Send + Sync {
    async fn save(&self, project: &mut Project) -> Result<()>;
    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    org_id: OrganizationId,
    id: ProjectId,
    description: String,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Project {
    pub fn new(org_id: OrganizationId, id: ProjectId, description: String) -> Result<Self> {
        let now = Utc::now();
        let mut project = Self {
            org_id,
            id,
            description,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&project_events::ProjectCreatedPayload {
            org_id: project.org_id.to_string(),
            project: project.id.to_string(),
            description: project.description.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            project.org_id.as_str(),
            project_events::NAMESPACE,
            project_events::TOPIC_CREATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        project.collector.collect(event);

        Ok(project)
    }

    pub fn restore(r: RestoreProject) -> Self {
        Self {
            org_id: r.org_id,
            id: r.id,
            description: r.description,
            metadata: r.metadata,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn update_description(&mut self, description: String) -> Result<()> {
        self.description = description;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&project_events::ProjectDescriptionUpdatedPayload {
            org_id: self.org_id.to_string(),
            project: self.id.to_string(),
            description: self.description.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            project_events::NAMESPACE,
            project_events::TOPIC_DESCRIPTION_UPDATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn set_metadata(&mut self, key: String, value: String) -> Result<()> {
        self.metadata.insert(key.clone(), value.clone());
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&project_events::ProjectMetadataSetPayload {
            org_id: self.org_id.to_string(),
            project: self.id.to_string(),
            key,
            value,
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            project_events::NAMESPACE,
            project_events::TOPIC_METADATA_SET,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }
    pub fn id(&self) -> &ProjectId {
        &self.id
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct RestoreProject {
    pub org_id: OrganizationId,
    pub id: ProjectId,
    pub description: String,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> Project {
        use orchy_events::OrganizationId;
        let org_id = OrganizationId::new("test").unwrap();
        let id = ProjectId::try_from("test-project".to_string()).unwrap();
        Project::new(org_id, id, "a test project".to_string()).unwrap()
    }

    #[test]
    fn update_description_changes() {
        let mut project = test_project();
        project
            .update_description("new description".to_string())
            .unwrap();
        assert_eq!(project.description(), "new description");
    }

    #[test]
    fn set_metadata_inserts() {
        let mut project = test_project();
        project
            .set_metadata("env".to_string(), "prod".to_string())
            .unwrap();
        assert_eq!(project.metadata().get("env"), Some(&"prod".to_string()));
    }
}
