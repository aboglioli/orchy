use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStore, MessageTarget};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct SendMessageCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub from_agent_id: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
}

pub struct SendMessage {
    messages: Arc<dyn MessageStore>,
    agents: Arc<dyn AgentStore>,
}

impl SendMessage {
    pub fn new(messages: Arc<dyn MessageStore>, agents: Arc<dyn AgentStore>) -> Self {
        Self { messages, agents }
    }

    pub async fn execute(&self, cmd: SendMessageCommand) -> Result<Vec<Message>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let from = AgentId::from_str(&cmd.from_agent_id).map_err(Error::InvalidInput)?;
        let to = MessageTarget::parse(&cmd.to)?;
        let reply_to = cmd
            .reply_to
            .map(|s| s.parse::<MessageId>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let targets = match &to {
            MessageTarget::Agent(_) => {
                let mut msg =
                    Message::new(org_id, project, namespace, from, to, cmd.body, reply_to)?;
                self.messages.save(&mut msg).await?;
                return Ok(vec![msg]);
            }
            MessageTarget::Broadcast => {
                let mut msg =
                    Message::new(org_id, project, namespace, from, to, cmd.body, reply_to)?;
                self.messages.save(&mut msg).await?;
                return Ok(vec![msg]);
            }
            MessageTarget::Role(role) => {
                let agents = self.agents.list(&org_id).await?;
                agents
                    .into_iter()
                    .filter(|a| a.project() == &project)
                    .filter(|a| a.roles().iter().any(|r| r == role))
                    .map(|a| a.id().clone())
                    .collect::<Vec<_>>()
            }
        };

        let mut sent = Vec::with_capacity(targets.len());
        for target_id in targets {
            let mut msg = Message::new(
                org_id.clone(),
                project.clone(),
                namespace.clone(),
                from.clone(),
                MessageTarget::Agent(target_id),
                cmd.body.clone(),
                reply_to,
            )?;
            self.messages.save(&mut msg).await?;
            sent.push(msg);
        }
        Ok(sent)
    }
}
