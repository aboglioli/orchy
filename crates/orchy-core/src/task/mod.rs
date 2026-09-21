mod events;
pub mod rollup;
mod status;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use events::{
    TaskBlocked, TaskClaimed, TaskCreated, TaskFinished, TaskReleased, TaskReparented,
    TaskRolledUp, TaskStarted, TaskSuperseded, TaskUnblocked, TaskUpdated,
};
pub use status::TaskStatus;

use crate::actor::{ActorId, Role};
use crate::clock::Clock;
use crate::entity_ref::EntityRef;
use crate::error::{DomainError, Result};
use crate::event::EventCollector;
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;
use crate::pagination::{Page, PageRequest};
use crate::priority::Priority;
use crate::tag::{self, Tag};
use crate::title::Title;

#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn get(&self, id: &Id) -> Result<Option<Task>>;
    async fn find(&self, query: &TaskQuery, page: PageRequest) -> Result<Page<Task>>;
    async fn children_of(&self, parent: &Id) -> Result<Vec<Task>>;
    async fn save(&self, task: &mut Task) -> Result<()>;
    async fn delete(&self, id: &Id) -> Result<()>;

    async fn require(&self, id: &Id) -> Result<Task> {
        self.get(id)
            .await?
            .ok_or_else(|| DomainError::not_found("task", id))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskQuery {
    pub status: Option<Vec<TaskStatus>>,
    pub namespace: Option<Namespace>,
    pub claimed_by: Option<ActorId>,
    pub role: Option<Role>,
    pub parent: Option<Id>,
    pub tags: Vec<Tag>,
    pub text: Option<String>,
}

impl TaskQuery {
    pub fn matches(&self, task: &Task) -> bool {
        if let Some(status) = &self.status
            && !status.contains(&task.status)
        {
            return false;
        }
        if let Some(namespace) = &self.namespace
            && !namespace.contains(&task.namespace)
        {
            return false;
        }
        if let Some(actor) = &self.claimed_by
            && task.claimed_by.as_ref() != Some(actor)
        {
            return false;
        }
        if let Some(role) = &self.role
            && !task.assigned_roles.contains(role)
        {
            return false;
        }
        if let Some(parent) = &self.parent
            && task.parent.as_ref() != Some(parent)
        {
            return false;
        }
        if !self.tags.iter().all(|t| task.tags.contains(t)) {
            return false;
        }
        if let Some(text) = &self.text {
            let needle = text.to_lowercase();
            let haystack = format!(
                "{} {}",
                task.title.as_str().to_lowercase(),
                task.description.to_lowercase()
            );
            if !haystack.contains(&needle) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    id: Id,
    title: Title,
    description: String,
    acceptance_criteria: Option<String>,
    status: TaskStatus,
    priority: Priority,
    namespace: Namespace,
    parent: Option<Id>,
    depends_on: Vec<Id>,
    assigned_roles: Vec<Role>,
    claimed_by: Option<ActorId>,
    claimed_at: Option<DateTime<Utc>>,
    tags: Vec<Tag>,
    refs: Vec<EntityRef>,
    note: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

#[derive(Debug, Clone)]
pub struct RestoreTask {
    pub id: Id,
    pub title: Title,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub status: TaskStatus,
    pub priority: Priority,
    pub namespace: Namespace,
    pub parent: Option<Id>,
    pub depends_on: Vec<Id>,
    pub assigned_roles: Vec<Role>,
    pub claimed_by: Option<ActorId>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub tags: Vec<Tag>,
    pub refs: Vec<EntityRef>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(restore: RestoreTask) -> Self {
        Self {
            id: restore.id,
            title: restore.title,
            description: restore.description,
            acceptance_criteria: restore.acceptance_criteria,
            status: restore.status,
            priority: restore.priority,
            namespace: restore.namespace,
            parent: restore.parent,
            depends_on: restore.depends_on,
            assigned_roles: restore.assigned_roles,
            claimed_by: restore.claimed_by,
            claimed_at: restore.claimed_at,
            tags: restore.tags,
            refs: restore.refs,
            note: restore.note,
            created_at: restore.created_at,
            updated_at: restore.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn create(
        title: Title,
        namespace: Namespace,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Self {
        let now = clock.now();
        let id = Id::generate(ids);
        let mut task = Self::new(RestoreTask {
            id: id.clone(),
            title: title.clone(),
            description: String::new(),
            acceptance_criteria: None,
            status: TaskStatus::Pending,
            priority: Priority::default(),
            namespace: namespace.clone(),
            parent: None,
            depends_on: Vec::new(),
            assigned_roles: Vec::new(),
            claimed_by: None,
            claimed_at: None,
            tags: Vec::new(),
            refs: Vec::new(),
            note: None,
            created_at: now,
            updated_at: now,
        });
        task.collector.collect(TaskCreated {
            id,
            namespace,
            title: title.into(),
            parent: None,
            at: now,
        });
        task
    }

    pub fn attach_to(&mut self, parent: Id, clock: &dyn Clock) -> Result<()> {
        if parent == self.id {
            return Err(DomainError::validation("a task cannot be its own parent"));
        }
        self.parent = Some(parent.clone());
        self.touch(clock);
        self.collector.collect(TaskReparented {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            parent: Some(parent),
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn detach(&mut self, clock: &dyn Clock) {
        self.parent = None;
        self.touch(clock);
        self.collector.collect(TaskReparented {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            parent: None,
            at: self.updated_at,
        });
    }

    pub fn claim(&mut self, by: ActorId, clock: &dyn Clock) -> Result<()> {
        if let Some(holder) = &self.claimed_by
            && holder != &by
            && !self.status.is_terminal()
        {
            return Err(DomainError::conflict(format!(
                "task is already claimed by {holder}"
            )));
        }
        self.status = self.status.transition_to(TaskStatus::Claimed)?;
        let now = clock.now();
        self.claimed_by = Some(by.clone());
        self.claimed_at = Some(now);
        self.updated_at = now;
        self.collector.collect(TaskClaimed {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            by,
            at: now,
        });
        Ok(())
    }

    pub fn release(&mut self, by: &ActorId, clock: &dyn Clock) -> Result<()> {
        match &self.claimed_by {
            Some(holder) if holder == by => {}
            Some(holder) => {
                return Err(DomainError::forbidden(format!(
                    "task is held by {holder}, not {by}"
                )));
            }
            None => return Err(DomainError::conflict("task is not claimed")),
        }
        self.status = self.status.transition_to(TaskStatus::Pending)?;
        let now = clock.now();
        self.claimed_by = None;
        self.claimed_at = None;
        self.updated_at = now;
        self.collector.collect(TaskReleased {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            by: by.clone(),
            at: now,
        });
        Ok(())
    }

    pub fn start(&mut self, clock: &dyn Clock) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::InProgress)?;
        self.touch(clock);
        self.collector.collect(TaskStarted {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn complete(&mut self, note: Option<String>, clock: &dyn Clock) -> Result<()> {
        self.finish(TaskStatus::Completed, note, clock)
    }

    pub fn fail(&mut self, reason: String, clock: &dyn Clock) -> Result<()> {
        self.finish(TaskStatus::Failed, Some(reason), clock)
    }

    pub fn cancel(&mut self, reason: String, clock: &dyn Clock) -> Result<()> {
        self.finish(TaskStatus::Cancelled, Some(reason), clock)
    }

    /// Retired because the work moved elsewhere — unlike cancelling, which says it is not
    /// wanted, or completing, which says it was done here.
    pub fn supersede(
        &mut self,
        by: Vec<Id>,
        reason: Option<String>,
        clock: &dyn Clock,
    ) -> Result<()> {
        if by.is_empty() {
            return Err(DomainError::validation(
                "a superseded task must name what replaces it",
            ));
        }
        if by.contains(&self.id) {
            return Err(DomainError::validation("a task cannot supersede itself"));
        }
        self.status = self.status.transition_to(TaskStatus::Superseded)?;
        self.note = reason.clone();
        self.touch(clock);
        self.collector.collect(TaskSuperseded {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            by,
            reason,
            at: self.updated_at,
        });
        Ok(())
    }

    fn finish(
        &mut self,
        status: TaskStatus,
        note: Option<String>,
        clock: &dyn Clock,
    ) -> Result<()> {
        self.status = self.status.transition_to(status)?;
        self.note = note.clone();
        self.touch(clock);
        self.collector.collect(TaskFinished {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            status,
            note,
            at: self.updated_at,
        });
        Ok(())
    }

    /// Deliberately bypasses `can_transition_to`. A parent sits in `Pending` while its
    /// children work, and `Pending -> Completed` is forbidden for an *agent* because work must
    /// be claimed before it is finished — a rule about skipping steps, not about a status
    /// derived from children. Only the terminal guard applies, so a human's explicit
    /// completion outranks a later derivation.
    pub fn roll_up(&mut self, status: TaskStatus, because: String, clock: &dyn Clock) -> bool {
        if self.status.is_terminal() || self.status == status {
            return false;
        }
        self.status = status;
        self.touch(clock);
        self.collector.collect(TaskRolledUp {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            status,
            because,
            at: self.updated_at,
        });
        true
    }

    pub fn block(&mut self, reason: String, clock: &dyn Clock) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Blocked)?;
        self.touch(clock);
        self.collector.collect(TaskBlocked {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            reason,
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn unblock(&mut self, clock: &dyn Clock) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Pending)?;
        self.touch(clock);
        self.collector.collect(TaskUnblocked {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            at: self.updated_at,
        });
        Ok(())
    }

    pub fn add_dependency(&mut self, on: Id, clock: &dyn Clock) -> Result<()> {
        if on == self.id {
            return Err(DomainError::validation("a task cannot depend on itself"));
        }
        if !self.depends_on.contains(&on) {
            self.depends_on.push(on);
            self.depends_on.sort();
            self.updated_field("depends_on", clock);
        }
        Ok(())
    }

    pub fn remove_dependency(&mut self, on: &Id, clock: &dyn Clock) {
        let before = self.depends_on.len();
        self.depends_on.retain(|d| d != on);
        if self.depends_on.len() != before {
            self.updated_field("depends_on", clock);
        }
    }

    pub fn retitle(&mut self, title: Title, clock: &dyn Clock) {
        self.title = title;
        self.updated_field("title", clock);
    }

    pub fn describe(&mut self, description: String, clock: &dyn Clock) {
        self.description = description;
        self.updated_field("description", clock);
    }

    pub fn set_acceptance_criteria(&mut self, criteria: Option<String>, clock: &dyn Clock) {
        self.acceptance_criteria = criteria.filter(|c| !c.trim().is_empty());
        self.updated_field("acceptance_criteria", clock);
    }

    pub fn set_priority(&mut self, priority: Priority, clock: &dyn Clock) {
        self.priority = priority;
        self.updated_field("priority", clock);
    }

    pub fn assign_roles(&mut self, roles: Vec<Role>, clock: &dyn Clock) {
        self.assigned_roles = roles;
        self.assigned_roles.sort();
        self.updated_field("assigned_roles", clock);
    }

    pub fn move_to(&mut self, namespace: Namespace, clock: &dyn Clock) {
        self.namespace = namespace;
        self.updated_field("namespace", clock);
    }

    pub fn retag(&mut self, add: Vec<Tag>, remove: &[Tag], clock: &dyn Clock) {
        tag::apply(&mut self.tags, add, remove);
        self.updated_field("tags", clock);
    }

    pub fn reference(&mut self, entity: EntityRef, clock: &dyn Clock) {
        if !self.refs.contains(&entity) {
            self.refs.push(entity);
            self.updated_field("refs", clock);
        }
    }

    pub fn touch(&mut self, clock: &dyn Clock) {
        self.updated_at = clock.now();
    }

    fn updated_field(&mut self, field: &str, clock: &dyn Clock) {
        self.touch(clock);
        self.collector.collect(TaskUpdated {
            id: self.id.clone(),
            namespace: self.namespace.clone(),
            field: field.to_owned(),
            at: self.updated_at,
        });
    }

    pub fn drain_events(&mut self) -> Vec<Box<dyn crate::event::DomainEvent>> {
        self.collector.drain()
    }

    pub fn id(&self) -> &Id {
        &self.id
    }
    pub fn title(&self) -> &Title {
        &self.title
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn acceptance_criteria(&self) -> Option<&str> {
        self.acceptance_criteria.as_deref()
    }
    pub fn status(&self) -> TaskStatus {
        self.status
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn parent(&self) -> Option<&Id> {
        self.parent.as_ref()
    }
    pub fn depends_on(&self) -> &[Id] {
        &self.depends_on
    }
    pub fn assigned_roles(&self) -> &[Role] {
        &self.assigned_roles
    }
    pub fn claimed_by(&self) -> Option<&ActorId> {
        self.claimed_by.as_ref()
    }
    pub fn claimed_at(&self) -> Option<DateTime<Utc>> {
        self.claimed_at
    }
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }
    pub fn refs(&self) -> &[EntityRef] {
        &self.refs
    }
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use ulid::Ulid;

    pub(super) struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    pub(super) struct SeqIds(std::sync::atomic::AtomicU64);

    impl IdGenerator for SeqIds {
        fn generate(&self) -> Ulid {
            let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ulid::from_parts(n, n as u128)
        }
    }

    pub(super) fn clock() -> FixedClock {
        FixedClock(DateTime::from_timestamp(1_700_000_000, 0).unwrap())
    }

    pub(super) fn ids() -> SeqIds {
        SeqIds(std::sync::atomic::AtomicU64::new(1))
    }

    pub(super) fn actor(alias: &str) -> ActorId {
        ActorId::new(alias, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    pub(super) fn task() -> Task {
        Task::create(
            Title::new("ship it").unwrap(),
            Namespace::root(),
            &ids(),
            &clock(),
        )
    }

    pub(super) fn claimed() -> Task {
        let mut task = task();
        task.claim(actor("claude"), &clock()).unwrap();
        task
    }

    #[test]
    fn a_new_task_is_pending_unclaimed_and_parentless() {
        let task = task();
        assert_eq!(task.status(), TaskStatus::Pending);
        assert_eq!(task.claimed_by(), None);
        assert_eq!(task.parent(), None);
        assert_eq!(task.priority(), Priority::Normal);
    }

    #[test]
    fn creating_collects_exactly_one_event() {
        let mut task = task();
        let events = task.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), "task.created");
    }

    #[test]
    fn claiming_records_the_holder_and_the_moment() {
        let task = claimed();
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.claimed_by(), Some(&actor("claude")));
        assert!(task.claimed_at().is_some());
    }

    #[test]
    fn a_second_agent_cannot_claim_a_held_task() {
        let mut task = claimed();
        let err = task.claim(actor("codex"), &clock()).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)), "{err:?}");
        assert_eq!(
            task.claimed_by(),
            Some(&actor("claude")),
            "the refused claim must not steal the task"
        );
    }

    #[test]
    fn the_holder_may_re_claim_its_own_task_idempotently() {
        let mut task = claimed();
        task.release(&actor("claude"), &clock()).unwrap();
        assert!(task.claim(actor("claude"), &clock()).is_ok());
    }

    #[test]
    fn only_the_holder_may_release() {
        let mut task = claimed();
        let err = task.release(&actor("codex"), &clock()).unwrap_err();
        assert!(matches!(err, DomainError::Forbidden(_)), "{err:?}");
        assert_eq!(task.status(), TaskStatus::Claimed);
    }

    #[test]
    fn releasing_an_unclaimed_task_is_a_conflict() {
        let mut task = task();
        assert!(matches!(
            task.release(&actor("claude"), &clock()).unwrap_err(),
            DomainError::Conflict(_)
        ));
    }

    #[test]
    fn releasing_returns_the_task_to_the_pool() {
        let mut task = claimed();
        task.release(&actor("claude"), &clock()).unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
        assert_eq!(task.claimed_by(), None);
        assert_eq!(task.claimed_at(), None);
    }

    #[test]
    fn a_pending_task_cannot_be_completed_without_being_claimed() {
        let mut task = task();
        assert!(matches!(
            task.complete(None, &clock()).unwrap_err(),
            DomainError::InvalidTransition { .. }
        ));
    }

    #[test]
    fn completing_stores_the_note_and_emits_a_finished_event() {
        let mut task = claimed();
        task.drain_events();
        task.complete(Some("done".to_owned()), &clock()).unwrap();
        assert_eq!(task.status(), TaskStatus::Completed);
        assert_eq!(task.note(), Some("done"));
        let events = task.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), "task.finished");
    }

    #[test]
    fn a_completed_task_is_absorbing() {
        let mut task = claimed();
        task.complete(None, &clock()).unwrap();
        assert!(task.fail("nope".to_owned(), &clock()).is_err());
        assert!(task.start(&clock()).is_err());
        assert!(task.cancel("nope".to_owned(), &clock()).is_err());
        assert_eq!(task.status(), TaskStatus::Completed);
    }

    #[test]
    fn roll_up_moves_an_open_parent_and_emits_its_own_topic() {
        let mut task = claimed();
        task.drain_events();
        assert!(task.roll_up(TaskStatus::Completed, "all done".to_owned(), &clock()));
        assert_eq!(task.status(), TaskStatus::Completed);
        let events = task.drain_events();
        assert_eq!(events[0].topic().as_str(), "task.rolled_up");
    }

    #[test]
    fn roll_up_never_moves_a_terminal_parent() {
        let mut task = claimed();
        task.complete(Some("by hand".to_owned()), &clock()).unwrap();
        task.drain_events();

        assert!(
            !task.roll_up(
                TaskStatus::Failed,
                "a straggler failed".to_owned(),
                &clock()
            ),
            "a human's explicit completion outranks a later derivation"
        );
        assert_eq!(task.status(), TaskStatus::Completed);
        assert!(task.drain_events().is_empty(), "a refused rollup is silent");
    }

    #[test]
    fn roll_up_moves_a_pending_parent_even_though_an_agent_could_not() {
        let mut task = task();
        assert_eq!(task.status(), TaskStatus::Pending);
        assert!(
            !TaskStatus::Pending.can_transition_to(TaskStatus::Completed),
            "an agent may not finish unclaimed work"
        );
        assert!(
            task.roll_up(
                TaskStatus::Completed,
                "all subtasks done".to_owned(),
                &clock()
            ),
            "but a parent is completed by derivation, not by being worked on"
        );
        assert_eq!(task.status(), TaskStatus::Completed);
    }

    #[test]
    fn roll_up_to_the_status_already_held_is_a_no_op() {
        let mut task = task();
        assert!(!task.roll_up(TaskStatus::Pending, "x".to_owned(), &clock()));
        assert!(task.drain_events().len() == 1, "only the creation event");
    }

    #[test]
    fn a_task_cannot_be_its_own_parent_or_dependency() {
        let mut task = task();
        let own = task.id().clone();
        assert!(task.attach_to(own.clone(), &clock()).is_err());
        assert!(task.add_dependency(own, &clock()).is_err());
    }

    #[test]
    fn attaching_records_the_parent_on_the_child_only() {
        let mut child = task();
        let parent = Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap();
        child.attach_to(parent.clone(), &clock()).unwrap();
        assert_eq!(child.parent(), Some(&parent));
        child.detach(&clock());
        assert_eq!(child.parent(), None);
    }

    #[test]
    fn dependencies_are_a_sorted_set() {
        let mut task = task();
        let a = Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap();
        let b = Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        task.add_dependency(a.clone(), &clock()).unwrap();
        task.add_dependency(b.clone(), &clock()).unwrap();
        task.add_dependency(a.clone(), &clock()).unwrap();
        assert_eq!(task.depends_on(), &[b, a.clone()]);
        task.remove_dependency(&a, &clock());
        assert_eq!(task.depends_on().len(), 1);
    }

    #[test]
    fn blocking_requires_unblocking_before_a_claim() {
        let mut task = task();
        task.block("waiting on api".to_owned(), &clock()).unwrap();
        assert_eq!(task.status(), TaskStatus::Blocked);
        assert!(task.claim(actor("claude"), &clock()).is_err());
        task.unblock(&clock()).unwrap();
        assert!(task.claim(actor("claude"), &clock()).is_ok());
    }

    #[test]
    fn query_filters_compose() {
        let mut task = task();
        task.retag(vec![Tag::new("rust").unwrap()], &[], &clock());
        task.claim(actor("claude"), &clock()).unwrap();

        let query = TaskQuery {
            status: Some(vec![TaskStatus::Claimed]),
            claimed_by: Some(actor("claude")),
            tags: vec![Tag::new("rust").unwrap()],
            ..Default::default()
        };
        assert!(query.matches(&task));

        let missing_tag = TaskQuery {
            tags: vec![Tag::new("go").unwrap()],
            ..Default::default()
        };
        assert!(!missing_tag.matches(&task));
    }

    #[test]
    fn query_by_namespace_includes_the_subtree() {
        let mut task = task();
        task.move_to(Namespace::new("/backend/auth").unwrap(), &clock());
        let query = TaskQuery {
            namespace: Some(Namespace::new("/backend").unwrap()),
            ..Default::default()
        };
        assert!(query.matches(&task), "a parent namespace sees its children");
    }

    #[test]
    fn query_text_searches_title_and_description() {
        let mut task = task();
        task.describe("rotate the signing key".to_owned(), &clock());
        let hit = TaskQuery {
            text: Some("SIGNING".to_owned()),
            ..Default::default()
        };
        assert!(hit.matches(&task), "text search is case-insensitive");
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert!(TaskQuery::default().matches(&task()));
    }
}

#[cfg(test)]
mod supersede_tests {
    use super::tests::*;
    use super::*;

    #[test]
    fn superseding_retires_the_task_and_names_its_replacements() {
        let mut task = task();
        task.drain_events();
        let replacements = vec![
            Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap(),
            Id::new("01CX5ZZKBKACTAV9WEVGEMMVRZ").unwrap(),
        ];
        task.supersede(replacements, Some("split out".to_owned()), &clock())
            .unwrap();

        assert_eq!(task.status(), TaskStatus::Superseded);
        assert!(task.status().is_terminal());
        assert!(task.status().is_neutral());
        assert_eq!(task.note(), Some("split out"));

        let events = task.drain_events();
        assert_eq!(events[0].topic().as_str(), "task.superseded");
    }

    #[test]
    fn superseding_needs_at_least_one_replacement() {
        let mut task = task();
        assert!(task.supersede(vec![], None, &clock()).is_err());
        assert_eq!(
            task.status(),
            TaskStatus::Pending,
            "the refusal changes nothing"
        );
    }

    #[test]
    fn a_task_cannot_supersede_itself() {
        let mut task = task();
        let own = task.id().clone();
        assert!(task.supersede(vec![own], None, &clock()).is_err());
    }

    #[test]
    fn a_finished_task_cannot_be_superseded() {
        let mut task = claimed();
        task.complete(None, &clock()).unwrap();
        assert!(
            task.supersede(
                vec![Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap()],
                None,
                &clock()
            )
            .is_err(),
            "work already done was not replaced"
        );
    }

    #[test]
    fn unstarted_work_can_be_replaced_without_being_claimed() {
        let mut task = task();
        assert!(
            task.supersede(
                vec![Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap()],
                None,
                &clock()
            )
            .is_ok()
        );
    }
}
