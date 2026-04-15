use std::sync::Arc;

use super::{Project, ProjectStore};
use crate::error::Result;
use crate::namespace::ProjectId;
use crate::organization::OrganizationId;

pub struct ProjectService<S: ProjectStore> {
    store: Arc<S>,
}

impl<S: ProjectStore> ProjectService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn get_or_create(&self, org: &OrganizationId, id: &ProjectId) -> Result<Project> {
        match self.store.find_by_id(org, id).await? {
            Some(project) => Ok(project),
            None => {
                let mut project = Project::new(org.clone(), id.clone(), String::new());
                self.store.save(&mut project).await?;
                Ok(project)
            }
        }
    }

    pub async fn get(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        self.store.find_by_id(org, id).await
    }

    pub async fn update_description(
        &self,
        org: &OrganizationId,
        id: &ProjectId,
        description: String,
    ) -> Result<Project> {
        let mut project = self.get_or_create(org, id).await?;
        project.update_description(description);
        self.store.save(&mut project).await?;
        Ok(project)
    }

    pub async fn set_metadata(
        &self,
        org: &OrganizationId,
        id: &ProjectId,
        key: String,
        value: String,
    ) -> Result<Project> {
        let mut project = self.get_or_create(org, id).await?;
        project.set_metadata(key, value);
        self.store.save(&mut project).await?;
        Ok(project)
    }
}
