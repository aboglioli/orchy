use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStore, MessageTarget};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::{ResourceKind, ResourceRef};

use crate::dto::MessageResponse;
use crate::parse_namespace;

pub struct SendMessageCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub from_agent_id: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
    pub refs: Option<Vec<crate::post_task::ResourceRefInput>>,
}

pub struct SendMessage {
    agents: Arc<dyn AgentStore>,
    messages: Arc<dyn MessageStore>,
}

impl SendMessage {
    pub fn new(agents: Arc<dyn AgentStore>, messages: Arc<dyn MessageStore>) -> Self {
        Self { agents, messages }
    }

    pub async fn execute(&self, cmd: SendMessageCommand) -> Result<Vec<MessageResponse>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let from = AgentId::from_str(&cmd.from_agent_id).map_err(Error::InvalidInput)?;

        let sender = self
            .agents
            .find_by_id(&from)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {from}")))?;
        if sender.org_id() != &org_id {
            return Err(Error::InvalidInput(format!(
                "agent {from} belongs to a different organization"
            )));
        }
        if sender.project() != &project {
            return Err(Error::InvalidInput(format!(
                "agent {from} belongs to a different project"
            )));
        }
        let to = MessageTarget::parse(&cmd.to)?;
        let reply_to = cmd
            .reply_to
            .map(|s| s.parse::<MessageId>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let refs = cmd
            .refs
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let kind = match r.kind.as_str() {
                    "task" => ResourceKind::Task,
                    "knowledge" => ResourceKind::Knowledge,
                    "agent" => ResourceKind::Agent,
                    "message" => ResourceKind::Message,
                    other => {
                        return Err(Error::InvalidInput(format!(
                            "unknown resource kind: {other}"
                        )));
                    }
                };
                let mut rr = ResourceRef::new(kind, r.id);
                if let Some(d) = r.display {
                    rr = rr.with_display(d);
                }
                Ok(rr)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut msg = Message::new(org_id, project, namespace, from, to, cmd.body, reply_to)?;

        for r in refs {
            msg.add_ref(r);
        }

        self.messages.save(&mut msg).await?;
        Ok(vec![MessageResponse::from(&msg)])
    }
}
