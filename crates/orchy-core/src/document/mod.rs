mod events;
mod frontmatter;
mod kind;

use std::sync::OnceLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use events::{
    DocumentCreated, DocumentFieldSet, DocumentMoved, DocumentPromoted, DocumentRetitled,
    DocumentRetyped, DocumentSectionReplaced, DocumentStatusChanged, DocumentSuperseded,
    DocumentTagged, DocumentWritten,
};
pub use frontmatter::{Frontmatter, validate_field_name};
pub use kind::{DocumentStatus, Kind};

use crate::body::Body;
use crate::clock::Clock;
use crate::content_hash;
use crate::error::{DomainError, Result};
use crate::event::{DomainEvent, EventCollector};
use crate::graph::Relation;
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;
use crate::pagination::{Page, PageRequest};
use crate::tag::{self, Tag};
use crate::title::Title;

const REJECTED_BECAUSE: &str = "rejected_because";

#[async_trait]
pub trait DocumentStore: Send + Sync {
    async fn get(&self, id: &Id) -> Result<Option<Document>>;
    /// Never paged: callers rely on seeing every match.
    async fn matching(&self, query: &DocumentQuery) -> Result<Vec<Document>>;
    async fn save(&self, document: &mut Document) -> Result<()>;

    async fn find(&self, query: &DocumentQuery, page: PageRequest) -> Result<Page<Document>> {
        Ok(Page::slice(self.matching(query).await?, page))
    }

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
    /// Computed on first use: listing thousands of documents rarely needs it.
    #[serde(skip)]
    content_hash: OnceLock<String>,
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
        Self {
            id: restore.id,
            kind: restore.kind,
            title: restore.title,
            namespace: restore.namespace,
            status: restore.status,
            tags: restore.tags,
            frontmatter: restore.frontmatter,
            body: restore.body,
            content_hash: OnceLock::new(),
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
            status: Some(kind.initial_status()),
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
            content_hash: document.content_hash().to_owned(),
            at: now,
        });
        document
    }

    pub fn edit(&mut self, body: Body, clock: &dyn Clock) {
        let prev_hash = self.content_hash().to_owned();
        self.body = body;
        self.rehash(clock);
        self.collector.collect(DocumentWritten {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            content_hash: self.content_hash().to_owned(),
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
        nth: Option<usize>,
        content: &str,
        clock: &dyn Clock,
    ) -> Result<()> {
        let body = self.body.replace_section(heading, nth, content)?;
        let prev_hash = self.content_hash().to_owned();
        self.body = body;
        self.rehash(clock);
        self.collector.collect(DocumentSectionReplaced {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            heading: heading.to_owned(),
            content_hash: self.content_hash().to_owned(),
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
        validate_field_name(field)?;
        if let Some(owner) = Relation::owner_of_field(field) {
            return Err(DomainError::forbidden(format!("`{field}` {owner}")));
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
        if self.status == Some(status) {
            return Ok(());
        }
        self.ensure_can_become(status)?;
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
        if title == self.title {
            return;
        }
        let from = std::mem::replace(&mut self.title, title);
        self.rehash(clock);
        self.collector.collect(DocumentRetitled {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from: from.to_string(),
            to: self.title.to_string(),
            at: self.updated_at,
        });
    }

    pub fn retype(&mut self, kind: Kind, clock: &dyn Clock) -> Result<()> {
        if kind == self.kind {
            return Ok(());
        }
        if kind.is_candidate() != self.kind.is_candidate() {
            return Err(DomainError::conflict(if self.is_candidate() {
                "a candidate becomes canon through `orchy promote`, or is turned down with `orchy reject`"
            } else {
                "canon does not become a proposal again; write a new candidate instead"
            }));
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
        self.ensure_can_become(DocumentStatus::Promoted)?;
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

    /// For a candidate that became a skill: the document stays as the record of the proposal.
    pub fn mark_promoted(&mut self, clock: &dyn Clock) -> Result<()> {
        if !self.is_candidate() {
            return Err(DomainError::conflict(
                "only a candidate can be promoted; this document is already canon",
            ));
        }
        self.set_status(DocumentStatus::Promoted, clock)
    }

    pub fn reject(&mut self, reason: Option<String>, clock: &dyn Clock) -> Result<()> {
        if !self.is_candidate() {
            return Err(DomainError::conflict(
                "only a candidate can be rejected; archive or supersede canon instead",
            ));
        }
        self.ensure_can_become(DocumentStatus::Rejected)?;
        if let Some(reason) = reason {
            self.frontmatter
                .set(REJECTED_BECAUSE, Value::String(reason));
        }
        self.set_status(DocumentStatus::Rejected, clock)
    }

    pub fn supersede(&mut self, by: &Document, clock: &dyn Clock) -> Result<()> {
        if by.id == self.id {
            return Err(DomainError::validation(
                "a document cannot supersede itself",
            ));
        }
        if by.is_candidate() || by.status.is_some_and(DocumentStatus::is_retired) {
            return Err(DomainError::conflict(format!(
                "`{}` is {}; only canon still in force can replace a document",
                by.id,
                by.status
                    .map_or_else(|| by.kind.to_string(), |s| s.to_string())
            )));
        }
        let by = by.id.clone();
        self.kind.validate_status(DocumentStatus::Superseded)?;
        self.ensure_can_become(DocumentStatus::Superseded)?;
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

    /// A document without a status (written by hand) may take any status its kind allows.
    fn ensure_can_become(&self, target: DocumentStatus) -> Result<()> {
        match self.status {
            Some(current) if !current.can_transition_to(target) => {
                Err(DomainError::invalid_transition(current, target))
            }
            _ => Ok(()),
        }
    }

    pub fn retag(&mut self, add: Vec<Tag>, remove: &[Tag], clock: &dyn Clock) {
        let before = self.tags.clone();
        tag::apply(&mut self.tags, add, remove);
        if self.tags == before {
            return;
        }
        self.rehash(clock);
        self.collector.collect(DocumentTagged {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            added: self
                .tags
                .iter()
                .filter(|t| !before.contains(t))
                .map(ToString::to_string)
                .collect(),
            removed: before
                .iter()
                .filter(|t| !self.tags.contains(t))
                .map(ToString::to_string)
                .collect(),
            at: self.updated_at,
        });
    }

    fn rehash(&mut self, clock: &dyn Clock) {
        self.updated_at = clock.now();
        self.content_hash = OnceLock::new();
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
        self.content_hash.get_or_init(|| {
            content_hash::content_hash(
                &[("title", self.title.as_str()), ("body", self.body.as_str())],
                &self.frontmatter,
            )
        })
    }

    /// Refuses a write made against an older version than the one stored.
    pub fn ensure_unchanged(&self, expected: Option<&str>) -> Result<()> {
        content_hash::ensure_matches(self.content_hash(), expected)
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
        "status" => Some("orchy archive / orchy unarchive / orchy supersede / orchy promote"),
        "type" => Some("orchy retype"),
        "namespace" => Some("orchy ns move"),
        "id" => Some("(ids are immutable)"),
        "tags" => Some("orchy tag"),
        "title" => Some("orchy retitle"),
        "created" | "updated" => Some("(timestamps are orchy's)"),
        _ => None,
    }
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

    fn replacement() -> Document {
        Document::create(
            Kind::Decision,
            Title::new("Rotate signing keys, again").unwrap(),
            Namespace::new("/backend").unwrap(),
            Body::new("Move to EdDSA."),
            &SeqIds(std::sync::atomic::AtomicU64::new(99)),
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

    type Mutation = fn(&mut Document);
    type Case = (&'static str, fn() -> Document, Mutation);

    #[test]
    fn every_change_to_a_document_records_an_event() {
        let cases: Vec<Case> = vec![
            ("edit", document, |d| d.edit(Body::new("new"), &clock())),
            ("append", document, |d| d.append("more", &clock())),
            ("replace_section", document, |d| {
                d.replace_section("Decision", None, "EdDSA", &clock())
                    .unwrap()
            }),
            ("replace_once", document, |d| {
                d.replace_once("HS256", "RS256", &clock()).unwrap()
            }),
            ("set_field", document, |d| {
                d.set_field("reviewer", json!("alan"), &clock()).unwrap()
            }),
            ("set_status", document, |d| {
                d.set_status(DocumentStatus::Archived, &clock()).unwrap()
            }),
            ("retitle", document, |d| {
                d.retitle(Title::new("Other").unwrap(), &clock())
            }),
            ("retype", document, |d| {
                d.retype(Kind::Note, &clock()).unwrap()
            }),
            ("move_to", document, |d| {
                d.move_to(Namespace::new("/web").unwrap(), &clock())
            }),
            ("promote", candidate, |d| {
                d.promote(Kind::Decision, Namespace::root(), &clock())
                    .unwrap()
            }),
            ("mark_promoted", candidate, |d| {
                d.mark_promoted(&clock()).unwrap()
            }),
            ("supersede", document, |d| {
                d.supersede(&replacement(), &clock()).unwrap()
            }),
            ("retag", document, |d| {
                d.retag(vec![Tag::new("x").unwrap()], &[], &clock())
            }),
        ];
        for (label, start, mutate) in cases {
            let mut document = start();
            document.drain_events();
            mutate(&mut document);
            assert!(
                !document.drain_events().is_empty(),
                "`{label}` changed the document without recording an event"
            );
        }
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
    fn what_replaced_a_document_cannot_be_undone_by_archiving_it() {
        let mut document = document();
        document.supersede(&replacement(), &clock()).unwrap();
        let archived = document.set_status(DocumentStatus::Archived, &clock());
        assert!(
            matches!(archived, Err(DomainError::InvalidTransition { .. })),
            "{archived:?}"
        );
        let again = document.supersede(&replacement(), &clock());
        assert!(
            matches!(again, Err(DomainError::InvalidTransition { .. })),
            "{again:?}"
        );
    }

    #[test]
    fn an_archived_document_comes_back_active_and_setting_the_same_status_is_a_no_op() {
        let mut document = document();
        document
            .set_status(DocumentStatus::Archived, &clock())
            .unwrap();
        document.drain_events();
        document
            .set_status(DocumentStatus::Archived, &clock())
            .unwrap();
        assert!(document.drain_events().is_empty());
        document
            .set_status(DocumentStatus::Active, &clock())
            .unwrap();
        assert_eq!(document.status(), Some(DocumentStatus::Active));
    }

    #[test]
    fn a_rejected_candidate_cannot_be_promoted() {
        let mut candidate = candidate();
        candidate.reject(None, &clock()).unwrap();
        let promoted = candidate.promote(Kind::Decision, Namespace::root(), &clock());
        assert!(
            matches!(promoted, Err(DomainError::InvalidTransition { .. })),
            "{promoted:?}"
        );
    }

    #[test]
    fn superseding_sets_the_status_and_emits_the_link_in_one_step() {
        let mut document = document();
        document.drain_events();
        document.supersede(&replacement(), &clock()).unwrap();

        assert_eq!(document.status().unwrap().as_str(), "superseded");
        let events = document.drain_events();
        assert_eq!(events[0].topic().as_str(), "document.superseded");
    }

    #[test]
    fn a_document_cannot_supersede_itself() {
        let mut document = document();
        let same = document.clone();
        assert!(document.supersede(&same, &clock()).is_err());
    }

    #[test]
    fn only_canon_still_in_force_can_replace_a_document() {
        let mut retired = replacement();
        retired
            .set_status(DocumentStatus::Archived, &clock())
            .unwrap();
        assert!(matches!(
            document().supersede(&retired, &clock()),
            Err(DomainError::Conflict(_))
        ));
        assert!(
            document().supersede(&candidate(), &clock()).is_err(),
            "a proposal does not replace canon until it is promoted"
        );
    }

    #[test]
    fn retyping_never_crosses_between_canon_and_proposals() {
        let mut superseded = document();
        superseded.supersede(&replacement(), &clock()).unwrap();
        assert!(
            superseded.retype(Kind::Candidate, &clock()).is_err(),
            "crossing would wipe the final status and let it come back"
        );
        assert!(candidate().retype(Kind::Note, &clock()).is_err());
        superseded.retype(Kind::Note, &clock()).unwrap();
        assert_eq!(superseded.status(), Some(DocumentStatus::Superseded));
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
            .replace_section("Decision", None, "Move to EdDSA.", &clock())
            .unwrap();
        assert!(document.body().as_str().contains("EdDSA"));
        assert!(document.body().as_str().contains("We use HS256"));
        assert!(!document.body().as_str().contains("Move to RS256"));
    }

    #[test]
    fn replace_section_reports_a_missing_heading_as_not_found() {
        let mut document = document();
        let err = document
            .replace_section("Nope", None, "x", &clock())
            .unwrap_err();
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
        let written = document();
        let document = Document::new(RestoreDocument {
            id: written.id().clone(),
            kind: *written.kind(),
            title: written.title().clone(),
            namespace: written.namespace().clone(),
            status: None,
            tags: Vec::new(),
            frontmatter: Frontmatter::new(),
            body: written.body().clone(),
            created_at: written.created_at(),
            updated_at: written.updated_at(),
        });
        let query = DocumentQuery {
            status: Some(vec![DocumentStatus::Active]),
            ..Default::default()
        };
        assert!(!query.matches(&document));
    }

    #[test]
    fn canon_starts_active_and_a_candidate_starts_proposed() {
        assert_eq!(document().status(), Some(DocumentStatus::Active));
        assert_eq!(candidate().status(), Some(DocumentStatus::Proposed));
    }

    #[test]
    fn only_a_candidate_can_be_rejected_and_the_reason_is_kept() {
        let mut proposal = candidate();
        proposal
            .reject(Some("duplicate".to_owned()), &clock())
            .unwrap();
        assert_eq!(proposal.status(), Some(DocumentStatus::Rejected));
        assert_eq!(
            proposal.frontmatter().string(REJECTED_BECAUSE),
            Some("duplicate")
        );
        assert!(document().reject(None, &clock()).is_err());
    }
}
