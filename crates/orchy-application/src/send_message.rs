use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStore, MessageTarget};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceRef;
use orchy_core::user::{OrgMembershipStore, UserStore};

use crate::dto::MessageDto;
use crate::parse_namespace;

pub struct SendMessageCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub from_agent_id: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
    pub refs: Vec<ResourceRef>,
}

pub struct SendMessage {
    agents: Arc<dyn AgentStore>,
    messages: Arc<dyn MessageStore>,
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl SendMessage {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        messages: Arc<dyn MessageStore>,
        users: Arc<dyn UserStore>,
        memberships: Arc<dyn OrgMembershipStore>,
    ) -> Self {
        Self {
            agents,
            messages,
            users,
            memberships,
        }
    }

    pub async fn execute(&self, cmd: SendMessageCommand) -> Result<MessageDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let from = if let Ok(id) = AgentId::from_str(&cmd.from_agent_id) {
            id
        } else {
            let alias = Alias::new(&cmd.from_agent_id).map_err(|_| {
                Error::InvalidInput(format!("invalid agent id: {}", cmd.from_agent_id))
            })?;
            self.agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound(format!("agent alias @{}", cmd.from_agent_id)))?
                .id()
                .clone()
        };

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

        let to = if let Some(alias_str) = cmd.to.strip_prefix('@') {
            let alias = Alias::new(alias_str)?;
            let target_agent = self
                .agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound(format!("agent alias @{alias_str}")))?;
            MessageTarget::Agent(target_agent.id().clone())
        } else {
            let target = MessageTarget::parse(&cmd.to)?;
            if let MessageTarget::User(ref uid) = target {
                self.users
                    .find_by_id(uid)
                    .await?
                    .ok_or_else(|| Error::NotFound(format!("user {uid}")))?;
                let membership = self.memberships.find(uid, &org_id).await?;
                if membership.is_none() {
                    return Err(Error::InvalidInput(format!(
                        "user {uid} does not belong to organization {org_id}"
                    )));
                }
            }
            target
        };
        let reply_to = cmd
            .reply_to
            .map(|s| s.parse::<MessageId>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut msg = Message::new(
            org_id, project, namespace, from, to, cmd.body, reply_to, cmd.refs,
        )?;

        self.messages.save(&mut msg).await?;
        Ok(MessageDto::from(&msg))
    }
}
