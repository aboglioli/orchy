pub mod events;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use self::events as message_events;
use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};
use crate::resource_ref::ResourceRef;

#[async_trait::async_trait]
pub trait MessageStore: Send + Sync {
    async fn save(&self, message: &mut Message) -> Result<()>;
    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>>;
    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>>;
    async fn mark_read_for_agent(&self, message_id: &MessageId, agent: &AgentId) -> Result<()>;
    async fn find_pending(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>>;
    async fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>>;
    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MessageId {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| Error::invalid_input(format!("invalid message id: {s}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum MessageTarget {
    Agent(AgentId),
    Role(String),
    Broadcast,
}

impl MessageTarget {
    pub fn parse(s: &str) -> Result<Self> {
        if s == "broadcast" {
            return Ok(MessageTarget::Broadcast);
        }
        if let Some(role) = s.strip_prefix("role:") {
            if role.is_empty() {
                return Err(Error::InvalidInput(
                    "role name must not be empty".to_string(),
                ));
            }
            return Ok(MessageTarget::Role(role.to_string()));
        }
        match AgentId::from_str(s) {
            Ok(id) => Ok(MessageTarget::Agent(id)),
            Err(_) => Err(Error::InvalidInput(format!(
                "cannot parse message target: '{s}'"
            ))),
        }
    }
}

impl TryFrom<String> for MessageTarget {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

impl From<MessageTarget> for String {
    fn from(t: MessageTarget) -> Self {
        t.to_string()
    }
}

impl fmt::Display for MessageTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageTarget::Broadcast => write!(f, "broadcast"),
            MessageTarget::Role(r) => write!(f, "role:{r}"),
            MessageTarget::Agent(id) => write!(f, "{id}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    Pending,
    Delivered,
    Read,
}

impl FromStr for MessageStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(MessageStatus::Pending),
            "delivered" => Ok(MessageStatus::Delivered),
            "read" => Ok(MessageStatus::Read),
            other => Err(format!("unknown message status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    id: MessageId,
    org_id: OrganizationId,
    project: ProjectId,
    namespace: Namespace,
    from: AgentId,
    to: MessageTarget,
    body: String,
    reply_to: Option<MessageId>,
    status: MessageStatus,
    created_at: DateTime<Utc>,
    refs: Vec<ResourceRef>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Message {
    pub fn new(
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        from: AgentId,
        to: MessageTarget,
        body: String,
        reply_to: Option<MessageId>,
        refs: Vec<ResourceRef>,
    ) -> Result<Self> {
        let mut msg = Self {
            id: MessageId::new(),
            org_id,
            project,
            namespace,
            from,
            to,
            body,
            reply_to,
            status: MessageStatus::Pending,
            created_at: Utc::now(),
            refs,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&message_events::MessageSentPayload {
            org_id: msg.org_id.to_string(),
            message_id: msg.id.to_string(),
            project: msg.project.to_string(),
            namespace: msg.namespace.to_string(),
            from: msg.from.to_string(),
            to: msg.to.to_string(),
            body: msg.body.clone(),
            reply_to: msg.reply_to.map(|id| id.to_string()),
            refs: msg
                .refs
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or_default())
                .collect(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            msg.org_id.as_str(),
            message_events::NAMESPACE,
            message_events::TOPIC_SENT,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        msg.collector.collect(event);

        Ok(msg)
    }

    pub fn restore(r: RestoreMessage) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            project: r.project,
            namespace: r.namespace,
            from: r.from,
            to: r.to,
            body: r.body,
            reply_to: r.reply_to,
            status: r.status,
            created_at: r.created_at,
            refs: r.refs,
            collector: EventCollector::new(),
        }
    }

    pub fn reply(&self, from: AgentId, body: String) -> Result<Self> {
        Self::new(
            self.org_id.clone(),
            self.project.clone(),
            self.namespace.clone(),
            from,
            MessageTarget::Agent(self.from.clone()),
            body,
            Some(self.id),
            vec![], // Replies don't inherit refs
        )
    }

    pub fn deliver(&mut self) -> Result<()> {
        if self.status == MessageStatus::Pending {
            self.status = MessageStatus::Delivered;

            let payload = Payload::from_json(&message_events::MessageDeliveredPayload {
                org_id: self.org_id.to_string(),
                message_id: self.id.to_string(),
                from: self.from.to_string(),
                to: self.to.to_string(),
                status: "delivered".to_string(),
            })
            .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
            let event = Event::create(
                self.org_id.as_str(),
                message_events::NAMESPACE,
                message_events::TOPIC_DELIVERED,
                payload,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?;
            self.collector.collect(event);
        }
        Ok(())
    }

    pub fn mark_read(&mut self) -> Result<()> {
        self.status = MessageStatus::Read;

        let payload = Payload::from_json(&message_events::MessageReadPayload {
            org_id: self.org_id.to_string(),
            message_id: self.id.to_string(),
            from: self.from.to_string(),
            to: self.to.to_string(),
            status: "read".to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            message_events::NAMESPACE,
            message_events::TOPIC_READ,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn is_directed_to(&self, agent: &AgentId) -> bool {
        match &self.to {
            MessageTarget::Agent(id) => id == agent,
            _ => false,
        }
    }

    pub fn is_broadcast(&self) -> bool {
        matches!(self.to, MessageTarget::Broadcast)
    }

    pub fn is_role_targeted(&self) -> bool {
        matches!(self.to, MessageTarget::Role(_))
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> MessageId {
        self.id
    }
    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn from(&self) -> &AgentId {
        &self.from
    }
    pub fn to(&self) -> &MessageTarget {
        &self.to
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply_to
    }
    pub fn status(&self) -> MessageStatus {
        self.status
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn refs(&self) -> &[ResourceRef] {
        &self.refs
    }
}

pub struct RestoreMessage {
    pub id: MessageId,
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub reply_to: Option<MessageId>,
    pub status: MessageStatus,
    pub created_at: DateTime<Utc>,
    pub refs: Vec<ResourceRef>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchy_events::OrganizationId;

    fn test_org() -> OrganizationId {
        OrganizationId::new("test").unwrap()
    }

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    #[test]
    fn parse_broadcast() {
        let t = MessageTarget::parse("broadcast").unwrap();
        assert_eq!(t, MessageTarget::Broadcast);
    }

    #[test]
    fn parse_role() {
        let t = MessageTarget::parse("role:reviewer").unwrap();
        assert_eq!(t, MessageTarget::Role("reviewer".to_string()));
    }

    #[test]
    fn parse_agent_id() {
        let id = AgentId::new();
        let s = id.to_string();
        let t = MessageTarget::parse(&s).unwrap();
        assert_eq!(t, MessageTarget::Agent(id));
    }

    #[test]
    fn empty_role_fails() {
        let result = MessageTarget::parse("role:");
        assert!(result.is_err());
    }

    #[test]
    fn new_message_is_pending() {
        let msg = Message::new(
            test_org(),
            test_project(),
            Namespace::root(),
            AgentId::new(),
            MessageTarget::Broadcast,
            "hi".into(),
            None,
            vec![],
        )
        .unwrap();
        assert_eq!(msg.status(), MessageStatus::Pending);
    }

    #[test]
    fn deliver_transitions_to_delivered() {
        let mut msg = Message::new(
            test_org(),
            test_project(),
            Namespace::root(),
            AgentId::new(),
            MessageTarget::Broadcast,
            "hi".into(),
            None,
            vec![],
        )
        .unwrap();
        msg.deliver().unwrap();
        assert_eq!(msg.status(), MessageStatus::Delivered);
    }

    #[test]
    fn mark_read_transitions() {
        let mut msg = Message::new(
            test_org(),
            test_project(),
            Namespace::root(),
            AgentId::new(),
            MessageTarget::Broadcast,
            "hi".into(),
            None,
            vec![],
        )
        .unwrap();
        msg.mark_read().unwrap();
        assert_eq!(msg.status(), MessageStatus::Read);
    }

    #[test]
    fn reply_creates_threaded_message() {
        let sender = AgentId::new();
        let receiver = AgentId::new();
        let original = Message::new(
            test_org(),
            test_project(),
            Namespace::root(),
            sender.clone(),
            MessageTarget::Agent(receiver.clone()),
            "hello".into(),
            None,
            vec![],
        )
        .unwrap();
        let reply = original.reply(receiver.clone(), "hey back".into()).unwrap();
        assert_eq!(reply.reply_to(), Some(original.id()));
        assert_eq!(reply.from(), &receiver);
        assert_eq!(reply.to(), &MessageTarget::Agent(sender.clone()));
    }

    #[test]
    fn message_preserves_refs() {
        use crate::resource_ref::ResourceRef;
        let org = OrganizationId::new("org").unwrap();
        let project = ProjectId::try_from("proj").unwrap();
        let namespace = Namespace::root();
        let from = AgentId::new();
        let refs = vec![
            ResourceRef::task("task-1"),
            ResourceRef::knowledge("auth/decision").with_display("JWT decision"),
        ];
        let msg = Message::new(
            org,
            project,
            namespace,
            from,
            MessageTarget::Broadcast,
            "check this out".to_string(),
            None,
            refs.clone(),
        )
        .unwrap();
        assert_eq!(msg.refs(), &refs);
    }
}
