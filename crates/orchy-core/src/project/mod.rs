pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::ProjectId;

use self::events as project_events;
use crate::note::Note;

pub trait ProjectStore: Send + Sync {
    fn save(&self, project: &Project) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &ProjectId) -> impl Future<Output = Result<Option<Project>>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    id: ProjectId,
    description: String,
    notes: Vec<Note>,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Project {
    pub fn new(id: ProjectId, description: String) -> Self {
        let now = Utc::now();
        let mut project = Self {
            id,
            description,
            notes: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            project.id.as_ref(),
            project_events::NAMESPACE,
            project_events::TOPIC_CREATED,
            Payload::from_json(&project_events::ProjectCreatedPayload {
                project: project.id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| project.collector.collect(e));

        project
    }

    pub fn restore(r: RestoreProject) -> Self {
        Self {
            id: r.id,
            description: r.description,
            notes: r.notes,
            metadata: r.metadata,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn update_description(&mut self, description: String) {
        self.description = description;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.id.as_ref(),
            project_events::NAMESPACE,
            project_events::TOPIC_DESCRIPTION_UPDATED,
            Payload::from_json(&project_events::ProjectDescriptionUpdatedPayload {
                project: self.id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn add_note(&mut self, author: Option<AgentId>, body: String) {
        self.notes.push(Note::new(author, body.clone()));
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.id.as_ref(),
            project_events::NAMESPACE,
            project_events::TOPIC_NOTE_ADDED,
            Payload::from_json(&project_events::ProjectNoteAddedPayload {
                project: self.id.to_string(),
                body,
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key.clone(), value.clone());
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.id.as_ref(),
            project_events::NAMESPACE,
            project_events::TOPIC_METADATA_SET,
            Payload::from_json(&project_events::ProjectMetadataSetPayload {
                project: self.id.to_string(),
                key,
                value,
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> &ProjectId {
        &self.id
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn notes(&self) -> &[Note] {
        &self.notes
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
    pub id: ProjectId,
    pub description: String,
    pub notes: Vec<Note>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> Project {
        let id = ProjectId::try_from("test-project".to_string()).unwrap();
        Project::new(id, "a test project".to_string())
    }

    #[test]
    fn new_project_empty_notes() {
        let project = test_project();
        assert!(project.notes().is_empty());
    }

    #[test]
    fn add_note_appends() {
        let mut project = test_project();
        project.add_note(None, "first note".to_string());
        assert_eq!(project.notes().len(), 1);
    }

    #[test]
    fn update_description_changes() {
        let mut project = test_project();
        project.update_description("new description".to_string());
        assert_eq!(project.description(), "new description");
    }

    #[test]
    fn set_metadata_inserts() {
        let mut project = test_project();
        project.set_metadata("env".to_string(), "prod".to_string());
        assert_eq!(project.metadata().get("env"), Some(&"prod".to_string()));
    }
}
