use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::MemoryBackend;

#[async_trait]
impl ProjectStore for MemoryBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        {
            let mut projects = self.projects.write().await;
            projects.insert(project.id().clone(), project.clone());
        }

        let events = project.drain_events();
        if !events.is_empty() {
            if let Err(e) = orchy_events::io::Writer::write_all(self, &events).await {
                tracing::error!("failed to persist events: {e}");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        let projects = self.projects.read().await;
        Ok(projects.get(id).filter(|p| p.org_id() == org).cloned())
    }
}
