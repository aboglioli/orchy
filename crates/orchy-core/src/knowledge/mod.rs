pub mod events;

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use self::events as knowledge_events;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};

#[async_trait::async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn save(&self, entry: &mut Knowledge) -> Result<()>;
    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>>;
    async fn find_by_ids(&self, ids: &[KnowledgeId]) -> Result<Vec<Knowledge>>;
    async fn find_by_path(
        &self,
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>>;
    async fn list(&self, filter: KnowledgeFilter, page: PageParams) -> Result<Page<Knowledge>>;
    async fn search(
        &self,
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<(Knowledge, Option<f32>)>>;
    async fn delete(&self, id: &KnowledgeId) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KnowledgeId(Uuid);

impl KnowledgeId {
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

impl Default for KnowledgeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for KnowledgeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for KnowledgeId {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| Error::invalid_input(format!("invalid knowledge id: {s}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeKind {
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
    Skill,
    Overview,
    Summary,
    Report,
}

impl KnowledgeKind {
    pub fn description(&self) -> &'static str {
        match self {
            KnowledgeKind::Note => "general observation or record",
            KnowledgeKind::Decision => "a choice made with rationale",
            KnowledgeKind::Discovery => "something found or learned",
            KnowledgeKind::Pattern => "a recurring approach or convention",
            KnowledgeKind::Context => "session summary or agent state snapshot",
            KnowledgeKind::Document => "long-form structured content",
            KnowledgeKind::Config => "configuration or setup information",
            KnowledgeKind::Reference => "external reference or link",
            KnowledgeKind::Plan => "strategy, roadmap, or approach",
            KnowledgeKind::Log => "activity or change log entry",
            KnowledgeKind::Skill => "instruction or convention agents must follow",
            KnowledgeKind::Overview => "project overview text included in bootstrap prompts",
            KnowledgeKind::Summary => {
                "compact synthesized output: task summaries, agent rollups, state snapshots"
            }
            KnowledgeKind::Report => {
                "richer completion artifact: implementation reports, post-task writeups"
            }
        }
    }

    pub fn all() -> &'static [KnowledgeKind] {
        &[
            KnowledgeKind::Note,
            KnowledgeKind::Decision,
            KnowledgeKind::Discovery,
            KnowledgeKind::Pattern,
            KnowledgeKind::Context,
            KnowledgeKind::Document,
            KnowledgeKind::Config,
            KnowledgeKind::Reference,
            KnowledgeKind::Plan,
            KnowledgeKind::Log,
            KnowledgeKind::Skill,
            KnowledgeKind::Overview,
            KnowledgeKind::Summary,
            KnowledgeKind::Report,
        ]
    }
}

impl fmt::Display for KnowledgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            KnowledgeKind::Note => "note",
            KnowledgeKind::Decision => "decision",
            KnowledgeKind::Discovery => "discovery",
            KnowledgeKind::Pattern => "pattern",
            KnowledgeKind::Context => "context",
            KnowledgeKind::Document => "document",
            KnowledgeKind::Config => "config",
            KnowledgeKind::Reference => "reference",
            KnowledgeKind::Plan => "plan",
            KnowledgeKind::Log => "log",
            KnowledgeKind::Skill => "skill",
            KnowledgeKind::Overview => "overview",
            KnowledgeKind::Summary => "summary",
            KnowledgeKind::Report => "report",
        };
        write!(f, "{s}")
    }
}

