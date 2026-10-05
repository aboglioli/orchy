mod events;
mod name;

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use events::{
    SkillCreated, SkillFieldSet, SkillMoved, SkillRenamed, SkillRestored, SkillRetired,
    SkillTagged, SkillWritten,
};
pub use name::{SkillName, Summary};

use crate::body::Body;
use crate::clock::Clock;
use crate::content_hash;
use crate::document::{Frontmatter, validate_field_name};
use crate::error::{DomainError, Result};
use crate::event::{DomainEvent, EventCollector};
use crate::graph::Relation;
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;
use crate::tag::{self, Tag};

#[async_trait]
pub trait SkillStore: Send + Sync {
    async fn get(&self, id: &Id) -> Result<Option<Skill>>;
    async fn all(&self) -> Result<Vec<Skill>>;
    async fn save(&self, skill: &mut Skill) -> Result<()>;

    async fn require(&self, id: &Id) -> Result<Skill> {
        self.get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("skill", id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillStatus {
    Active,
    Retired,
}

impl SkillStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Retired => "retired",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    id: Id,
    name: SkillName,
    summary: Summary,
    namespace: Namespace,
    status: SkillStatus,
    tags: Vec<Tag>,
    frontmatter: Frontmatter,
    body: Body,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

#[derive(Debug, Clone)]
pub struct RestoreSkill {
    pub id: Id,
    pub name: SkillName,
    pub summary: Summary,
    pub namespace: Namespace,
    pub status: SkillStatus,
    pub tags: Vec<Tag>,
    pub frontmatter: Frontmatter,
    pub body: Body,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Skill {
    pub fn new(restore: RestoreSkill) -> Self {
        Self {
            id: restore.id,
            name: restore.name,
            summary: restore.summary,
            namespace: restore.namespace,
            status: restore.status,
            tags: restore.tags,
            frontmatter: restore.frontmatter,
            body: restore.body,
            created_at: restore.created_at,
            updated_at: restore.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn create(
        name: SkillName,
        summary: Summary,
        namespace: Namespace,
        body: Body,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Self {
        let now = clock.now();
        let id = Id::generate(ids);
        let mut skill = Self::new(RestoreSkill {
            id: id.clone(),
            name: name.clone(),
            summary: summary.clone(),
            namespace: namespace.clone(),
            status: SkillStatus::Active,
            tags: Vec::new(),
            frontmatter: Frontmatter::new(),
            body,
            created_at: now,
            updated_at: now,
        });
        skill.collector.collect(SkillCreated {
            id,
            namespace,
            name: name.into(),
            summary: summary.into(),
            at: now,
        });
        skill
    }

    pub fn edit(&mut self, body: Body, clock: &dyn Clock) {
        self.body = body;
        self.touch(clock);
        self.collector.collect(SkillWritten {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            name: self.name.to_string(),
            at: self.updated_at,
        });
    }

    pub fn describe(&mut self, summary: Summary, clock: &dyn Clock) {
        self.summary = summary;
        self.touch(clock);
        self.collector.collect(SkillWritten {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            name: self.name.to_string(),
            at: self.updated_at,
        });
    }

    pub fn rename(&mut self, name: SkillName, clock: &dyn Clock) {
        if name == self.name {
            return;
        }
        let from = std::mem::replace(&mut self.name, name);
        self.touch(clock);
        self.collector.collect(SkillRenamed {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from: from.into(),
            to: self.name.to_string(),
            at: self.updated_at,
        });
    }

    pub fn move_to(&mut self, namespace: Namespace, clock: &dyn Clock) {
        if namespace == self.namespace {
            return;
        }
        let from = std::mem::replace(&mut self.namespace, namespace);
        self.touch(clock);
        self.collector.collect(SkillMoved {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            from,
            name: self.name.to_string(),
            at: self.updated_at,
        });
    }

    pub fn retire(&mut self, clock: &dyn Clock) -> Result<()> {
        if self.status == SkillStatus::Retired {
            return Err(DomainError::invalid_transition("retired", "retired"));
        }
        self.status = SkillStatus::Retired;
        self.touch(clock);
        self.collector.collect(SkillRetired {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            name: self.name.to_string(),
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn restore(&mut self, clock: &dyn Clock) -> Result<()> {
        if self.status == SkillStatus::Active {
            return Err(DomainError::invalid_transition("active", "active"));
        }
        self.status = SkillStatus::Active;
        self.touch(clock);
        self.collector.collect(SkillRestored {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            name: self.name.to_string(),
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn tag(&mut self, add: Vec<Tag>, remove: &[Tag], clock: &dyn Clock) {
        let before = self.tags.clone();
        tag::apply(&mut self.tags, add, remove);
        if self.tags == before {
            return;
        }
        self.touch(clock);
        self.collector.collect(SkillTagged {
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

    pub fn set_field(&mut self, field: &str, value: Value, clock: &dyn Clock) -> Result<()> {
        validate_field_name(field)?;
        if let Some(owner) = Relation::owner_of_field(field) {
            return Err(DomainError::forbidden(format!("`{field}` {owner}")));
        }
        if let Some(command) = managed_field(field) {
            return Err(DomainError::forbidden(format!(
                "`{field}` is maintained by orchy; use {command}"
            )));
        }
        self.frontmatter.set(field, value.clone());
        self.touch(clock);
        self.field_changed(field, Some(value));
        Ok(())
    }

    pub fn remove_field(&mut self, field: &str, clock: &dyn Clock) -> Result<()> {
        validate_field_name(field)?;
        if let Some(owner) = Relation::owner_of_field(field) {
            return Err(DomainError::forbidden(format!("`{field}` {owner}")));
        }
        if let Some(command) = managed_field(field) {
            return Err(DomainError::forbidden(format!(
                "`{field}` is maintained by orchy; use {command}"
            )));
        }
        self.frontmatter.remove(field);
        self.touch(clock);
        self.field_changed(field, None);
        Ok(())
    }

    fn field_changed(&mut self, field: &str, value: Option<Value>) {
        self.collector.collect(SkillFieldSet {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            field: field.to_owned(),
            value,
            at: self.updated_at,
        });
    }

    fn touch(&mut self, clock: &dyn Clock) {
        self.updated_at = clock.now();
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn name(&self) -> &SkillName {
        &self.name
    }

    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    pub fn status(&self) -> SkillStatus {
        self.status
    }

    pub fn is_active(&self) -> bool {
        self.status == SkillStatus::Active
    }

    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }

    pub fn frontmatter(&self) -> &Frontmatter {
        &self.frontmatter
    }

    pub fn content_hash(&self) -> String {
        content_hash::content_hash(
            &[
                ("name", self.name.as_str()),
                ("summary", self.summary.as_str()),
                ("body", self.body.as_str()),
            ],
            &self.frontmatter,
        )
    }

    pub fn ensure_unchanged(&self, expected: Option<&str>) -> Result<()> {
        content_hash::ensure_matches(&self.content_hash(), expected)
    }

    pub fn body(&self) -> &Body {
        &self.body
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn drain_events(&mut self) -> Vec<Box<dyn DomainEvent>> {
        self.collector.drain()
    }
}

pub fn managed_field(field: &str) -> Option<&'static str> {
    match field {
        "id" => Some("(ids are immutable)"),
        "type" => Some("(a skill is always a skill)"),
        "name" => Some("`orchy skill write` under the new name"),
        "summary" => Some("`orchy skill write <name> --summary ...`"),
        "namespace" => Some("`orchy skill write <name> --namespace ...`"),
        "status" => Some("`orchy skill retire` / `orchy skill restore`"),
        "tags" => Some("`orchy skill write <name> --tag ...`"),
        "created" | "updated" => Some("(timestamps are orchy's)"),
        _ => None,
    }
}

pub fn in_scope(skills: &[Skill], namespace: &Namespace) -> Vec<Skill> {
    let mut depth_of: BTreeMap<String, usize> = BTreeMap::new();
    depth_of.insert(namespace.as_str().to_owned(), 0);
    for (distance, ancestor) in namespace.ancestors().iter().enumerate() {
        depth_of.insert(ancestor.as_str().to_owned(), distance + 1);
    }

    let mut nearest: BTreeMap<&str, (usize, &Skill)> = BTreeMap::new();
    for skill in skills.iter().filter(|s| s.is_active()) {
        let Some(distance) = depth_of.get(skill.namespace().as_str()) else {
            continue;
        };
        nearest
            .entry(skill.name().as_str())
            .and_modify(|found| {
                if *distance < found.0 {
                    *found = (*distance, skill);
                }
            })
            .or_insert((*distance, skill));
    }

    let mut resolved: Vec<Skill> = nearest.into_values().map(|(_, s)| s.clone()).collect();
    resolved.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use crate::id::IdGenerator;

    struct FixedClock;
    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            DateTime::from_timestamp(1_700_000_000, 0).unwrap()
        }
    }

    struct Ids;
    impl IdGenerator for Ids {
        fn generate(&self) -> ulid::Ulid {
            ulid::Ulid::new()
        }
    }

    fn skill(name: &str, namespace: &str) -> Skill {
        Skill::create(
            SkillName::new(name).unwrap(),
            Summary::new("how we do it here").unwrap(),
            Namespace::new(namespace).unwrap(),
            Body::new("## When to use\n\nalways"),
            &Ids,
            &FixedClock,
        )
    }

    type Mutation = fn(&mut Skill);
    type Case = (&'static str, fn() -> Skill, Mutation);

    #[test]
    fn every_change_to_a_skill_records_an_event() {
        let fresh = || skill("commits", "/");
        let retired = || {
            let mut s = skill("commits", "/");
            s.retire(&FixedClock).unwrap();
            s
        };
        let cases: Vec<Case> = vec![
            ("edit", fresh, |s| s.edit(Body::new("other"), &FixedClock)),
            ("describe", fresh, |s| {
                s.describe(Summary::new("new").unwrap(), &FixedClock)
            }),
            ("rename", fresh, |s| {
                s.rename(SkillName::new("renamed").unwrap(), &FixedClock)
            }),
            ("move_to", fresh, |s| {
                s.move_to(Namespace::new("/web").unwrap(), &FixedClock)
            }),
            ("retire", fresh, |s| s.retire(&FixedClock).unwrap()),
            ("restore", retired, |s| s.restore(&FixedClock).unwrap()),
            ("tag", fresh, |s| {
                s.tag(vec![Tag::new("x").unwrap()], &[], &FixedClock)
            }),
            ("set_field", fresh, |s| {
                s.set_field("owner", serde_json::json!("alan"), &FixedClock)
                    .unwrap()
            }),
            ("remove_field", fresh, |s| {
                s.remove_field("owner", &FixedClock).unwrap()
            }),
        ];
        for (label, start, mutate) in cases {
            let mut skill = start();
            skill.drain_events();
            mutate(&mut skill);
            assert!(
                !skill.drain_events().is_empty(),
                "`{label}` changed the skill without recording an event"
            );
        }
    }

    #[test]
    fn creating_records_the_name_and_summary_agents_will_scan() {
        let mut created = skill("code-review", "/backend");
        let events = created.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), "skill.created");
        assert_eq!(created.status(), SkillStatus::Active);
    }

    #[test]
    fn a_retired_skill_is_still_readable_but_teaches_nobody() {
        let mut retired = skill("old-way", "/");
        retired.retire(&FixedClock).unwrap();
        assert!(!retired.is_active());
        assert!(
            retired.retire(&FixedClock).is_err(),
            "retiring twice is a mistake worth naming"
        );

        let scoped = in_scope(&[retired], &Namespace::root());
        assert!(
            scoped.is_empty(),
            "a retired skill is out of scope everywhere"
        );
    }

    #[test]
    fn an_agent_inherits_what_the_namespaces_above_it_declare() {
        let skills = [skill("review", "/"), skill("migrations", "/backend")];
        let deep = Namespace::new("/backend/auth").unwrap();

        let names: Vec<String> = in_scope(&skills, &deep)
            .iter()
            .map(|s| s.name().to_string())
            .collect();
        assert_eq!(names, vec!["migrations", "review"]);
    }

    #[test]
    fn a_sibling_namespace_teaches_nothing() {
        let skills = [skill("migrations", "/backend")];
        let elsewhere = Namespace::new("/frontend").unwrap();
        assert!(in_scope(&skills, &elsewhere).is_empty());
    }

    #[test]
    fn the_nearest_declaration_of_a_name_is_the_one_that_applies() {
        let mut general = skill("review", "/");
        general.describe(
            Summary::new("the organisation's rule").unwrap(),
            &FixedClock,
        );
        let mut local = skill("review", "/backend");
        local.describe(
            Summary::new("what backend does instead").unwrap(),
            &FixedClock,
        );

        let scoped = in_scope(&[general, local], &Namespace::new("/backend").unwrap());
        assert_eq!(scoped.len(), 1, "one name, one skill in force");
        assert_eq!(scoped[0].summary().as_str(), "what backend does instead");
    }

    #[test]
    fn renaming_and_moving_are_recorded_but_only_when_something_changed() {
        let mut moved = skill("review", "/");
        moved.drain_events();

        moved.rename(SkillName::new("review").unwrap(), &FixedClock);
        moved.move_to(Namespace::root(), &FixedClock);
        assert!(moved.drain_events().is_empty(), "a no-op is not an event");

        moved.rename(SkillName::new("code-review").unwrap(), &FixedClock);
        moved.move_to(Namespace::new("/backend").unwrap(), &FixedClock);
        let topics: Vec<String> = moved
            .drain_events()
            .iter()
            .map(|e| e.topic().as_str().to_owned())
            .collect();
        assert_eq!(topics, vec!["skill.renamed", "skill.moved"]);
    }
}
