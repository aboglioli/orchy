use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStatus, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::message::MessageStore;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::project::ProjectStore;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

use crate::dto::{
    AgentResponse, AgentSummaryResponse, KnowledgeResponse, MessageResponse, ProjectResponse,
    SummaryCounts, TaskResponse,
};

pub struct GetAgentSummaryCommand {
    pub org_id: String,
    pub agent_id: String,
}

pub struct GetAgentSummary {
    agents: Arc<dyn AgentStore>,
    projects: Arc<dyn ProjectStore>,
    messages: Arc<dyn MessageStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl GetAgentSummary {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        projects: Arc<dyn ProjectStore>,
        messages: Arc<dyn MessageStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            agents,
            projects,
            messages,
            tasks,
            knowledge,
        }
    }

    pub async fn execute(&self, cmd: GetAgentSummaryCommand) -> Result<AgentSummaryResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id: AgentId = cmd.agent_id.parse()?;

        let agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        if agent.org_id() != &org_id {
            return Err(Error::NotFound(format!("agent {agent_id}")));
        }

        let project = self
            .projects
            .find_by_id(&org_id, agent.project())
            .await?
            .map(|p| ProjectResponse::from(&p));

        let all_agents = self
            .agents
            .list(&org_id, PageParams::unbounded())
            .await?
            .items;
        let connected_agents: Vec<AgentResponse> = all_agents
            .iter()
            .filter(|a| a.id() != agent.id())
            .filter(|a| a.status() != AgentStatus::Disconnected)
            .filter(|a| a.project() == agent.project())
            .map(AgentResponse::from)
            .collect();

        let inbox: Vec<MessageResponse> = self
            .messages
            .find_unread(
                agent.id(),
                agent.roles(),
                &org_id,
                agent.project(),
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(MessageResponse::from)
            .collect();

        let agent_roles: std::collections::HashSet<&str> =
            agent.roles().iter().map(|r| r.as_str()).collect();

        let pending_tasks: Vec<TaskResponse> = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(agent.project().clone()),
                    namespace: Some(agent.namespace().clone()),
                    status: Some(TaskStatus::Pending),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .filter(|t| {
                t.assigned_roles().is_empty()
                    || t.assigned_roles()
                        .iter()
                        .any(|r| agent_roles.contains(r.as_str()))
            })
            .map(TaskResponse::from)
            .collect();

        let skills: Vec<KnowledgeResponse> = self
            .knowledge
            .list(
                KnowledgeFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(agent.project().clone()),
                    include_org_level: true,
                    kind: Some(KnowledgeKind::Skill),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(KnowledgeResponse::from)
            .collect();

        let handoff_context: Vec<KnowledgeResponse> = self
            .knowledge
            .list(
                KnowledgeFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(agent.project().clone()),
                    include_org_level: false,
                    kind: Some(KnowledgeKind::Context),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .iter()
            .map(KnowledgeResponse::from)
            .collect();

        let counts = SummaryCounts {
            connected_agents: connected_agents.len(),
            inbox_messages: inbox.len(),
            pending_tasks: pending_tasks.len(),
            skills: skills.len(),
        };

        Ok(AgentSummaryResponse {
            agent: AgentResponse::from(&agent),
            project,
            counts,
            connected_agents,
            inbox,
            pending_tasks,
            skills,
            handoff_context,
        })
    }
}
