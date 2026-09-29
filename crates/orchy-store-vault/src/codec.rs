use chrono::{DateTime, Utc};
use orchy_core::{
    Actor, ActorId, Body, Document, DocumentStatus, DomainError, EntityKind, EntityRef,
    Frontmatter, Id, Kind, Message, MessageStatus, Namespace, Priority, Problem, ProblemKind,
    Recipient, RestoreDocument, RestoreMessage, RestoreSkill, RestoreTask, Result, Role, Skill,
    SkillName, SkillStatus, Summary, Tag, Task, TaskStatus, Title,
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

const DOCUMENT_KEYS: [&str; 8] = [
    "id",
    "type",
    "title",
    "namespace",
    "status",
    "tags",
    "created",
    "updated",
];

const SKILL_KEYS: [&str; 8] = [
    "id",
    "type",
    "name",
    "summary",
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

const MISSING: &str = "no `";

fn missing(field: &str) -> DomainError {
    DomainError::validation(format!("{MISSING}{field}` in its frontmatter"))
}

/// Names the file in a decoding error, which is the one thing a person needs to fix it.
pub fn at(key: &str) -> impl Fn(DomainError) -> DomainError + '_ {
    move |e| match e {
        DomainError::Validation(detail) | DomainError::UnknownType(detail) => {
            DomainError::validation(format!("{key}: {detail}"))
        }
        other => other,
    }
}

/// Why a file the vault indexed could not be read as the entity it claims to be.
pub fn problem(key: &str, file: &MarkdownFile, e: &DomainError) -> Problem {
    let id = id_of(file).and_then(|raw| Id::new(raw).ok());
    let kind = match e {
        DomainError::UnknownType(_) => ProblemKind::UnknownType,
        DomainError::Validation(detail) if detail.starts_with(MISSING) => ProblemKind::MissingField,
        _ => ProblemKind::InvalidField,
    };
    let detail = match e {
        DomainError::UnknownType(name) => format!("`{name}` is not a registered type"),
        other => other.to_string(),
    };
    Problem::new(kind, key, id, detail)
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

fn task_ref(id: &Id) -> String {
    EntityRef::task(id.clone()).to_string()
}

fn task_id(reference: impl AsRef<str>) -> Result<Id> {
    EntityRef::parse_or_assume(reference.as_ref(), Some(EntityKind::Task))
        .map(|entity| entity.id().clone())
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
        frontmatter.set("parent", json!(task_ref(parent)));
    }
    if !task.depends_on().is_empty() {
        frontmatter.set("depends_on", list(task.depends_on().iter().map(task_ref)));
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

    let mut body = task.description().to_owned();
    for (heading, text) in [
        (ACCEPTANCE, task.acceptance_criteria()),
        (OUTCOME, task.note()),
    ] {
        if let Some(text) = text {
            body.push_str(&format!("\n\n{heading}\n\n{text}"));
        }
    }

    MarkdownFile {
        frontmatter,
        body: Body::new(body),
    }
}

pub fn task_from_markdown(file: &MarkdownFile) -> Result<Task> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id"))?)?;
    let title = Title::new(fm.string("title").ok_or_else(|| missing("title"))?)?;
    let status: TaskStatus = fm
        .string("status")
        .ok_or_else(|| missing("status"))?
        .parse()?;

    let TaskBody {
        description,
        acceptance_criteria,
        note,
    } = split_task_body(file.body.as_str());

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
        parent: fm.string("parent").map(task_id).transpose()?,
        depends_on: fm
            .strings("depends_on")
            .iter()
            .map(task_id)
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
        note,
        created_at: timestamp(fm, "created").unwrap_or_else(|| id.created_at()),
        updated_at: timestamp(fm, "updated").unwrap_or_else(|| id.created_at()),
    }))
}

const ACCEPTANCE: &str = "## Acceptance";
const OUTCOME: &str = "## Outcome";

struct TaskBody {
    description: String,
    acceptance_criteria: Option<String>,
    note: Option<String>,
}

