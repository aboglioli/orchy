use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::dto::ProjectDto;

pub struct SetProjectMetadataCommand {
    pub org_id: String,
    pub project: String,
    pub key: String,
    pub value: String,
}

pub struct SetProjectMetadata {
    store: Arc<dyn ProjectStore>,
}

impl SetProjectMetadata {
    pub fn new(store: Arc<dyn ProjectStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: SetProjectMetadataCommand) -> Result<ProjectDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut p = match self.store.find_by_id(&org_id, &project).await? {
            Some(project) => project,
            None => Project::new(org_id, project, String::new())?,
        };

        p.set_metadata(cmd.key, cmd.value)?;
        self.store.save(&mut p).await?;
        Ok(ProjectDto::from(&p))
    }
}
