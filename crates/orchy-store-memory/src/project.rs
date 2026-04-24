use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::MemoryState;

pub struct MemoryProjectStore {
    state: Arc<MemoryState>,
}

impl MemoryProjectStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ProjectStore for MemoryProjectStore {
    async fn save(&self, project: &mut Project) -> Result<()> {
        {
            let mut projects = self.state.projects.write().await;
            projects.insert(project.id().clone(), project.clone());
        }

        let events = project.drain_events();
        if !events.is_empty() {
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| orchy_core::error::Error::Store(e.to_string()))?;
                self.state.events.write().await.push(serialized);
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        let projects = self.state.projects.read().await;
        Ok(projects.get(id).filter(|p| p.org_id() == org).cloned())
    }
}