/// A task's description, then its trailing `## Acceptance` and `## Outcome` sections, in
/// either order. Only a line that is exactly one of those headings starts a section.
fn split_task_body(body: &str) -> TaskBody {
    let mut description = Vec::new();
    let mut acceptance = Vec::new();
    let mut outcome = Vec::new();
    let mut current = &mut description;
    for line in body.lines() {
        match line.trim_end() {
            ACCEPTANCE => current = &mut acceptance,
            OUTCOME => current = &mut outcome,
            _ => current.push(line),
        }
    }
    let section = |lines: Vec<&str>| {
        let text = lines.join("\n").trim().to_owned();
        (!text.is_empty()).then_some(text)
    };
    TaskBody {
        description: description.join("\n").trim().to_owned(),
        acceptance_criteria: section(acceptance),
        note: section(outcome),
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
    frontmatter.set("updated", stamp(document.updated_at()));

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

pub fn document_from_markdown(file: &MarkdownFile) -> Result<Document> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id"))?)?;
    let kind = fm
        .string("type")
        .ok_or_else(|| missing("type"))?
        .parse::<Kind>()?;
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
        status: fm
            .string("status")
            .map(str::parse::<DocumentStatus>)
            .transpose()?,
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

pub fn message_from_markdown(file: &MarkdownFile) -> Result<Message> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id"))?)?;
    let thread = fm
        .string("thread")
        .map(Id::new)
        .transpose()?
        .unwrap_or_else(|| id.clone());

    Ok(Message::new(RestoreMessage {
        id: id.clone(),
        thread,
        in_reply_to: fm.string("in_reply_to").map(Id::new).transpose()?,
        from: fm.string("from").ok_or_else(|| missing("from"))?.parse()?,
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

pub fn actor_from_markdown(file: &MarkdownFile) -> Result<Actor> {
    let fm = &file.frontmatter;
    let id: ActorId = fm.string("id").ok_or_else(|| missing("id"))?.parse()?;
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

pub fn skill_to_markdown(skill: &Skill) -> MarkdownFile {
    let mut frontmatter = Frontmatter::new();
    frontmatter.set("id", json!(skill.id().to_string()));
    frontmatter.set("type", json!("skill"));
    frontmatter.set("name", json!(skill.name().to_string()));
    frontmatter.set("summary", json!(skill.summary().to_string()));
    frontmatter.set("namespace", json!(skill.namespace().to_string()));
    frontmatter.set("status", json!(skill.status().as_str()));
    if !skill.tags().is_empty() {
        frontmatter.set("tags", list(skill.tags().iter().map(ToString::to_string)));
    }
    frontmatter.set("created", stamp(skill.created_at()));
    frontmatter.set("updated", stamp(skill.updated_at()));

    for (key, value) in skill.frontmatter().iter() {
        if !SKILL_KEYS.contains(&key) && key != "updated" {
            frontmatter.set(key, value.clone());
        }
    }

    MarkdownFile {
        frontmatter,
        body: skill.body().clone(),
    }
}

pub fn skill_from_markdown(file: &MarkdownFile) -> Result<Skill> {
    let fm = &file.frontmatter;
    let id = Id::new(fm.string("id").ok_or_else(|| missing("id"))?)?;
    let name = SkillName::new(fm.string("name").ok_or_else(|| missing("name"))?)?;
    let summary = Summary::new(fm.string("summary").ok_or_else(|| missing("summary"))?)?;

    let mut carried = Frontmatter::new();
    for (key, value) in fm.iter() {
        if !SKILL_KEYS.contains(&key) && key != "updated" {
            carried.set(key, value.clone());
        }
    }

    Ok(Skill::new(RestoreSkill {
        id: id.clone(),
        name,
        summary,
        namespace: fm
            .string("namespace")
            .map(Namespace::new)
            .transpose()?
            .unwrap_or_default(),
        status: match fm.string("status") {
            Some("retired") => SkillStatus::Retired,
            _ => SkillStatus::Active,
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_body_splits_into_description_criteria_and_outcome_in_either_order() {
        let body = split_task_body(
            "Do it.\n\n## Outcome\n\nDone in abc123.\n\n## Acceptance\n\nTests pass.",
        );
        assert_eq!(body.description, "Do it.");
        assert_eq!(body.acceptance_criteria.as_deref(), Some("Tests pass."));
        assert_eq!(body.note.as_deref(), Some("Done in abc123."));
    }

    #[test]
    fn a_lookalike_heading_is_part_of_the_description() {
        let body = split_task_body("Intro\n### Acceptance\nnot a section");
        assert_eq!(body.description, "Intro\n### Acceptance\nnot a section");
        assert!(body.acceptance_criteria.is_none());
    }

    #[test]
    fn an_empty_section_is_no_section() {
        let body = split_task_body("Intro\n\n## Outcome\n\n");
        assert!(body.note.is_none());
    }
}
