mod recipient;

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

pub use recipient::Recipient;

use crate::actor::ActorId;
use crate::body::Body;
use crate::clock::Clock;
use crate::entity_ref::EntityRef;
use crate::error::{DomainError, Result};
use crate::event::{DomainEvent, EventCollector, payload_of, topic};
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;
use crate::priority::Priority;
use crate::title::Title;

#[async_trait]
pub trait MessageStore: Send + Sync {
    async fn get(&self, id: &Id) -> Result<Option<Message>>;
    async fn thread(&self, thread: &Id) -> Result<Vec<Message>>;
    async fn inbox(&self, for_actor: &ActorId, after: Option<&Id>) -> Result<Vec<Message>>;
    async fn sent_by(&self, actor: &ActorId) -> Result<Vec<Message>>;
    async fn save(&self, message: &mut Message) -> Result<()>;

    async fn require(&self, id: &Id) -> Result<Message> {
        self.get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("message", id))
    }
}

pub trait ReadWatermarks: Send + Sync {
    fn watermark(&self, actor: &ActorId) -> Result<Option<Id>>;
    fn advance(&self, actor: &ActorId, to: &Id) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Open,
    Resolved,
}

impl MessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
        }
    }
}

impl fmt::Display for MessageStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MessageStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            other => Err(DomainError::validation(format!(
                "unknown message status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSent {
    pub id: Id,
    pub thread: Id,
    pub namespace: Namespace,
    pub from: ActorId,
    pub to: Vec<Recipient>,
    pub at: DateTime<Utc>,
}

impl DomainEvent for MessageSent {
    fn topic(&self) -> Topic {
        topic("message.sent")
    }
    fn key(&self) -> Id {
        self.id.clone()
    }
    fn namespace(&self) -> Namespace {
        self.namespace.clone()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadResolved {
    pub id: Id,
    pub thread: Id,
    pub namespace: Namespace,
    pub by: ActorId,
    pub at: DateTime<Utc>,
}

impl DomainEvent for ThreadResolved {
    fn topic(&self) -> Topic {
        topic("message.resolved")
    }
    fn key(&self) -> Id {
        self.id.clone()
    }
    fn namespace(&self) -> Namespace {
        self.namespace.clone()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    id: Id,
    thread: Id,
    in_reply_to: Option<Id>,
    from: ActorId,
    to: Vec<Recipient>,
    subject: Option<Title>,
    body: Body,
    priority: Priority,
    status: MessageStatus,
    namespace: Namespace,
    refs: Vec<EntityRef>,
    created_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

#[derive(Debug, Clone)]
pub struct RestoreMessage {
    pub id: Id,
    pub thread: Id,
    pub in_reply_to: Option<Id>,
    pub from: ActorId,
    pub to: Vec<Recipient>,
    pub subject: Option<Title>,
    pub body: Body,
    pub priority: Priority,
    pub status: MessageStatus,
    pub namespace: Namespace,
    pub refs: Vec<EntityRef>,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(restore: RestoreMessage) -> Self {
        Self {
            id: restore.id,
            thread: restore.thread,
            in_reply_to: restore.in_reply_to,
            from: restore.from,
            to: restore.to,
            subject: restore.subject,
            body: restore.body,
            priority: restore.priority,
            status: restore.status,
            namespace: restore.namespace,
            refs: restore.refs,
            created_at: restore.created_at,
            collector: EventCollector::new(),
        }
    }

    pub fn send(
        from: ActorId,
        to: Vec<Recipient>,
        subject: Option<Title>,
        body: Body,
        namespace: Namespace,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Result<Self> {
        if to.is_empty() {
            return Err(DomainError::validation("a message needs a recipient"));
        }
        if body.is_empty() {
            return Err(DomainError::validation("a message needs a body"));
        }
        let now = clock.now();
        let id = Id::generate(ids);
        let mut message = Self::new(RestoreMessage {
            id: id.clone(),
            thread: id.clone(),
            in_reply_to: None,
            from: from.clone(),
            to: to.clone(),
            subject,
            body,
            priority: Priority::default(),
            status: MessageStatus::Open,
            namespace: namespace.clone(),
            refs: Vec::new(),
            created_at: now,
        });
        message.collector.collect(MessageSent {
            id: id.clone(),
            thread: id,
            namespace,
            from,
            to,
            at: now,
        });
        Ok(message)
    }

    pub fn reply(
        &self,
        from: ActorId,
        body: Body,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Result<Self> {
        if body.is_empty() {
            return Err(DomainError::validation("a reply needs a body"));
        }
        let now = clock.now();
        let id = Id::generate(ids);
        let to = vec![Recipient::Instance(self.from.clone())];
        let mut reply = Self::new(RestoreMessage {
            id: id.clone(),
            thread: self.thread.clone(),
            in_reply_to: Some(self.id.clone()),
            from: from.clone(),
            to: to.clone(),
            subject: self.subject.clone(),
            body,
            priority: self.priority,
            status: MessageStatus::Open,
            namespace: self.namespace.clone(),
            refs: Vec::new(),
            created_at: now,
        });
        reply.collector.collect(MessageSent {
            id,
            thread: self.thread.clone(),
            namespace: self.namespace.clone(),
            from,
            to,
            at: now,
        });
        Ok(reply)
    }

    pub fn is_thread_root(&self) -> bool {
        self.id == self.thread
    }

    pub fn resolve(&mut self, by: ActorId, clock: &dyn Clock) -> Result<()> {
        if !self.is_thread_root() {
            return Err(DomainError::conflict(
                "resolution is recorded on the thread root, not on a reply",
            ));
        }
        if self.status == MessageStatus::Resolved {
            return Err(DomainError::conflict("thread is already resolved"));
        }
        self.status = MessageStatus::Resolved;
        self.collector.collect(ThreadResolved {
            id: self.id.clone(),
            thread: self.thread.clone(),
            namespace: self.namespace.clone(),
            by,
            at: clock.now(),
        });
        Ok(())
    }

    pub fn set_priority(&mut self, priority: Priority) {
        self.priority = priority;
    }

    pub fn reference(&mut self, entity: EntityRef) {
        if !self.refs.contains(&entity) {
            self.refs.push(entity);
        }
    }

    pub fn is_unread_for(&self, watermark: Option<&Id>) -> bool {
        match watermark {
            Some(mark) => &self.id > mark,
            None => true,
        }
    }

    pub fn drain_events(&mut self) -> Vec<Box<dyn DomainEvent>> {
        self.collector.drain()
    }

    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn thread(&self) -> &Id {
        &self.thread
    }
    pub fn in_reply_to(&self) -> Option<&Id> {
        self.in_reply_to.as_ref()
    }
    pub fn from(&self) -> &ActorId {
        &self.from
    }
    pub fn to(&self) -> &[Recipient] {
        &self.to
    }
    pub fn subject(&self) -> Option<&Title> {
        self.subject.as_ref()
    }
    pub fn body(&self) -> &Body {
        &self.body
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
    pub fn status(&self) -> MessageStatus {
        self.status
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn refs(&self) -> &[EntityRef] {
        &self.refs
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ulid::Ulid;

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    struct SeqIds(std::sync::atomic::AtomicU64);

    impl IdGenerator for SeqIds {
        fn generate(&self) -> Ulid {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ulid::from_parts(n, n as u128)
        }
    }

    fn clock() -> FixedClock {
        FixedClock(DateTime::from_timestamp(1_700_000_000, 0).unwrap())
    }

    fn actor(alias: &str) -> ActorId {
        ActorId::new(alias, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn send(ids: &SeqIds) -> Message {
        Message::send(
            actor("claude"),
            vec![Recipient::Broadcast],
            Some(Title::new("heads up").unwrap()),
            Body::new("the build is red"),
            Namespace::root(),
            ids,
            &clock(),
        )
        .unwrap()
    }

    fn ids() -> SeqIds {
        SeqIds(std::sync::atomic::AtomicU64::new(1))
    }

    #[test]
    fn a_new_message_opens_its_own_thread() {
        let message = send(&ids());
        assert!(message.is_thread_root());
        assert_eq!(message.id(), message.thread());
        assert_eq!(message.status(), MessageStatus::Open);
        assert_eq!(message.in_reply_to(), None);
    }

    #[test]
    fn sending_requires_a_recipient_and_a_body() {
        let ids = ids();
        assert!(
            Message::send(
                actor("claude"),
                vec![],
                None,
                Body::new("hi"),
                Namespace::root(),
                &ids,
                &clock()
            )
            .is_err(),
            "no recipient"
        );
        assert!(
            Message::send(
                actor("claude"),
                vec![Recipient::Broadcast],
                None,
                Body::new("  "),
                Namespace::root(),
                &ids,
                &clock()
            )
            .is_err(),
            "empty body"
        );
    }

    #[test]
    fn sending_emits_exactly_one_event() {
        let mut message = send(&ids());
        let events = message.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), "message.sent");
    }

    #[test]
    fn a_reply_joins_the_thread_and_addresses_the_sender_directly() {
        let ids = ids();
        let root = send(&ids);
        let reply = root
            .reply(actor("codex"), Body::new("on it"), &ids, &clock())
            .unwrap();

        assert_eq!(reply.thread(), root.thread());
        assert_ne!(reply.id(), root.id(), "a reply is a new file");
        assert_eq!(reply.in_reply_to(), Some(root.id()));
        assert!(!reply.is_thread_root());
        assert_eq!(reply.to(), &[Recipient::Instance(actor("claude"))]);
    }

    #[test]
    fn a_reply_inherits_the_subject_so_threads_stay_readable() {
        let ids = ids();
        let root = send(&ids);
        let reply = root
            .reply(actor("codex"), Body::new("on it"), &ids, &clock())
            .unwrap();
        assert_eq!(reply.subject(), root.subject());
    }

    #[test]
    fn resolution_is_recorded_on_the_thread_root_only() {
        let ids = ids();
        let root = send(&ids);
        let mut reply = root
            .reply(actor("codex"), Body::new("on it"), &ids, &clock())
            .unwrap();

        let err = reply.resolve(actor("codex"), &clock()).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
    }

    #[test]
    fn resolving_twice_is_a_conflict() {
        let mut root = send(&ids());
        root.resolve(actor("codex"), &clock()).unwrap();
        assert_eq!(root.status(), MessageStatus::Resolved);
        assert!(root.resolve(actor("codex"), &clock()).is_err());
    }

    #[test]
    fn resolving_emits_its_own_topic() {
        let mut root = send(&ids());
        root.drain_events();
        root.resolve(actor("codex"), &clock()).unwrap();
        let events = root.drain_events();
        assert_eq!(events[0].topic().as_str(), "message.resolved");
    }

    #[test]
    fn unread_is_a_single_comparison_against_the_local_watermark() {
        let ids = ids();
        let first = send(&ids);
        let second = send(&ids);
        assert!(second.id() > first.id(), "ulids are time-ordered");

        assert!(
            first.is_unread_for(None),
            "no watermark means everything is unread"
        );
        assert!(
            !first.is_unread_for(Some(first.id())),
            "the watermark itself is read"
        );
        assert!(second.is_unread_for(Some(first.id())));
        assert!(!first.is_unread_for(Some(second.id())));
    }

    #[test]
    fn status_round_trips_through_its_wire_name() {
        for status in [MessageStatus::Open, MessageStatus::Resolved] {
            assert_eq!(status.as_str().parse::<MessageStatus>().unwrap(), status);
        }
        assert!("read".parse::<MessageStatus>().is_err());
        assert!("delivered".parse::<MessageStatus>().is_err());
    }

    #[test]
    fn references_are_deduplicated() {
        let mut message = send(&ids());
        let task = EntityRef::task(Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap());
        message.reference(task.clone());
        message.reference(task);
        assert_eq!(message.refs().len(), 1);
    }
}