impl FromStr for KnowledgeKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "note" => Ok(KnowledgeKind::Note),
            "decision" => Ok(KnowledgeKind::Decision),
            "discovery" => Ok(KnowledgeKind::Discovery),
            "pattern" => Ok(KnowledgeKind::Pattern),
            "context" => Ok(KnowledgeKind::Context),
            "document" => Ok(KnowledgeKind::Document),
            "config" => Ok(KnowledgeKind::Config),
            "reference" => Ok(KnowledgeKind::Reference),
            "plan" => Ok(KnowledgeKind::Plan),
            "log" => Ok(KnowledgeKind::Log),
            "skill" => Ok(KnowledgeKind::Skill),
            "overview" => Ok(KnowledgeKind::Overview),
            "summary" => Ok(KnowledgeKind::Summary),
            "report" => Ok(KnowledgeKind::Report),
            other => Err(format!("unknown knowledge kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(u64);

impl Version {
    pub fn new(v: u64) -> Self {
        Self(v)
    }

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
pub struct Knowledge {
    id: KnowledgeId,
    org_id: OrganizationId,
    project: Option<ProjectId>,
    namespace: Namespace,
    path: String,
    kind: KnowledgeKind,
    title: String,
    content: String,
    tags: Vec<String>,
    version: Version,
    metadata: HashMap<String, String>,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    valid_from: Option<DateTime<Utc>>,
    valid_until: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    persisted_version: Option<Version>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Knowledge {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        org_id: OrganizationId,
        project: Option<ProjectId>,
        namespace: Namespace,
        path: String,
        kind: KnowledgeKind,
        title: String,
        content: String,
        tags: Vec<String>,
        metadata: HashMap<String, String>,
    ) -> Result<Self> {
        validate_path(&path)?;
        if title.trim().is_empty() {
            return Err(Error::InvalidInput("title must not be empty".into()));
        }

        let now = Utc::now();
        let mut entry = Self {
            id: KnowledgeId::new(),
            org_id,
            project,
            namespace,
            path,
            kind,
            title,
            content,
            tags,
            version: Version::initial(),
            metadata,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            valid_from: None,
            valid_until: None,
            created_at: now,
            updated_at: now,
            persisted_version: None,
            collector: EventCollector::new(),
        };

        entry.collector.collect(
            Event::create(
                entry.org_id.as_str(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_CREATED,
                Payload::from_json(&knowledge_events::KnowledgeCreatedPayload {
                    org_id: entry.org_id.to_string(),
                    entry_id: entry.id.to_string(),
                    project: entry.project.as_ref().map(|p| p.to_string()),
                    namespace: entry.namespace.to_string(),
                    path: entry.path.clone(),
                    kind: entry.kind.to_string(),
                    title: entry.title.clone(),
                    content: entry.content.clone(),
                    tags: entry.tags.clone(),
                    metadata: entry.metadata.clone(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(entry)
    }

    pub fn restore(r: RestoreKnowledge) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            project: r.project,
            namespace: r.namespace,
            path: r.path,
            kind: r.kind,
            title: r.title,
            content: r.content,
            tags: r.tags,
            version: r.version,
            metadata: r.metadata,
            embedding: r.embedding,
            embedding_model: r.embedding_model,
            embedding_dimensions: r.embedding_dimensions,
            valid_from: r.valid_from,
            valid_until: r.valid_until,
            created_at: r.created_at,
            updated_at: r.updated_at,
            persisted_version: Some(r.version),
            collector: EventCollector::new(),
        }
    }

    pub fn update(&mut self, title: String, content: String) -> Result<()> {
        self.title = title;
        self.content = content;
        self.version = self.version.next();
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeUpdatedPayload {
            entry_id: self.id.to_string(),
            path: self.path.clone(),
            title: self.title.clone(),
            content: self.content.clone(),
            version: self.version.as_u64(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_UPDATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn change_kind(&mut self, new_kind: KnowledgeKind) -> Result<()> {
        if self.kind == new_kind {
            return Ok(());
        }
        let old_kind = self.kind;
        self.kind = new_kind;
        self.version = self.version.next();
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeKindChangedPayload {
            entry_id: self.id.to_string(),
            path: self.path.clone(),
            old_kind: old_kind.to_string(),
            new_kind: self.kind.to_string(),
            version: self.version.as_u64(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_KIND_CHANGED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn add_tag(&mut self, tag: String) -> Result<()> {
        if !self.tags.contains(&tag) {
            self.tags.push(tag.clone());
            self.updated_at = Utc::now();

            let payload = Payload::from_json(&knowledge_events::KnowledgeTaggedPayload {
                entry_id: self.id.to_string(),
                tag,
            })
            .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
            let event = Event::create(
                self.org_id.as_str(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_TAGGED,
                payload,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?;
            self.collector.collect(event);
        }
        Ok(())
    }

    pub fn remove_tag(&mut self, tag: &str) -> Result<()> {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at = Utc::now();

            let payload = Payload::from_json(&knowledge_events::KnowledgeTagRemovedPayload {
                entry_id: self.id.to_string(),
                tag: tag.to_string(),
            })
            .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
            let event = Event::create(
                self.org_id.as_str(),
                knowledge_events::NAMESPACE,
                knowledge_events::TOPIC_TAG_REMOVED,
                payload,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?;
            self.collector.collect(event);
        }
        Ok(())
    }

    pub fn move_to(&mut self, namespace: Namespace) -> Result<()> {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeMovedPayload {
            entry_id: self.id.to_string(),
            from_namespace,
            to_namespace: self.namespace.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_MOVED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn rename(&mut self, path: String) -> Result<()> {
        validate_path(&path)?;
        let old_path = self.path.clone();
        self.path = path;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeRenamedPayload {
            entry_id: self.id.to_string(),
            old_path,
            new_path: self.path.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_RENAMED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        Ok(())
    }

    pub fn set_metadata(&mut self, key: String, value: String) -> Result<()> {
        self.metadata.insert(key.clone(), value.clone());
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeMetadataSetPayload {
            entry_id: self.id.to_string(),
            key,
            value,
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_METADATA_SET,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn remove_metadata(&mut self, key: &str) -> Result<bool> {
        if self.metadata.remove(key).is_none() {
            return Ok(false);
        }
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&knowledge_events::KnowledgeMetadataRemovedPayload {
            entry_id: self.id.to_string(),
            key: key.to_string(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_METADATA_REMOVED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(true)
    }

    pub fn mark_deleted(&mut self) -> Result<()> {
        let payload = Payload::from_json(&knowledge_events::KnowledgeDeletedPayload {
            entry_id: self.id.to_string(),
            path: self.path.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_DELETED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn set_embedding(
        &mut self,
        embedding: Vec<f32>,
        model: String,
        dimensions: u32,
    ) -> Result<()> {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model.clone());
        self.embedding_dimensions = Some(dimensions);

        let payload = Payload::from_json(&knowledge_events::KnowledgeEmbeddingUpdatedPayload {
            entry_id: self.id.to_string(),
            model,
            dimensions,
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.org_id.as_str(),
            knowledge_events::NAMESPACE,
            knowledge_events::TOPIC_EMBEDDING_UPDATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);
        Ok(())
    }

    pub fn persisted_version(&self) -> Option<Version> {
        self.persisted_version
    }

    pub fn mark_persisted(&mut self) {
        self.persisted_version = Some(self.version);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> KnowledgeId {
        self.id
    }
    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }
    pub fn project(&self) -> Option<&ProjectId> {
        self.project.as_ref()
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn kind(&self) -> KnowledgeKind {
        self.kind
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
    pub fn valid_from(&self) -> Option<DateTime<Utc>> {
        self.valid_from
    }
    pub fn valid_until(&self) -> Option<DateTime<Utc>> {
        self.valid_until
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct RestoreKnowledge {
    pub id: KnowledgeId,
    pub org_id: OrganizationId,
    pub project: Option<ProjectId>,
    pub namespace: Namespace,
    pub path: String,
    pub kind: KnowledgeKind,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub version: Version,
    pub metadata: HashMap<String, String>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WriteKnowledge {
    pub org_id: OrganizationId,
    pub project: Option<ProjectId>,
    pub namespace: Namespace,
    pub path: String,
    pub kind: KnowledgeKind,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub expected_version: Option<Version>,
    pub metadata: HashMap<String, String>,
    pub metadata_remove: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct KnowledgeFilter {
    pub org_id: Option<OrganizationId>,
    pub project: Option<ProjectId>,
    pub include_org_level: bool,
    pub namespace: Option<Namespace>,
    pub kind: Option<KnowledgeKind>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    /// When Some(true): only entries with no incoming produces/owned_by edges.
    /// When Some(false): only entries with at least one such edge.
    /// When None: no filter.
    pub orphaned: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchy_events::OrganizationId;

    fn test_org() -> OrganizationId {
        OrganizationId::new("test").unwrap()
    }

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
        let entry = Knowledge::new(
            test_org(),
            Some(proj("test")),
            Namespace::root(),
            "decisions/db".into(),
            KnowledgeKind::Decision,
            "Database choice".into(),
            "We chose PostgreSQL".into(),
            vec!["infra".into()],
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(entry.kind(), KnowledgeKind::Decision);
        assert_eq!(entry.path(), "decisions/db");
        assert_eq!(entry.version().as_u64(), 1);
        assert_eq!(entry.tags(), &["infra"]);
    }

    #[test]
    fn empty_title_fails() {
        let result = Knowledge::new(
            test_org(),
            Some(proj("test")),
            Namespace::root(),
            "path".into(),
            KnowledgeKind::Note,
            "".into(),
            "content".into(),
            vec![],
            HashMap::new(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn kind_roundtrip() {
        for t in KnowledgeKind::all() {
            let s = t.to_string();
            let parsed: KnowledgeKind = s.parse().unwrap();
            assert_eq!(*t, parsed);
        }
    }

    #[test]
    fn remove_metadata_only_when_key_exists() {
        let mut md = HashMap::new();
        md.insert("k".into(), "v".into());
        let mut entry = Knowledge::new(
            test_org(),
            Some(proj("test")),
            Namespace::root(),
            "path".into(),
            KnowledgeKind::Note,
            "title".into(),
            "c".into(),
            vec![],
            md,
        )
        .unwrap();
        assert!(!entry.remove_metadata("missing").unwrap());
        assert!(entry.remove_metadata("k").unwrap());
        assert!(entry.metadata().is_empty());
    }

    #[test]
    fn update_increments_version() {
        let mut entry = Knowledge::new(
            test_org(),
            Some(proj("test")),
            Namespace::root(),
            "key".into(),
            KnowledgeKind::Note,
            "title".into(),
            "v1".into(),
            vec![],
            HashMap::new(),
        )
        .unwrap();
        entry.update("title".into(), "v2".into()).unwrap();
        assert_eq!(entry.version().as_u64(), 2);
        assert_eq!(entry.content(), "v2");
    }

    #[test]
    fn change_kind_updates_kind_and_version() {
        let mut entry = Knowledge::new(
            test_org(),
            Some(proj("test")),
            Namespace::root(),
            "key".into(),
            KnowledgeKind::Note,
            "title".into(),
            "body".into(),
            vec![],
            HashMap::new(),
        )
        .unwrap();
        assert_eq!(entry.version().as_u64(), 1);
        entry.change_kind(KnowledgeKind::Overview).unwrap();
        assert_eq!(entry.kind(), KnowledgeKind::Overview);
        assert_eq!(entry.version().as_u64(), 2);
        entry.change_kind(KnowledgeKind::Overview).unwrap();
        assert_eq!(entry.version().as_u64(), 2);
    }
}
