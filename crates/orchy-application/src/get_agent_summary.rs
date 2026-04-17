use std::sync::Arc;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::message::{Message, MessageStore};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::project::{Project, ProjectStore};
use orchy_core::task::{Task, TaskFilter, TaskStatus, TaskStore};

pub struct GetAgentSummaryCommand {
    pub org_id: String,
    pub agent_id: String,
}

#[derive(serde::Serialize)]
pub struct AgentSummary {
    pub agent: Agent,
    pub project: Option<Project>,
    pub connected_agents: Vec<Agent>,
    pub inbox: Vec<Message>,
    pub pending_tasks: Vec<Task>,
    pub skills: Vec<Knowledge>,
    pub handoff_context: Vec<Knowledge>,
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

    pub async fn execute(&self, cmd: GetAgentSummaryCommand) -> Result<AgentSummary> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id: AgentId = cmd
            .agent_id
            .parse()
            .map_err(|e: String| Error::InvalidInput(e))?;

        let agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        if agent.org_id() != &org_id {
            return Err(Error::NotFound(format!("agent {agent_id}")));
        }

        let project = self.projects.find_by_id(&org_id, agent.project()).await?;

        let all_agents = self
            .agents
            .list(&org_id, PageParams::unbounded())
            .await?
            .items;
        let connected_agents: Vec<Agent> = all_agents
            .into_iter()
            .filter(|a| a.id() != agent.id())
            .filter(|a| a.status() != AgentStatus::Disconnected)
            .filter(|a| a.project() == agent.project())
            .collect();

        let inbox = self
            .messages
            .find_pending(
                agent.id(),
                agent.roles(),
                &org_id,
                agent.project(),
                agent.namespace(),
                PageParams::unbounded(),
            )
            .await
            .map(|p| p.items)
            .unwrap_or_default();

        let pending_tasks = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(agent.project().clone()),
                    status: Some(TaskStatus::Pending),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await
            .map(|p| p.items)
            .unwrap_or_default();

        let skills = self
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
            .await
            .map(|p| p.items)
            .unwrap_or_default();

        let handoff_context = self
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
            .await
            .map(|p| p.items)
            .unwrap_or_default();

        Ok(AgentSummary {
            agent,
            project,
            connected_agents,
            inbox,
            pending_tasks,
            skills,
            handoff_context,
        })
    }
}
