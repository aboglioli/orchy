use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;

use crate::dto::ProjectResponse;

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

    pub async fn execute(&self, cmd: SetProjectMetadataCommand) -> Result<ProjectResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut p = self
            .store
            .find_by_id(&org_id, &project)
            .await?
            .ok_or(Error::NotFound("project not found".to_string()))?;

        p.set_metadata(cmd.key, cmd.value)?;
        self.store.save(&mut p).await?;
        Ok(ProjectResponse::from(&p))
    }
}
