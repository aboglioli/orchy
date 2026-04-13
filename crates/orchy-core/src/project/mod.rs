pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::ProjectId;
use crate::note::Note;

pub trait ProjectStore: Send + Sync {
    fn save(&self, project: &Project) -> impl Future<Output = Result<()>> + Send;
    fn get(&self, id: &ProjectId) -> impl Future<Output = Result<Option<Project>>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    id: ProjectId,
    description: String,
    notes: Vec<Note>,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(id: ProjectId, description: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            description,
            notes: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn restore(
        id: ProjectId,
        description: String,
        notes: Vec<Note>,
        metadata: HashMap<String, String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            description,
            notes,
            metadata,
            created_at,
            updated_at,
        }
    }

    pub fn update_description(&mut self, description: String) {
        self.description = description;
        self.updated_at = Utc::now();
    }

    pub fn add_note(&mut self, author: Option<AgentId>, body: String) {
        self.notes.push(Note::new(author, body));
        self.updated_at = Utc::now();
    }

    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
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
