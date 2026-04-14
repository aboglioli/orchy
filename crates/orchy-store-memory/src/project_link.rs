use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::project_link::{ProjectLink, ProjectLinkId, ProjectLinkStore};
use orchy_events::SerializedEvent;

use crate::MemoryBackend;

impl ProjectLinkStore for MemoryBackend {
    async fn save(&self, link: &mut ProjectLink) -> Result<()> {
        let mut links = self
            .project_links
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        links.insert(link.id(), link.clone());
        drop(links);

        let events = link.drain_events();
        if !events.is_empty() {
            let serialized: Vec<SerializedEvent> = events
                .iter()
                .filter_map(|e| SerializedEvent::from_event(e).ok())
                .collect();
            let mut store = self
                .events
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            store.extend(serialized);
        }
        Ok(())
    }

    async fn delete(&self, id: &ProjectLinkId) -> Result<()> {
        let mut links = self
            .project_links
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        links.remove(id);
        Ok(())
    }

    async fn find_by_id(&self, id: &ProjectLinkId) -> Result<Option<ProjectLink>> {
        let links = self
            .project_links
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(links.get(id).cloned())
    }

    async fn list_by_target(&self, target: &ProjectId) -> Result<Vec<ProjectLink>> {
        let links = self
            .project_links
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(links
            .values()
            .filter(|l| l.target_project() == target)
            .cloned()
            .collect())
    }

    async fn find_link(
        &self,
        source: &ProjectId,
        target: &ProjectId,
    ) -> Result<Option<ProjectLink>> {
        let links = self
            .project_links
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(links
            .values()
            .find(|l| l.source_project() == source && l.target_project() == target)
            .cloned())
    }
}
