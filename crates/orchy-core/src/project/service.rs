use std::sync::Arc;

use super::{Project, ProjectStore};
use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::ProjectId;

pub struct ProjectService<S: ProjectStore> {
    store: Arc<S>,
}

impl<S: ProjectStore> ProjectService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn get_or_create(&self, id: &ProjectId) -> Result<Project> {
        match self.store.get(id).await? {
            Some(project) => Ok(project),
            None => {
                let project = Project::new(id.clone(), String::new());
                self.store.save(&project).await?;
                Ok(project)
            }
        }
    }

    pub async fn get(&self, id: &ProjectId) -> Result<Option<Project>> {
        self.store.get(id).await
    }

    pub async fn update_description(&self, id: &ProjectId, description: String) -> Result<Project> {
        let mut project = self.get_or_create(id).await?;
        project.update_description(description);
        self.store.save(&project).await?;
        Ok(project)
    }

    pub async fn add_note(
        &self,
        id: &ProjectId,
        author: Option<AgentId>,
        body: String,
    ) -> Result<Project> {
        let mut project = self.get_or_create(id).await?;
        project.add_note(author, body);
        self.store.save(&project).await?;
        Ok(project)
    }
}
