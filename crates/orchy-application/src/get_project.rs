use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::dto::ProjectDto;

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

    pub async fn execute(&self, cmd: GetProjectCommand) -> ApplicationResult<ProjectDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;

        let p = match self.store.find_by_id(&org_id, &project).await? {
            Some(project) => project,
            None => Project::new(org_id, project, String::new())?,
        };

        Ok(ProjectDto::from(&p))
    }
}
