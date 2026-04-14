use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::project::{Project, ProjectStore};
use orchy_events::SerializedEvent;

use crate::MemoryBackend;

impl ProjectStore for MemoryBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        let mut projects = self
            .projects
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        projects.insert(project.id().clone(), project.clone());
        drop(projects);

        let events = project.drain_events();
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

    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        let projects = self
            .projects
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(projects.get(id).cloned())
    }
}
