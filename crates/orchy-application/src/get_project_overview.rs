use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::project::{Project, ProjectStore};
use orchy_core::task::{TaskFilter, TaskStore};

use crate::dto::{
    AgentResponse, KnowledgeResponse, ProjectOverviewResponse, ProjectResponse, TaskResponse,
};
use crate::parse_namespace;

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

    pub async fn execute(&self, cmd: GetProjectOverviewCommand) -> Result<ProjectOverviewResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project_id =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let project = match self.projects.find_by_id(&org_id, &project_id).await? {
            Some(project) => project,
            None => Project::new(org_id.clone(), project_id.clone(), String::new())?,
        };
        let project = Some(ProjectResponse::from(&project));

        let all_agents = self
            .agents
            .list(&org_id, PageParams::unbounded())
            .await?
            .items;
        let agents: Vec<AgentResponse> = all_agents
            .iter()
            .filter(|a| a.project() == &project_id)
            .filter(|a| {
                namespace
                    .as_ref()
                    .map(|ns| a.namespace().starts_with(ns))
                    .unwrap_or(true)
            })
            .map(AgentResponse::from)
            .collect();

        let tasks: Vec<TaskResponse> = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project_id.clone()),
                    namespace: namespace.clone(),
                    include_archived: None,
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(TaskResponse::from)
            .collect();

        let skills: Vec<KnowledgeResponse> = self
            .knowledge
            .list(
                KnowledgeFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project_id.clone()),
                    include_org_level: true,
                    kind: Some(KnowledgeKind::Skill),
                    namespace: namespace.clone(),
                    include_archived: None,
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(KnowledgeResponse::from)
            .collect();

        let overviews: Vec<KnowledgeResponse> = self
            .knowledge
            .list(
                KnowledgeFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project_id.clone()),
                    include_org_level: true,
                    kind: Some(KnowledgeKind::Overview),
                    namespace: namespace.clone(),
                    include_archived: None,
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(KnowledgeResponse::from)
            .collect();

        Ok(ProjectOverviewResponse {
            project,
            agents,
            tasks,
            skills,
            overviews,
        })
    }
}
