mod events;
mod frontmatter;
mod kind;
mod search;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use events::{
    DocumentCreated, DocumentFieldSet, DocumentMoved, DocumentPromoted, DocumentRetyped,
    DocumentSectionReplaced, DocumentStatusChanged, DocumentSuperseded, DocumentWritten,
};
pub use frontmatter::Frontmatter;
pub use kind::{DocumentStatus, Kind};
pub use search::{Hit, Search, SearchQuery, rank};

use crate::body::Body;
use crate::clock::Clock;
use crate::error::{DomainError, Result};
use crate::event::{DomainEvent, EventCollector};
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;
use crate::pagination::{Page, PageRequest};
use crate::tag::{self, Tag};
use crate::title::Title;

#[async_trait]
pub trait DocumentStore: Send + Sync {
    async fn get(&self, id: &Id) -> Result<Option<Document>>;
    async fn find(&self, query: &DocumentQuery, page: PageRequest) -> Result<Page<Document>>;
    async fn save(&self, document: &mut Document) -> Result<()>;
    async fn delete(&self, id: &Id) -> Result<()>;

    async fn require(&self, id: &Id) -> Result<Document> {
        self.get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("document", id))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentQuery {
    pub kind: Option<Vec<Kind>>,
    pub status: Option<Vec<DocumentStatus>>,
    pub namespace: Option<Namespace>,
    pub tags: Vec<Tag>,
    pub text: Option<String>,
    pub updated_since: Option<DateTime<Utc>>,
}

impl DocumentQuery {
    pub fn matches(&self, document: &Document) -> bool {
        if let Some(kinds) = &self.kind
            && !kinds.contains(&document.kind)
        {
            return false;
        }
        if let Some(statuses) = &self.status {
            match &document.status {
                Some(status) if statuses.contains(status) => {}
                _ => return false,
            }
        }
        if let Some(namespace) = &self.namespace
            && !namespace.contains(&document.namespace)
        {
            return false;
        }
        if !self.tags.iter().all(|t| document.tags.contains(t)) {
            return false;
        }
        if let Some(since) = self.updated_since
            && document.updated_at < since
        {
            return false;
        }
        if let Some(text) = &self.text {
            let needle = text.to_lowercase();
            let title = document.title.as_str().to_lowercase();
            let body = document.body.as_str().to_lowercase();
            if !title.contains(&needle) && !body.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    id: Id,
    kind: Kind,
    title: Title,
    namespace: Namespace,
    status: Option<DocumentStatus>,
    tags: Vec<Tag>,
    frontmatter: Frontmatter,
    body: Body,
    content_hash: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

#[derive(Debug, Clone)]
pub struct RestoreDocument {
    pub id: Id,
    pub kind: Kind,
    pub title: Title,
    pub namespace: Namespace,
    pub status: Option<DocumentStatus>,
    pub tags: Vec<Tag>,
    pub frontmatter: Frontmatter,
    pub body: Body,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn new(restore: RestoreDocument) -> Self {
        let content_hash = hash_of(&restore.title, &restore.body, &restore.frontmatter);
        Self {
            id: restore.id,
            kind: restore.kind,
            title: restore.title,
            namespace: restore.namespace,
            status: restore.status,
            tags: restore.tags,
            frontmatter: restore.frontmatter,
            body: restore.body,
            content_hash,
            created_at: restore.created_at,
            updated_at: restore.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn create(
        kind: Kind,
        title: Title,
        namespace: Namespace,
        body: Body,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Self {
        let now = clock.now();
        let id = Id::generate(ids);
        let mut document = Self::new(RestoreDocument {
            id: id.clone(),
            kind,
            title: title.clone(),
            namespace: namespace.clone(),
            status: None,
            tags: Vec::new(),
            frontmatter: Frontmatter::new(),
            body,
            created_at: now,
            updated_at: now,
        });
        document.collector.collect(DocumentCreated {
            id,
            namespace,
            kind,
            title: title.into(),
            content_hash: document.content_hash.clone(),
            at: now,
        });
        document
    }

    pub fn edit(&mut self, body: Body, clock: &dyn Clock) {
        let prev_hash = self.content_hash.clone();
        self.body = body;
        self.rehash(clock);
        self.collector.collect(DocumentWritten {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            content_hash: self.content_hash.clone(),
            prev_hash,
            at: self.updated_at,
        });
    }

    pub fn append(&mut self, extra: &str, clock: &dyn Clock) {
        let body = self.body.append(extra);
        self.edit(body, clock);
    }

    pub fn replace_section(
        &mut self,
        heading: &str,
        content: &str,
        clock: &dyn Clock,
    ) -> Result<()> {
        let body = self
            .body
            .replace_section(heading, content)
            .ok_or_else(|| DomainError::not_found("section", heading))?;
        let prev_hash = self.content_hash.clone();
        self.body = body;
        self.rehash(clock);
        self.collector.collect(DocumentSectionReplaced {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            heading: heading.to_owned(),
            content_hash: self.content_hash.clone(),
            prev_hash,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn replace_once(
        &mut self,
        needle: &str,
        replacement: &str,
        clock: &dyn Clock,
    ) -> Result<()> {
        let body = self
            .body
            .replace_once(needle, replacement)?
            .ok_or_else(|| DomainError::not_found("text", needle))?;
        self.edit(body, clock);
        Ok(())
    }

    pub fn set_field(&mut self, field: &str, value: Value, clock: &dyn Clock) -> Result<()> {
        if Kind::is_projected_field(field) {
            return Err(DomainError::forbidden(format!(
                "`{field}` is maintained by orchy and cannot be set by hand"
            )));
        }
        if let Some(command) = semantic_command_for(field) {
            return Err(DomainError::forbidden(format!(
                "`{field}` is a semantic transition; use `{command}` instead"
            )));
        }
        self.frontmatter.set(field, value.clone());
        self.rehash(clock);
        self.collector.collect(DocumentFieldSet {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            field: field.to_owned(),
            value,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn set_status(&mut self, status: DocumentStatus, clock: &dyn Clock) -> Result<()> {
        self.kind.validate_status(status)?;
        self.status = Some(status);
        self.rehash(clock);
        self.collector.collect(DocumentStatusChanged {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            status,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn retitle(&mut self, title: Title, clock: &dyn Clock) {
        self.title = title;
        self.rehash(clock);
    }

    pub fn retype(&mut self, kind: Kind, clock: &dyn Clock) -> Result<()> {
        if let Some(status) = self.status
            && !kind.allows(status)
        {
            self.status = None;
        }
        let from = std::mem::replace(&mut self.kind, kind);
        self.rehash(clock);
        self.collector.collect(DocumentRetyped {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from,
            to: kind,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn move_to(&mut self, namespace: Namespace, clock: &dyn Clock) {
        let from = std::mem::replace(&mut self.namespace, namespace);
        self.rehash(clock);
        self.collector.collect(DocumentMoved {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from,
            at: self.updated_at,
        });
    }

    pub fn is_candidate(&self) -> bool {
        self.kind.is_candidate()
    }

    /// Candidacy is the kind, not a directory, so promotion declares what the proposal
    /// becomes rather than moving a file.
    pub fn promote(&mut self, into: Kind, namespace: Namespace, clock: &dyn Clock) -> Result<()> {
        if !self.is_candidate() {
            return Err(DomainError::conflict(
                "only a candidate can be promoted; this document is already canon",
            ));
        }
        if into.is_candidate() {
            return Err(DomainError::validation(
                "promoting means becoming something: name the type it graduates into",
            ));
        }
        self.kind = into;
        self.status = Some(DocumentStatus::Active);
        let from = std::mem::replace(&mut self.namespace, namespace);
        self.rehash(clock);
        self.collector.collect(DocumentPromoted {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn supersede(&mut self, by: Id, clock: &dyn Clock) -> Result<()> {
        if by == self.id {
            return Err(DomainError::validation(
                "a document cannot supersede itself",
            ));
        }
        self.kind.validate_status(DocumentStatus::Superseded)?;
        self.status = Some(DocumentStatus::Superseded);
        self.rehash(clock);
        self.collector.collect(DocumentSuperseded {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            by,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn retag(&mut self, add: Vec<Tag>, remove: &[Tag], clock: &dyn Clock) {
        tag::apply(&mut self.tags, add, remove);
        self.rehash(clock);
    }

    fn rehash(&mut self, clock: &dyn Clock) {
        self.updated_at = clock.now();
        self.content_hash = hash_of(&self.title, &self.body, &self.frontmatter);
    }

    pub fn drain_events(&mut self) -> Vec<Box<dyn DomainEvent>> {
        self.collector.drain()
    }

    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn kind(&self) -> &Kind {
        &self.kind
    }
    pub fn title(&self) -> &Title {
        &self.title
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn status(&self) -> Option<DocumentStatus> {
        self.status
    }
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }
    pub fn frontmatter(&self) -> &Frontmatter {
        &self.frontmatter
    }
    pub fn body(&self) -> &Body {
        &self.body
    }
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn semantic_command_for(field: &str) -> Option<&'static str> {
    match field {
        "status" => Some("orchy archive / orchy supersede"),
        "type" => Some("orchy retype"),
        "namespace" => Some("orchy ns move"),
        "id" => Some("(ids are immutable)"),
        "tags" => Some("orchy tag"),
        "title" => Some("orchy retitle"),
        _ => None,
    }
}

fn hash_of(title: &Title, body: &Body, frontmatter: &Frontmatter) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"title\x00");
    hasher.update(title.as_str().as_bytes());
    hasher.update(b"\x00body\x00");
    hasher.update(body.as_str().as_bytes());
    for (key, value) in frontmatter.iter() {
        hasher.update(b"\x00field\x00");
        hasher.update(key.as_bytes());
        hasher.update(b"\x00");
        hasher.update(value.to_string().as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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

    fn ids() -> SeqIds {
        SeqIds(std::sync::atomic::AtomicU64::new(1))
    }

    fn document() -> Document {
        Document::create(
            Kind::Decision,
            Title::new("Rotate signing keys").unwrap(),
            Namespace::new("/backend").unwrap(),
            Body::new("# Context\nWe use HS256.\n\n# Decision\nMove to RS256.\n"),
            &ids(),
            &clock(),
        )
    }

    fn candidate() -> Document {
        Document::create(
            Kind::Candidate,
            Title::new("Maybe").unwrap(),
            Namespace::new("/inbox").unwrap(),
            Body::new("unsure"),
            &ids(),
            &clock(),
        )
    }

    #[test]
    fn creating_emits_one_event_carrying_the_content_hash() {
        let mut document = document();
        let hash = document.content_hash().to_owned();
        let events = document.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), "document.created");
        assert!(!hash.is_empty());
    }

    #[test]
    fn the_content_hash_is_stable_for_identical_content() {
        let a = document();
        let b = document();
        assert_eq!(
            a.content_hash(),
            b.content_hash(),
            "the same content must hash the same on every host"
        );
        assert_eq!(a.content_hash().len(), 64, "sha256 renders as 64 hex chars");
    }

    #[test]
    fn editing_changes_the_hash_and_chains_to_the_previous_one() {
        let mut document = document();
        let before = document.content_hash().to_owned();
        document.drain_events();

        document.edit(Body::new("something else"), &clock());
        assert_ne!(document.content_hash(), before);

        let events = document.drain_events();
        assert_eq!(events[0].topic().as_str(), "document.written");
        let text = String::from_utf8(events[0].payload().unwrap().data().to_vec()).unwrap();
        assert!(text.contains(&before), "the event must carry the prev_hash");
    }

    #[test]
    fn title_and_frontmatter_are_part_of_the_hash_not_just_the_body() {
        let mut a = document();
        a.retitle(Title::new("A different title").unwrap(), &clock());
        assert_ne!(a.content_hash(), document().content_hash());

        let mut b = document();
        b.set_field("owner", json!("alan"), &clock()).unwrap();
        assert_ne!(b.content_hash(), document().content_hash());
    }

    #[test]
    fn set_field_refuses_a_field_orchy_maintains() {
        let mut document = document();
        let err = document
            .set_field("superseded_by", json!(["x"]), &clock())
            .unwrap_err();
        assert!(matches!(err, DomainError::Forbidden(_)), "{err:?}");
    }

    #[test]
    fn set_field_redirects_a_semantic_transition_to_its_command() {
        let mut document = document();
        let err = document
            .set_field("status", json!("superseded"), &clock())
            .unwrap_err();
        assert!(err.to_string().contains("orchy supersede"), "{err}");

        for field in ["type", "namespace", "id", "tags", "title"] {
            assert!(
                document.set_field(field, json!("x"), &clock()).is_err(),
                "`{field}` must be refused by set"
            );
        }
    }

    #[test]
    fn set_field_accepts_an_authors_own_key() {
        let mut document = document();
        document
            .set_field("reviewer", json!("codex"), &clock())
            .unwrap();
        assert_eq!(document.frontmatter().string("reviewer"), Some("codex"));
    }

    #[test]
    fn status_is_validated_against_the_types_enum() {
        let mut document = document();
        assert!(
            document
                .set_status(DocumentStatus::Active, &clock())
                .is_ok()
        );
        assert!(
            document
                .set_status(DocumentStatus::Promoted, &clock())
                .is_err()
        );
    }

    #[test]
    fn superseding_sets_the_status_and_emits_the_link_in_one_step() {
        let mut document = document();
        document.drain_events();
        let by = Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap();
        document.supersede(by, &clock()).unwrap();

        assert_eq!(document.status().unwrap().as_str(), "superseded");
        let events = document.drain_events();
        assert_eq!(events[0].topic().as_str(), "document.superseded");
    }

    #[test]
    fn a_document_cannot_supersede_itself() {
        let mut document = document();
        let own = document.id().clone();
        assert!(document.supersede(own, &clock()).is_err());
    }

    #[test]
    fn moving_keeps_the_id_so_links_never_need_repairing() {
        let mut document = document();
        let id = document.id().clone();
        document.move_to(Namespace::new("/frontend").unwrap(), &clock());
        assert_eq!(document.id(), &id);
        assert_eq!(document.namespace().as_str(), "/frontend");
    }

    #[test]
    fn only_a_candidate_can_be_promoted() {
        let mut canon = document();
        assert!(!canon.is_candidate());
        assert!(
            canon
                .promote(
                    Kind::Decision,
                    Namespace::new("/backend").unwrap(),
                    &clock()
                )
                .is_err(),
            "promoting canon is a conflict, not a no-op"
        );

        let mut candidate = candidate();
        assert!(candidate.is_candidate());
        candidate
            .promote(
                Kind::Decision,
                Namespace::new("/backend").unwrap(),
                &clock(),
            )
            .unwrap();
        assert!(!candidate.is_candidate());
        assert_eq!(candidate.kind(), &Kind::Decision);
        assert_eq!(candidate.status(), Some(DocumentStatus::Active));
        assert_eq!(candidate.namespace().as_str(), "/backend");
    }

    #[test]
    fn a_candidate_cannot_graduate_into_another_candidate() {
        let mut candidate = candidate();
        assert!(
            candidate
                .promote(Kind::Candidate, Namespace::root(), &clock())
                .is_err()
        );
    }

    #[test]
    fn retyping_drops_a_status_the_new_type_does_not_recognise() {
        let mut document = document();
        document
            .set_status(DocumentStatus::Active, &clock())
            .unwrap();
        document.retype(Kind::Candidate, &clock()).unwrap();
        assert_eq!(
            document.status(),
            None,
            "a status that does not exist in the target type must not survive"
        );
    }

    #[test]
    fn an_invented_type_cannot_be_constructed_at_all() {
        assert!(
            "invented".parse::<Kind>().is_err(),
            "the enum is the registry: an unknown type never reaches an aggregate"
        );
    }

    #[test]
    fn replace_section_targets_one_heading() {
        let mut document = document();
        document
            .replace_section("Decision", "Move to EdDSA.", &clock())
            .unwrap();
        assert!(document.body().as_str().contains("EdDSA"));
        assert!(document.body().as_str().contains("We use HS256"));
        assert!(!document.body().as_str().contains("Move to RS256"));
    }

    #[test]
    fn replace_section_reports_a_missing_heading_as_not_found() {
        let mut document = document();
        let err = document.replace_section("Nope", "x", &clock()).unwrap_err();
        assert!(matches!(err, DomainError::NotFound { .. }), "{err:?}");
    }

    #[test]
    fn replace_once_refuses_an_ambiguous_target() {
        let mut document = Document::create(
            Kind::Note,
            Title::new("t").unwrap(),
            Namespace::root(),
            Body::new("x\nx\n"),
            &ids(),
            &clock(),
        );
        assert!(document.replace_once("x", "y", &clock()).is_err());
    }

    #[test]
    fn query_matches_on_every_axis() {
        let mut document = document();
        document
            .set_status(DocumentStatus::Active, &clock())
            .unwrap();
        document.retag(vec![Tag::new("auth").unwrap()], &[], &clock());

        assert!(DocumentQuery::default().matches(&document));
        assert!(
            DocumentQuery {
                kind: Some(vec![Kind::Decision]),
                ..Default::default()
            }
            .matches(&document)
        );
        assert!(
            DocumentQuery {
                namespace: Some(Namespace::root()),
                ..Default::default()
            }
            .matches(&document)
        );
        assert!(
            DocumentQuery {
                tags: vec![Tag::new("auth").unwrap()],
                ..Default::default()
            }
            .matches(&document)
        );
        assert!(
            DocumentQuery {
                text: Some("rs256".to_owned()),
                ..Default::default()
            }
            .matches(&document)
        );

        assert!(
            !DocumentQuery {
                kind: Some(vec![Kind::Note]),
                ..Default::default()
            }
            .matches(&document)
        );
        assert!(
            !DocumentQuery {
                namespace: Some(Namespace::new("/frontend").unwrap()),
                ..Default::default()
            }
            .matches(&document)
        );
        assert!(
            !DocumentQuery {
                tags: vec![Tag::new("nope").unwrap()],
                ..Default::default()
            }
            .matches(&document)
        );
    }

    #[test]
    fn a_status_filter_excludes_documents_with_no_status_at_all() {
        let document = document();
        assert_eq!(document.status(), None);
        let query = DocumentQuery {
            status: Some(vec![DocumentStatus::Active]),
            ..Default::default()
        };
        assert!(!query.matches(&document));
    }
}
