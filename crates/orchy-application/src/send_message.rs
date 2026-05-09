use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Resource};
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

    pub async fn execute(&self, cmd: SendMessageCommand) -> ApplicationResult<MessageDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let from = if let Ok(id) = AgentId::from_str(&cmd.from_agent_id) {
            id
        } else {
            let alias = Alias::new(&cmd.from_agent_id).map_err(|_| {
                Error::invalid_input(format!("invalid agent id: {}", cmd.from_agent_id))
            })?;
            self.agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Agent,
                    id: cmd.from_agent_id.to_string(),
                })?
                .id()
                .clone()
        };

        let sender = self
            .agents
            .find_by_id(&from)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Agent,
                id: from.to_string(),
            })?;
        if sender.org_id() != &org_id {
            return Err(Error::invalid_input(format!(
                "agent {from} belongs to a different organization"
            ))
            .into());
        }
        if sender.project() != &project {
            return Err(Error::invalid_input(format!(
                "agent {from} belongs to a different project"
            ))
            .into());
        }

        let to = if let Some(alias_str) = cmd.to.strip_prefix('@') {
            let alias = Alias::new(alias_str)?;
            let target_agent = self
                .agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Agent,
                    id: alias_str.to_owned(),
                })?;
            MessageTarget::Agent(target_agent.id().clone())
        } else {
            let target = MessageTarget::parse(&cmd.to)?;
            if let MessageTarget::User(ref uid) = target {
                self.users
                    .find_by_id(uid)
                    .await?
                    .ok_or_else(|| Error::NotFound {
                        resource: Resource::User,
                        id: uid.to_string(),
                    })?;
                let membership = self.memberships.find(uid, &org_id).await?;
                if membership.is_none() {
                    return Err(Error::invalid_input(format!(
                        "user {uid} does not belong to organization {org_id}"
                    ))
                    .into());
                }
            }
            target
        };
        let reply_to = cmd.reply_to.map(|s| s.parse::<MessageId>()).transpose()?;

        let mut msg = Message::new(
            org_id, project, namespace, from, to, cmd.body, reply_to, cmd.refs,
        )?;

        self.messages.save(&mut msg).await?;
        Ok(MessageDto::from(&msg))
    }
}
