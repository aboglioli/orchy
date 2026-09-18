use chrono::{DateTime, Utc};
use orchy_core::{
    Actor, ActorId, Body, Document, DomainError, Frontmatter, Id, Kind, Message, MessageStatus,
    Namespace, Priority, Recipient, RestoreDocument, RestoreMessage, RestoreTask, Result, Role,
    Status, Tag, Task, TaskStatus, Title,
};
use serde_json::{Value, json};

use crate::markdown::MarkdownFile;

const TASK_KEYS: [&str; 14] = [
    "id",
    "type",
    "title",
    "status",
    "priority",
    "namespace",
    "parent",
    "depends_on",
    "roles",
    "claimed_by",
    "claimed_at",
    "tags",
    "created",
    "updated",
];

const MESSAGE_KEYS: [&str; 11] = [
    "id",
    "type",
    "thread",
    "in_reply_to",
    "from",
    "to",
    "subject",
    "priority",
    "status",
    "namespace",
    "created",
];

const DOCUMENT_KEYS: [&str; 7] = [
    "id",
    "type",
    "title",
    "namespace",
    "status",
    "tags",
    "created",
];

const ACTOR_KEYS: [&str; 7] = [
    "id",
    "type",
    "display_name",
    "roles",
    "namespace",
    "announced",
    "last_seen",
];

fn missing(field: &str, key: &str) -> DomainError {
    DomainError::validation(format!("`{key}` has no `{field}` in its frontmatter"))
}

fn timestamp(frontmatter: &Frontmatter, field: &str) -> Option<DateTime<Utc>> {
    frontmatter
        .string(field)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc))
}

