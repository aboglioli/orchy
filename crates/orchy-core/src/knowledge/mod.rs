pub mod events;
pub mod service;

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

use self::events as knowledge_events;

pub trait EntryStore: Send + Sync {
    fn save(&self, entry: &mut Entry) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &EntryId) -> impl Future<Output = Result<Option<Entry>>> + Send;
    fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> impl Future<Output = Result<Option<Entry>>> + Send;
    fn list(&self, filter: EntryFilter) -> impl Future<Output = Result<Vec<Entry>>> + Send;
    fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Entry>>> + Send;
    fn delete(&self, id: &EntryId) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryId(Uuid);

impl EntryId {
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

impl Default for EntryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EntryId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Note,
    Decision,
    Discovery,
    Pattern,
    Context,
    Document,
    Config,
    Reference,
    Plan,
    Log,
}

impl EntryType {
    pub fn description(&self) -> &'static str {
        match self {
            EntryType::Note => "general observation or record",
            EntryType::Decision => "a choice made with rationale",
            EntryType::Discovery => "something found or learned",
            EntryType::Pattern => "a recurring approach or convention",
            EntryType::Context => "session summary or agent state snapshot",
            EntryType::Document => "long-form structured content",
            EntryType::Config => "configuration or setup information",
            EntryType::Reference => "external reference or link",
            EntryType::Plan => "strategy, roadmap, or approach",
            EntryType::Log => "activity or change log entry",
        }
    }

    pub fn all() -> &'static [EntryType] {
        &[
            EntryType::Note,
            EntryType::Decision,
            EntryType::Discovery,
            EntryType::Pattern,
            EntryType::Context,
            EntryType::Document,
            EntryType::Config,
            EntryType::Reference,
            EntryType::Plan,
            EntryType::Log,
        ]
    }
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EntryType::Note => "note",
            EntryType::Decision => "decision",
            EntryType::Discovery => "discovery",
            EntryType::Pattern => "pattern",
            EntryType::Context => "context",
            EntryType::Document => "document",
            EntryType::Config => "config",
            EntryType::Reference => "reference",
            EntryType::Plan => "plan",
            EntryType::Log => "log",
        };
        write!(f, "{s}")
    }
}

