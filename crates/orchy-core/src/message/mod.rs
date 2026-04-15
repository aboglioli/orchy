pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;

use self::events as message_events;

pub trait MessageStore: Send + Sync {
    fn save(&self, message: &mut Message) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &MessageId) -> impl Future<Output = Result<Option<Message>>> + Send;
    fn find_pending(
        &self,
        agent: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<Message>>> + Send;
    fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<Message>>> + Send;
    fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> impl Future<Output = Result<Vec<Message>>> + Send;
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
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
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
    ) -> Self {
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
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            msg.org_id.as_str(),
            message_events::NAMESPACE,
            message_events::TOPIC_SENT,
            Payload::from_json(&message_events::MessageSentPayload {
                org_id: msg.org_id.to_string(),
                message_id: msg.id.to_string(),
                project: msg.project.to_string(),
                namespace: msg.namespace.to_string(),
                from: msg.from.to_string(),
                to: msg.to.to_string(),
                body: msg.body.clone(),
                reply_to: msg.reply_to.map(|id| id.to_string()),
            })
            .unwrap(),
        )
        .map(|e| msg.collector.collect(e));

        msg
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
            collector: EventCollector::new(),
        }
    }

    pub fn reply(&self, from: AgentId, body: String) -> Self {
        Self::new(
            self.org_id.clone(),
            self.project.clone(),
            self.namespace.clone(),
            from,
            MessageTarget::Agent(self.from),
            body,
            Some(self.id),
        )
    }

    pub fn deliver(&mut self) {
        if self.status == MessageStatus::Pending {
            self.status = MessageStatus::Delivered;

            let _ = Event::create(
                self.org_id.as_str(),
                message_events::NAMESPACE,
                message_events::TOPIC_DELIVERED,
                Payload::from_json(&message_events::MessageDeliveredPayload {
                    org_id: self.org_id.to_string(),
                    message_id: self.id.to_string(),
                    from: self.from.to_string(),
                    to: self.to.to_string(),
                    status: "delivered".to_string(),
                })
                .unwrap(),
            )
            .map(|e| self.collector.collect(e));
        }
    }

    pub fn mark_read(&mut self) {
        self.status = MessageStatus::Read;

        let _ = Event::create(
            self.org_id.as_str(),
            message_events::NAMESPACE,
            message_events::TOPIC_READ,
            Payload::from_json(&message_events::MessageReadPayload {
                org_id: self.org_id.to_string(),
                message_id: self.id.to_string(),
                from: self.from.to_string(),
                to: self.to.to_string(),
                status: "read".to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
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
    pub fn from(&self) -> AgentId {
        self.from
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
        );
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
        );
        msg.deliver();
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
        );
        msg.mark_read();
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
            sender,
            MessageTarget::Agent(receiver),
            "hello".into(),
            None,
        );
        let reply = original.reply(receiver, "hey back".into());
        assert_eq!(reply.reply_to(), Some(original.id()));
        assert_eq!(reply.from(), receiver);
        assert_eq!(reply.to(), &MessageTarget::Agent(sender));
    }
}