fn stamp(at: DateTime<Utc>) -> Value {
    json!(at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

fn list(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

pub fn task_to_markdown(task: &Task, carried: Frontmatter) -> MarkdownFile {
    let mut frontmatter = Frontmatter::new();
    frontmatter.set("id", json!(task.id().to_string()));
    frontmatter.set("type", json!("task"));
    frontmatter.set("title", json!(task.title().to_string()));
    frontmatter.set("status", json!(task.status().as_str()));
    frontmatter.set("priority", json!(task.priority().as_str()));
    frontmatter.set("namespace", json!(task.namespace().to_string()));

    if let Some(parent) = task.parent() {
        frontmatter.set("parent", json!(parent.to_string()));
    }
    if !task.depends_on().is_empty() {
        frontmatter.set(
            "depends_on",
            list(task.depends_on().iter().map(ToString::to_string)),
        );
    }
    if !task.assigned_roles().is_empty() {
        frontmatter.set(
            "roles",
            list(task.assigned_roles().iter().map(ToString::to_string)),
        );
    }
    if let Some(holder) = task.claimed_by() {
        frontmatter.set("claimed_by", json!(holder.to_string()));
    }
    if let Some(at) = task.claimed_at() {
        frontmatter.set("claimed_at", stamp(at));
    }
    if !task.tags().is_empty() {
        frontmatter.set("tags", list(task.tags().iter().map(ToString::to_string)));
    }
    frontmatter.set("created", stamp(task.created_at()));
    frontmatter.set("updated", stamp(task.updated_at()));

    for (key, value) in carried.iter() {
        if !TASK_KEYS.contains(&key) {
            frontmatter.set(key, value.clone());
        }
    }

    let body = match task.acceptance_criteria() {
        Some(criteria) => Body::new(format!(
            "{}\n\n## Acceptance\n\n{criteria}",
            task.description()
        )),
        None => Body::new(task.description()),
    };

    MarkdownFile { frontmatter, body }
}

pub fn task_from_markdown(file: &MarkdownFile, key: &str) -> Result<Task> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id", key))?)?;
    let title = Title::new(fm.string("title").ok_or_else(|| missing("title", key))?)?;
    let status: TaskStatus = fm
        .string("status")
        .ok_or_else(|| missing("status", key))?
        .parse()?;

    let (description, acceptance_criteria) = split_acceptance(file.body.as_str());

    Ok(Task::new(RestoreTask {
        id: id.clone(),
        title,
        description,
        acceptance_criteria,
        status,
        priority: fm
            .string("priority")
            .map(str::parse::<Priority>)
            .transpose()?
            .unwrap_or_default(),
        namespace: fm
            .string("namespace")
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default(),
        parent: fm.string("parent").map(Id::new).transpose()?,
        depends_on: fm
            .strings("depends_on")
            .iter()
            .map(Id::new)
            .collect::<Result<Vec<_>>>()?,
        assigned_roles: fm
            .strings("roles")
            .iter()
            .map(Role::new)
            .collect::<Result<Vec<_>>>()?,
        claimed_by: fm
            .string("claimed_by")
            .map(str::parse::<ActorId>)
            .transpose()?,
        claimed_at: timestamp(fm, "claimed_at"),
        tags: fm
            .strings("tags")
            .iter()
            .map(Tag::new)
            .collect::<Result<Vec<_>>>()?,
        refs: Vec::new(),
        note: None,
        created_at: timestamp(fm, "created").unwrap_or_else(|| id.created_at()),
        updated_at: timestamp(fm, "updated").unwrap_or_else(|| id.created_at()),
    }))
}

fn split_acceptance(body: &str) -> (String, Option<String>) {
    const HEADING: &str = "## Acceptance";
    match body.split_once(HEADING) {
        Some((description, criteria)) => {
            let criteria = criteria.trim();
            (
                description.trim().to_owned(),
                (!criteria.is_empty()).then(|| criteria.to_owned()),
            )
        }
        None => (body.trim().to_owned(), None),
    }
}

pub fn document_to_markdown(document: &Document) -> MarkdownFile {
    let mut frontmatter = Frontmatter::new();
    frontmatter.set("id", json!(document.id().to_string()));
    frontmatter.set("type", json!(document.kind().to_string()));
    frontmatter.set("title", json!(document.title().to_string()));
    frontmatter.set("namespace", json!(document.namespace().to_string()));
    if let Some(status) = document.status() {
        frontmatter.set("status", json!(status.to_string()));
    }
    if !document.tags().is_empty() {
        frontmatter.set(
            "tags",
            list(document.tags().iter().map(ToString::to_string)),
        );
    }
    frontmatter.set("created", stamp(document.created_at()));

    for (key, value) in document.frontmatter().iter() {
        if !DOCUMENT_KEYS.contains(&key) {
            frontmatter.set(key, value.clone());
        }
    }

    MarkdownFile {
        frontmatter,
        body: document.body().clone(),
    }
}

pub fn document_from_markdown(file: &MarkdownFile, key: &str) -> Result<Document> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id", key))?)?;
    let kind = Kind::new(fm.string("type").ok_or_else(|| missing("type", key))?)?;
    let title = Title::new(
        fm.string("title")
            .map(str::to_owned)
            .unwrap_or_else(|| id.to_string()),
    )?;

    let mut carried = Frontmatter::new();
    for (name, value) in fm.iter() {
        if !DOCUMENT_KEYS.contains(&name) {
            carried.set(name, value.clone());
        }
    }

    Ok(Document::new(RestoreDocument {
        id: id.clone(),
        kind,
        title,
        namespace: fm
            .string("namespace")
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default(),
        status: fm.string("status").map(Status::new).transpose()?,
        tags: fm
            .strings("tags")
            .iter()
            .map(Tag::new)
            .collect::<Result<Vec<_>>>()?,
        frontmatter: carried,
        body: file.body.clone(),
        created_at: timestamp(fm, "created").unwrap_or_else(|| id.created_at()),
        updated_at: timestamp(fm, "updated").unwrap_or_else(|| id.created_at()),
    }))
}

pub fn message_to_markdown(message: &Message) -> MarkdownFile {
    let mut frontmatter = Frontmatter::new();
    frontmatter.set("id", json!(message.id().to_string()));
    frontmatter.set("type", json!("message"));
    frontmatter.set("thread", json!(message.thread().to_string()));
    if let Some(parent) = message.in_reply_to() {
        frontmatter.set("in_reply_to", json!(parent.to_string()));
    }
    frontmatter.set("from", json!(message.from().to_string()));
    frontmatter.set("to", list(message.to().iter().map(ToString::to_string)));
    if let Some(subject) = message.subject() {
        frontmatter.set("subject", json!(subject.to_string()));
    }
    frontmatter.set("priority", json!(message.priority().as_str()));
    frontmatter.set("status", json!(message.status().as_str()));
    frontmatter.set("namespace", json!(message.namespace().to_string()));
    frontmatter.set("created", stamp(message.created_at()));

    MarkdownFile {
        frontmatter,
        body: message.body().clone(),
    }
}

pub fn message_from_markdown(file: &MarkdownFile, key: &str) -> Result<Message> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id", key))?)?;
    let thread = fm
        .string("thread")
        .map(Id::new)
        .transpose()?
        .unwrap_or_else(|| id.clone());

    Ok(Message::new(RestoreMessage {
        id: id.clone(),
        thread,
        in_reply_to: fm.string("in_reply_to").map(Id::new).transpose()?,
        from: fm
            .string("from")
            .ok_or_else(|| missing("from", key))?
            .parse()?,
        to: fm
            .strings("to")
            .iter()
            .map(|r| r.parse::<Recipient>())
            .collect::<Result<Vec<_>>>()?,
        subject: fm.string("subject").map(Title::new).transpose()?,
        body: file.body.clone(),
        priority: fm
            .string("priority")
            .map(str::parse::<Priority>)
            .transpose()?
            .unwrap_or_default(),
        status: fm
            .string("status")
            .map(str::parse::<MessageStatus>)
            .transpose()?
            .unwrap_or(MessageStatus::Open),
        namespace: fm
            .string("namespace")
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default(),
        refs: Vec::new(),
        created_at: timestamp(fm, "created").unwrap_or_else(|| id.created_at()),
    }))
}

