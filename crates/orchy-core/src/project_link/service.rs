use std::sync::Arc;

use super::{ProjectLink, ProjectLinkStore, SharedResourceType};
use crate::error::{Error, Result};
use crate::namespace::ProjectId;

pub struct ProjectLinkService<S: ProjectLinkStore> {
    store: Arc<S>,
}

impl<S: ProjectLinkStore> ProjectLinkService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn link(
        &self,
        source: ProjectId,
        target: ProjectId,
        resource_types: Vec<SharedResourceType>,
    ) -> Result<ProjectLink> {
        if let Some(existing) = self.store.find_link(&source, &target).await? {
            self.store.delete(&existing.id()).await?;
        }

        let link = ProjectLink::new(source, target, resource_types)?;
        self.store.save(&link).await?;
        Ok(link)
    }

    pub async fn unlink(&self, source: &ProjectId, target: &ProjectId) -> Result<()> {
        let link = self
            .store
            .find_link(source, target)
            .await?
            .ok_or_else(|| Error::NotFound(format!("link from {source} to {target}")))?;

        self.store.delete(&link.id()).await
    }

    pub async fn list_links(&self, target: &ProjectId) -> Result<Vec<ProjectLink>> {
        self.store.list_by_target(target).await
    }

    pub async fn linked_projects(
        &self,
        target: &ProjectId,
        resource_type: SharedResourceType,
    ) -> Result<Vec<ProjectId>> {
        let links = self.store.list_by_target(target).await?;
        Ok(links
            .into_iter()
            .filter(|l| l.has_resource_type(resource_type))
            .map(|l| l.source_project().clone())
            .collect())
    }
}
