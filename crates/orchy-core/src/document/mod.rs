pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::memory::Version;
use crate::namespace::{Namespace, ProjectId};

pub trait DocumentStore: Send + Sync {
    fn save(&self, doc: &mut Document) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &DocumentId) -> impl Future<Output = Result<Option<Document>>> + Send;
    fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> impl Future<Output = Result<Option<Document>>> + Send;
    fn list(&self, filter: DocumentFilter) -> impl Future<Output = Result<Vec<Document>>> + Send;
    fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<Document>>> + Send;
    fn delete(&self, id: &DocumentId) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(Uuid);

impl DocumentId {
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

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for DocumentId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

fn validate_path(path: &str) -> Result<()> {
    if path.trim().is_empty() {
        return Err(Error::InvalidInput(
            "document path must not be empty".into(),
        ));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidInput(
            "document path must not start or end with '/'".into(),
        ));
    }
    for part in path.split('/') {
        if part.is_empty() {
            return Err(Error::InvalidInput(
                "document path must not contain empty segments".into(),
            ));
        }
        for ch in part.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return Err(Error::InvalidInput(format!(
                    "invalid character '{ch}' in document path segment '{part}'"
                )));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    id: DocumentId,
    project: ProjectId,
    namespace: Namespace,
    path: String,
    title: String,
    content: String,
    tags: Vec<String>,
    version: Version,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    created_by: Option<AgentId>,
    updated_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Document {
    pub fn new(
        project: ProjectId,
        namespace: Namespace,
        path: String,
        title: String,
        content: String,
        tags: Vec<String>,
        created_by: Option<AgentId>,
    ) -> Result<Self> {
        validate_path(&path)?;
        if title.trim().is_empty() {
            return Err(Error::InvalidInput(
                "document title must not be empty".into(),
            ));
        }

        let now = Utc::now();
        Ok(Self {
            id: DocumentId::new(),
            project,
            namespace,
            path,
            title,
            content,
            tags,
            version: Version::initial(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            created_by,
            updated_by: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn restore(r: RestoreDocument) -> Self {
        Self {
            id: r.id,
            project: r.project,
            namespace: r.namespace,
            path: r.path,
            title: r.title,
            content: r.content,
            tags: r.tags,
            version: r.version,
            embedding: r.embedding,
            embedding_model: r.embedding_model,
            embedding_dimensions: r.embedding_dimensions,
            created_by: r.created_by,
            updated_by: r.updated_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }

    pub fn update(&mut self, title: String, content: String, updated_by: Option<AgentId>) {
        self.title = title;
        self.content = content;
        self.version = self.version.next();
        if let Some(author) = updated_by {
            self.updated_by = Some(author);
        }
        self.updated_at = Utc::now();
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
            self.updated_at = Utc::now();
        }
    }

    pub fn remove_tag(&mut self, tag: &str) {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at = Utc::now();
        }
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        self.namespace = namespace;
        self.updated_at = Utc::now();
    }

    pub fn rename(&mut self, path: String) -> Result<()> {
        validate_path(&path)?;
        self.path = path;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn id(&self) -> DocumentId {
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
    pub fn embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
    pub fn embedding_model(&self) -> Option<&str> {
        self.embedding_model.as_deref()
    }
    pub fn embedding_dimensions(&self) -> Option<u32> {
        self.embedding_dimensions
    }
    pub fn created_by(&self) -> Option<AgentId> {
        self.created_by
    }
    pub fn updated_by(&self) -> Option<AgentId> {
        self.updated_by
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct RestoreDocument {
    pub id: DocumentId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub path: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub version: Version,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub created_by: Option<AgentId>,
    pub updated_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WriteDocument {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub path: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub expected_version: Option<Version>,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentFilter {
    pub project: Option<ProjectId>,
    pub namespace: Option<Namespace>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    #[test]
    fn valid_path() {
        assert!(validate_path("specs/auth").is_ok());
        assert!(validate_path("readme").is_ok());
        assert!(validate_path("architecture/db-design").is_ok());
    }

    #[test]
    fn invalid_path() {
        assert!(validate_path("").is_err());
        assert!(validate_path("/specs").is_err());
        assert!(validate_path("specs/").is_err());
        assert!(validate_path("specs//auth").is_err());
        assert!(validate_path("specs/auth design").is_err());
    }

    #[test]
    fn new_document_has_initial_version() {
        let doc = Document::new(
            project(),
            Namespace::root(),
            "spec".into(),
            "Spec".into(),
            "content".into(),
            vec![],
            None,
        )
        .unwrap();
        assert_eq!(doc.version().as_u64(), 1);
    }

    #[test]
    fn update_increments_version() {
        let mut doc = Document::new(
            project(),
            Namespace::root(),
            "spec".into(),
            "Spec".into(),
            "v1".into(),
            vec![],
            None,
        )
        .unwrap();
        doc.update("Spec v2".into(), "v2".into(), None);
        assert_eq!(doc.version().as_u64(), 2);
        assert_eq!(doc.content(), "v2");
    }

    #[test]
    fn tag_operations() {
        let mut doc = Document::new(
            project(),
            Namespace::root(),
            "spec".into(),
            "Spec".into(),
            "content".into(),
            vec![],
            None,
        )
        .unwrap();
        doc.add_tag("architecture".into());
        assert_eq!(doc.tags(), &["architecture"]);
        doc.add_tag("architecture".into());
        assert_eq!(doc.tags().len(), 1);
        doc.remove_tag("architecture");
        assert!(doc.tags().is_empty());
    }

    #[test]
    fn rename_validates_path() {
        let mut doc = Document::new(
            project(),
            Namespace::root(),
            "old".into(),
            "Doc".into(),
            "content".into(),
            vec![],
            None,
        )
        .unwrap();
        assert!(doc.rename("new/path".into()).is_ok());
        assert_eq!(doc.path(), "new/path");
        assert!(doc.rename("".into()).is_err());
    }

    #[test]
    fn empty_title_fails() {
        let result = Document::new(
            project(),
            Namespace::root(),
            "spec".into(),
            "".into(),
            "content".into(),
            vec![],
            None,
        );
        assert!(result.is_err());
    }
}