pub fn actor_to_markdown(actor: &Actor, carried: Frontmatter) -> MarkdownFile {
    let mut frontmatter = Frontmatter::new();
    frontmatter.set("id", json!(actor.id().to_string()));
    frontmatter.set("type", json!("agent"));
    if let Some(name) = actor.display_name() {
        frontmatter.set("display_name", json!(name));
    }
    if !actor.roles().is_empty() {
        frontmatter.set("roles", list(actor.roles().iter().map(ToString::to_string)));
    }
    frontmatter.set("namespace", json!(actor.namespace().to_string()));
    frontmatter.set("announced", stamp(actor.announced_at()));
    frontmatter.set("last_seen", stamp(actor.last_seen()));

    for (key, value) in carried.iter() {
        if !ACTOR_KEYS.contains(&key) {
            frontmatter.set(key, value.clone());
        }
    }

    MarkdownFile {
        frontmatter,
        body: Body::new(""),
    }
}

pub fn actor_from_markdown(file: &MarkdownFile, key: &str) -> Result<Actor> {
    let fm = &file.frontmatter;
    let id: ActorId = fm.string("id").ok_or_else(|| missing("id", key))?.parse()?;
    let announced = timestamp(fm, "announced").unwrap_or_else(Utc::now);

    Ok(Actor::new(
        id,
        fm.string("display_name").map(str::to_owned),
        fm.strings("roles")
            .iter()
            .map(Role::new)
            .collect::<Result<Vec<_>>>()?,
        fm.string("namespace")
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default(),
        announced,
        timestamp(fm, "last_seen").unwrap_or(announced),
    ))
}

pub fn kind_of(file: &MarkdownFile) -> Option<&str> {
    file.frontmatter.string("type")
}

pub fn id_of(file: &MarkdownFile) -> Option<&str> {
    file.frontmatter.string("id")
}

pub fn carried_frontmatter(file: &MarkdownFile) -> Frontmatter {
    file.frontmatter.clone()
}

pub const MESSAGE_FIELDS: [&str; 11] = MESSAGE_KEYS;