impl FromStr for EntryType {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "note" => Ok(EntryType::Note),
            "decision" => Ok(EntryType::Decision),
            "discovery" => Ok(EntryType::Discovery),
            "pattern" => Ok(EntryType::Pattern),
            "context" => Ok(EntryType::Context),
            "document" => Ok(EntryType::Document),
            "config" => Ok(EntryType::Config),
            "reference" => Ok(EntryType::Reference),
            "plan" => Ok(EntryType::Plan),
            "log" => Ok(EntryType::Log),
            other => Err(format!("unknown entry type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(u64);

impl Version {
    pub fn initial() -> Self {
        Self(1)
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Version {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::InvalidInput("path must not be empty".into()));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidInput(
            "path must not start or end with '/'".into(),
        ));
    }
    if path.contains("//") {
        return Err(Error::InvalidInput("path must not contain '//'".into()));
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(Error::InvalidInput("path contains empty segment".into()));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::InvalidInput(format!(
                "invalid character in path segment: {segment}"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    id: EntryId,
    project: ProjectId,
    namespace: Namespace,
    path: String,
    entry_type: EntryType,
    title: String,
    content: String,
    tags: Vec<String>,
    version: Version,
    agent_id: Option<AgentId>,
    metadata: HashMap<String, String>,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Entry {
    pub fn new(
        project: ProjectId,
        namespace: Namespace,
        path: String,
        entry_type: EntryType,
        title: String,
        content: String,
        tags: Vec<String>,
        agent_id: Option<AgentId>,
        metadata: HashMap<String, String>,
    ) -> Result<Self> {
        validate_path(&path)?;
        if title.trim().is_empty() {
            return Err(Error::InvalidInput("title must not be empty".into()));
        }

        let now = Utc::now();
        let mut entry = Self {
            id: EntryId::new(),
            project,
            namespace,
            path,
            entry_type,
            title,
            content,
            tags,
            version: Version::initial(),
            agent_id,
            metadata,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        entry.collector.collect(
            Event::create(
                entry.project.as_ref(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_CREATED,
                Payload::from_json(&knowledge_events::EntryCreatedPayload {
                    entry_id: entry.id.to_string(),
                    project: entry.project.to_string(),
                    namespace: entry.namespace.to_string(),
                    path: entry.path.clone(),
                    entry_type: entry.entry_type.to_string(),
                    title: entry.title.clone(),
                    content: entry.content.clone(),
                    tags: entry.tags.clone(),
                    agent_id: entry.agent_id.map(|a| a.to_string()),
                    metadata: entry.metadata.clone(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(entry)
    }

    pub fn restore(r: RestoreEntry) -> Self {
        Self {
            id: r.id,
            project: r.project,
            namespace: r.namespace,
            path: r.path,
            entry_type: r.entry_type,
            title: r.title,
            content: r.content,
            tags: r.tags,
            version: r.version,
            agent_id: r.agent_id,
            metadata: r.metadata,
            embedding: r.embedding,
            embedding_model: r.embedding_model,
            embedding_dimensions: r.embedding_dimensions,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn update(&mut self, title: String, content: String, agent_id: Option<AgentId>) {
        self.title = title;
        self.content = content;
        self.version = self.version.next();
        if let Some(agent) = agent_id {
            self.agent_id = Some(agent);
        }
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_UPDATED,
            Payload::from_json(&knowledge_events::EntryUpdatedPayload {
                entry_id: self.id.to_string(),
                path: self.path.clone(),
                title: self.title.clone(),
                content: self.content.clone(),
                version: self.version.as_u64(),
                agent_id: self.agent_id.map(|a| a.to_string()),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag.clone());
            self.updated_at = Utc::now();

            let _ = Event::create(
                self.project.as_ref(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_TAGGED,
                Payload::from_json(&knowledge_events::EntryTaggedPayload {
                    entry_id: self.id.to_string(),
                    tag,
                })
                .unwrap(),
            )
            .map(|e| self.collector.collect(e));
        }
    }

    pub fn remove_tag(&mut self, tag: &str) {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at = Utc::now();

            let _ = Event::create(
                self.project.as_ref(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_TAG_REMOVED,
                Payload::from_json(&knowledge_events::EntryTagRemovedPayload {
                    entry_id: self.id.to_string(),
                    tag: tag.to_string(),
                })
                .unwrap(),
            )
            .map(|e| self.collector.collect(e));
        }
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_MOVED,
            Payload::from_json(&knowledge_events::EntryMovedPayload {
                entry_id: self.id.to_string(),
                from_namespace,
                to_namespace: self.namespace.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn rename(&mut self, path: String) -> Result<()> {
        validate_path(&path)?;
        let old_path = self.path.clone();
        self.path = path;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_RENAMED,
            Payload::from_json(&knowledge_events::EntryRenamedPayload {
                entry_id: self.id.to_string(),
                old_path,
                new_path: self.path.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));

        Ok(())
    }

    pub fn set_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key.clone(), value.clone());
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_METADATA_SET,
            Payload::from_json(&knowledge_events::EntryMetadataSetPayload {
                entry_id: self.id.to_string(),
                key,
                value,
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn mark_deleted(&mut self) {
        let _ = Event::create(
            self.project.as_ref(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_DELETED,
            Payload::from_json(&knowledge_events::EntryDeletedPayload {
                entry_id: self.id.to_string(),
                path: self.path.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> EntryId {
        self.id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn entry_type(&self) -> EntryType {
        self.entry_type
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn content(&self) -> &str {
        &self.content
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn agent_id(&self) -> Option<AgentId> {
        self.agent_id
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
    pub fn embedding_model(&self) -> Option<&str> {
        self.embedding_model.as_deref()
    }
    pub fn embedding_dimensions(&self) -> Option<u32> {
        self.embedding_dimensions
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct RestoreEntry {
    pub id: EntryId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub path: String,
    pub entry_type: EntryType,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub version: Version,
    pub agent_id: Option<AgentId>,
    pub metadata: HashMap<String, String>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WriteEntry {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub path: String,
    pub entry_type: EntryType,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub expected_version: Option<Version>,
    pub agent_id: Option<AgentId>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct EntryFilter {
    pub project: Option<ProjectId>,
    pub namespace: Option<Namespace>,
    pub entry_type: Option<EntryType>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub agent_id: Option<AgentId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proj(s: &str) -> ProjectId {
        ProjectId::try_from(s).unwrap()
    }

    #[test]
    fn valid_paths() {
        assert!(validate_path("decisions/db-choice").is_ok());
        assert!(validate_path("context/session-1").is_ok());
        assert!(validate_path("specs/auth-design").is_ok());
        assert!(validate_path("simple-key").is_ok());
        assert!(validate_path("a/b/c/d").is_ok());
    }

    #[test]
    fn invalid_paths() {
        assert!(validate_path("").is_err());
        assert!(validate_path("/leading").is_err());
        assert!(validate_path("trailing/").is_err());
        assert!(validate_path("double//slash").is_err());
        assert!(validate_path("has spaces").is_err());
        assert!(validate_path("has.dots").is_err());
    }

    #[test]
    fn create_entry() {
        let entry = Entry::new(
            proj("test"),
            Namespace::root(),
            "decisions/db".into(),
            EntryType::Decision,
            "Database choice".into(),
            "We chose PostgreSQL".into(),
            vec!["infra".into()],
            None,
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(entry.entry_type(), EntryType::Decision);
        assert_eq!(entry.path(), "decisions/db");
        assert_eq!(entry.version().as_u64(), 1);
        assert_eq!(entry.tags(), &["infra"]);
    }

    #[test]
    fn empty_title_fails() {
        let result = Entry::new(
            proj("test"),
            Namespace::root(),
            "path".into(),
            EntryType::Note,
            "".into(),
            "content".into(),
            vec![],
            None,
            HashMap::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn entry_type_roundtrip() {
        for t in EntryType::all() {
            let s = t.to_string();
            let parsed: EntryType = s.parse().unwrap();
            assert_eq!(*t, parsed);
        }
    }

    #[test]
    fn update_increments_version() {
        let mut entry = Entry::new(
            proj("test"),
            Namespace::root(),
            "key".into(),
            EntryType::Note,
            "title".into(),
            "v1".into(),
            vec![],
            None,
            HashMap::new(),
        )
        .unwrap();
        entry.update("title".into(), "v2".into(), None);
        assert_eq!(entry.version().as_u64(), 2);
        assert_eq!(entry.content(), "v2");
    }
}
