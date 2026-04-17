use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageStore, MessageTarget};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{ReviewRequest, ReviewStore, TaskId, TaskStore};

use crate::parse_namespace;

pub struct RequestReviewCommand {
    pub task_id: String,
    pub requester_agent_id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub reviewer_agent: Option<String>,
    pub reviewer_role: Option<String>,
}

pub struct RequestReview {
    tasks: Arc<dyn TaskStore>,
    reviews: Arc<dyn ReviewStore>,
    messages: Arc<dyn MessageStore>,
}

impl RequestReview {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        reviews: Arc<dyn ReviewStore>,
        messages: Arc<dyn MessageStore>,
    ) -> Self {
        Self {
            tasks,
            reviews,
            messages,
        }
    }

    pub async fn execute(&self, cmd: RequestReviewCommand) -> Result<ReviewRequest> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let requester = AgentId::from_str(&cmd.requester_agent_id).map_err(Error::InvalidInput)?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let reviewer = cmd
            .reviewer_agent
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        self.tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let mut review = ReviewRequest::new(
            task_id,
            org_id.clone(),
            project.clone(),
            namespace.clone(),
            requester.clone(),
            reviewer.clone(),
            cmd.reviewer_role.clone(),
        )?;
        self.reviews.save(&mut review).await?;

        let body = format!(
            "Review requested for task {} (review {})",
            review.task_id(),
            review.id()
        );
        let target = if let Some(agent) = reviewer {
            MessageTarget::Agent(agent)
        } else if let Some(role) = cmd.reviewer_role {
            MessageTarget::Role(role)
        } else {
            MessageTarget::Broadcast
        };
        let mut msg = Message::new(org_id, project, namespace, requester, target, body, None)?;
        let _ = self.messages.save(&mut msg).await;

        Ok(review)
    }
}
