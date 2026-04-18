use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::project::ProjectStore;
use orchy_core::task::{TaskFilter, TaskStore};

use crate::dto::{
    AgentResponse, KnowledgeResponse, ProjectOverview, ProjectResponse, TaskResponse,
};

pub struct GetProjectOverviewCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct GetProjectOverview {
    projects: Arc<dyn ProjectStore>,
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl GetProjectOverview {
    pub fn new(
        projects: Arc<dyn ProjectStore>,
        agents: Arc<dyn AgentStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            projects,
            agents,
            tasks,
            knowledge,
        }
    }

    pub async fn execute(&self, cmd: GetProjectOverviewCommand) -> Result<ProjectOverview> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project_id =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project = self
            .projects
            .find_by_id(&org_id, &project_id)
            .await?
            .map(|p| ProjectResponse::from(&p));

        let all_agents = self
            .agents
            .list(&org_id, PageParams::unbounded())
            .await?
            .items;
        let agents: Vec<AgentResponse> = all_agents
            .iter()
            .filter(|a| a.project() == &project_id)
            .map(AgentResponse::from)
            .collect();

        let tasks: Vec<TaskResponse> = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project_id.clone()),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(TaskResponse::from)
            .collect();

        let overviews: Vec<KnowledgeResponse> = self
            .knowledge
            .list(
                KnowledgeFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project_id.clone()),
                    include_org_level: true,
                    kind: Some(KnowledgeKind::Overview),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(KnowledgeResponse::from)
            .collect();

        Ok(ProjectOverview {
            project,
            agents,
            tasks,
            overviews,
        })
    }
}
