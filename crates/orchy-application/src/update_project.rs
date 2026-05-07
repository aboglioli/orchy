use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::project::{Project, ProjectStore};

use crate::dto::ProjectDto;

pub struct UpdateProjectCommand {
    pub org_id: String,
    pub project: String,
    pub description: String,
}

pub struct UpdateProject {
    store: Arc<dyn ProjectStore>,
}

impl UpdateProject {
    pub fn new(store: Arc<dyn ProjectStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: UpdateProjectCommand) -> ApplicationResult<ProjectDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;

        let mut p = match self.store.find_by_id(&org_id, &project).await? {
            Some(project) => project,
            None => Project::new(org_id, project, String::new())?,
        };

        p.update_description(cmd.description)?;
        self.store.save(&mut p).await?;
        Ok(ProjectDto::from(&p))
    }
}
