use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use orchy_core::{Actor, Document, Edge, Hit, Lease, Message, RecordedEvent, Skill, Task};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDto {
    pub id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub status: String,
    pub priority: String,
    pub namespace: String,
    pub parent: Option<String>,
    pub depends_on: Vec<String>,
    pub assigned_roles: Vec<String>,
    pub claimed_by: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Task> for TaskDto {
    fn from(task: &Task) -> Self {
        Self {
            id: task.id().to_string(),
            title: task.title().to_string(),
            description: task.description().to_owned(),
            acceptance_criteria: task.acceptance_criteria().map(str::to_owned),
            status: task.status().to_string(),
            priority: task.priority().to_string(),
            namespace: task.namespace().to_string(),
            parent: task.parent().map(ToString::to_string),
            depends_on: task.depends_on().iter().map(ToString::to_string).collect(),
            assigned_roles: task
                .assigned_roles()
                .iter()
                .map(ToString::to_string)
                .collect(),
            claimed_by: task.claimed_by().map(ToString::to_string),
            tags: task.tags().iter().map(ToString::to_string).collect(),
            note: task.note().map(str::to_owned),
            created_at: task.created_at(),
            updated_at: task.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentDto {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub namespace: String,
    pub status: Option<String>,
    pub tags: Vec<String>,
    pub frontmatter: serde_json::Value,
    pub body: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Document> for DocumentDto {
    fn from(document: &Document) -> Self {
        let frontmatter = serde_json::Value::Object(
            document
                .frontmatter()
                .iter()
                .map(|(k, v)| (k.to_owned(), v.clone()))
                .collect(),
        );
        Self {
            id: document.id().to_string(),
            kind: document.kind().to_string(),
            title: document.title().to_string(),
            namespace: document.namespace().to_string(),
            status: document.status().map(|s| s.to_string()),
            tags: document.tags().iter().map(ToString::to_string).collect(),
            frontmatter,
            body: document.body().to_string(),
            content_hash: document.content_hash().to_owned(),
            created_at: document.created_at(),
            updated_at: document.updated_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: String,
    pub thread: String,
    pub in_reply_to: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub subject: Option<String>,
    pub body: String,
    pub priority: String,
    pub status: String,
    pub namespace: String,
    pub created_at: DateTime<Utc>,
}

impl From<&Message> for MessageDto {
    fn from(message: &Message) -> Self {
        Self {
            id: message.id().to_string(),
            thread: message.thread().to_string(),
            in_reply_to: message.in_reply_to().map(ToString::to_string),
            from: message.from().to_string(),
            to: message.to().iter().map(ToString::to_string).collect(),
            subject: message.subject().map(ToString::to_string),
            body: message.body().to_string(),
            priority: message.priority().to_string(),
            status: message.status().to_string(),
            namespace: message.namespace().to_string(),
            created_at: message.created_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorDto {
    pub id: String,
    pub alias: String,
    pub machine: String,
    pub display_name: Option<String>,
    pub roles: Vec<String>,
    pub namespace: String,
    pub announced_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

impl From<&Actor> for ActorDto {
    fn from(actor: &Actor) -> Self {
        Self {
            id: actor.id().to_string(),
            alias: actor.id().alias().to_string(),
            machine: actor.id().machine().to_string(),
            display_name: actor.display_name().map(str::to_owned),
            roles: actor.roles().iter().map(ToString::to_string).collect(),
            namespace: actor.namespace().to_string(),
            announced_at: actor.announced_at(),
            last_seen: actor.last_seen(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeDto {
    pub from: String,
    pub to: String,
    pub relation: String,
}

impl From<&Edge> for EdgeDto {
    fn from(edge: &Edge) -> Self {
        Self {
            from: edge.from().to_string(),
            to: edge.to().to_string(),
            relation: edge.relation().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseDto {
    pub resource: String,
    pub holder: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl From<&Lease> for LeaseDto {
    fn from(lease: &Lease) -> Self {
        Self {
            resource: lease.resource().to_string(),
            holder: lease.holder().to_string(),
            acquired_at: lease.acquired_at(),
            expires_at: lease.expires_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HitDto {
    /// `document:<id>` or `skill:<id>`, so a caller knows what it found and can read it back.
    pub entity: String,
    pub kind: String,
    pub id: String,
    pub heading: Option<String>,
    pub excerpt: String,
    pub namespace: String,
    pub updated_at: DateTime<Utc>,
    pub matches: usize,
}

impl From<&Hit> for HitDto {
    fn from(hit: &Hit) -> Self {
        Self {
            entity: hit.entity.to_string(),
            kind: hit.entity.kind().to_string(),
            id: hit.entity.id().to_string(),
            heading: hit.heading.clone(),
            excerpt: hit.excerpt.clone(),
            namespace: hit.namespace.to_string(),
            updated_at: hit.updated_at,
            matches: hit.matches,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventDto {
    pub topic: String,
    pub key: String,
    pub namespace: String,
    pub actor: Option<String>,
    pub payload: serde_json::Value,
    pub recorded_at: DateTime<Utc>,
}

impl From<&RecordedEvent> for EventDto {
    fn from(event: &RecordedEvent) -> Self {
        Self {
            topic: event.topic.clone(),
            key: event.key.clone(),
            namespace: event.namespace.clone(),
            actor: event.actor.clone(),
            payload: event.payload.clone(),
            recorded_at: event.recorded_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageDto<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

impl<T> PageDto<T> {
    pub fn new(items: Vec<T>, total: usize, offset: usize, limit: usize) -> Self {
        Self {
            items,
            total,
            offset,
            limit,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDto {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub namespace: String,
    pub status: String,
    pub tags: Vec<String>,
    pub frontmatter: BTreeMap<String, serde_json::Value>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&Skill> for SkillDto {
    fn from(skill: &Skill) -> Self {
        Self {
            id: skill.id().to_string(),
            name: skill.name().to_string(),
            summary: skill.summary().to_string(),
            namespace: skill.namespace().to_string(),
            status: skill.status().as_str().to_owned(),
            tags: skill.tags().iter().map(ToString::to_string).collect(),
            frontmatter: skill
                .frontmatter()
                .iter()
                .map(|(k, v)| (k.to_owned(), v.clone()))
                .collect(),
            body: skill.body().as_str().to_owned(),
            created_at: skill.created_at(),
            updated_at: skill.updated_at(),
        }
    }
}

/// Everything an agent needs before it does anything: who it is, what it is expected to follow,
/// and what is waiting for it. Assembled in one call so a new agent has no order to get wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefingDto {
    pub actor: ActorDto,
    pub skills: Vec<SkillDto>,
    pub unread: usize,
    pub claimed: Vec<TaskDto>,
    pub next: Option<TaskDto>,
    pub handoff: Option<DocumentDto>,
}
