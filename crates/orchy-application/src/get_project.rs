use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::dto::ProjectResponse;

pub struct GetProjectCommand {
    pub org_id: String,
    pub project: String,
}

pub struct GetProject {
    store: Arc<dyn ProjectStore>,
}

impl GetProject {
    pub fn new(store: Arc<dyn ProjectStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: GetProjectCommand) -> Result<ProjectResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;

        match self.store.find_by_id(&org_id, &project).await? {
            Some(p) => Ok(ProjectResponse::from(&p)),
            None => {
                let mut p = Project::new(org_id, project, String::new())?;
                self.store.save(&mut p).await?;
                Ok(ProjectResponse::from(&p))
            }
        }
    }
}
