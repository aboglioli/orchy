use std::sync::Arc;

use orchy_application::{
    AddDependencyCommand, Application, ApplicationDeps, ArchiveTaskCommand, ClaimTaskCommand,
    CompleteTaskCommand, GetTaskCommand, ListEdgesCommand, ListTasksCommand, MergeTasksCommand,
    MoveTaskCommand, PostTaskCommand, ReleaseTaskCommand, RemoveDependencyCommand,
    SplitTaskCommand, SubtaskInput, UnarchiveTaskCommand, UpdateTaskCommand,
};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::api_key::{
    ApiKey, ApiKeyGenerator, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, PlainApiKey,
};
use orchy_core::error::{Error, Result};
use orchy_core::graph::EdgeStore;
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::{
    HashedPassword, OrgMembershipStore, PasswordHasher, PlainPassword, UserId, UserStore,
};
use orchy_store_memory::*;

struct NoopHasher;

impl PasswordHasher for NoopHasher {
    fn hash(&self, plain: &PlainPassword) -> Result<HashedPassword> {
        HashedPassword::new(plain.as_str())
    }

    fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> Result<()> {
        if plain.as_str() == hashed.as_str() {
            Ok(())
        } else {
            Err(Error::InvalidInput("password mismatch".into()))
        }
    }
}

struct NoopApiKeyGenerator;

impl ApiKeyGenerator for NoopApiKeyGenerator {
    fn generate(
        &self,
        org_id: &OrganizationId,
        user_id: Option<UserId>,
        name: String,
    ) -> Result<(PlainApiKey, ApiKey)> {
        let plain = PlainApiKey::new(
            "sk_0000000000000000000000000000000000000000000000000000000000000000".into(),
        )?;
        let hashed = HashedApiKey::new("noop-hash".into())?;
        let prefix = ApiKeyPrefix::new("sk_00000".into())?;
        let suffix = ApiKeySuffix::new("0000".into())?;
        let key = ApiKey::new(org_id.clone(), name, hashed, prefix, suffix, user_id);
        Ok((plain, key))
    }

    fn hash(&self, _plain: &PlainApiKey) -> HashedApiKey {
        HashedApiKey::new("noop-hash".into()).unwrap()
    }
}

fn mem() -> Arc<MemoryState> {
    Arc::new(MemoryState::new())
}

fn build_app(s: &Arc<MemoryState>) -> Application {
    Application::new(ApplicationDeps {
        agents: Arc::new(MemoryAgentStore::new(s.clone())) as Arc<dyn AgentStore>,
        tasks: Arc::new(MemoryTaskStore::new(s.clone())) as Arc<dyn TaskStore>,
        projects: Arc::new(MemoryProjectStore::new(s.clone())) as Arc<dyn ProjectStore>,
        knowledge: Arc::new(MemoryKnowledgeStore::new(s.clone())) as Arc<dyn KnowledgeStore>,
        messages: Arc::new(MemoryMessageStore::new(s.clone())) as Arc<dyn MessageStore>,
        locks: Arc::new(MemoryLockStore::new(s.clone())) as Arc<dyn LockStore>,
        namespaces: Arc::new(MemoryNamespaceStore::new(s.clone())),
        orgs: Arc::new(MemoryOrganizationStore::new(s.clone())),
        edges: Arc::new(MemoryEdgeStore::new(s.clone())) as Arc<dyn EdgeStore>,
        embeddings: None,
        reader_factory: Arc::new(MemoryReaderFactory::new(s.clone())),
        users: Arc::new(MemoryUserStore::new(s.clone())) as Arc<dyn UserStore>,
        memberships: Arc::new(MemoryOrgMembershipStore::new(s.clone()))
            as Arc<dyn OrgMembershipStore>,
        token_encoder: None,
        hasher: Arc::new(NoopHasher),
        api_keys: Arc::new(MemoryApiKeyStore::new(s.clone())) as Arc<dyn ApiKeyStore>,
        api_key_generator: Arc::new(NoopApiKeyGenerator),
    })
}

async fn seed_agent(s: &Arc<MemoryState>, org: &str, alias: &str) -> AgentId {
    let mut agent = Agent::register(
        OrganizationId::new(org).unwrap(),
        ProjectId::try_from("proj").unwrap(),
        Namespace::root(),
        Alias::new(alias).unwrap(),
        vec![],
        String::new(),
        None,
        Default::default(),
        None,
    )
    .unwrap();
    let id = agent.id().clone();
    MemoryAgentStore::new(s.clone())
        .save(&mut agent)
        .await
        .unwrap();
    id
}

fn post_cmd(org: &str, title: &str) -> PostTaskCommand {
    PostTaskCommand {
        org_id: org.into(),
        project: "proj".into(),
        namespace: None,
        title: title.into(),
        description: "desc".into(),
        acceptance_criteria: None,
        priority: None,
        assigned_roles: None,
        created_by: None,
        parent_id: None,
        depends_on: None,
    }
}

#[tokio::test]
async fn post_task_creates_pending_task() {
    let s = mem();
    let app = build_app(&s);

    let dto = app
        .post_task
        .execute(post_cmd("org", "my task"))
        .await
        .unwrap();

    assert_eq!(dto.title, "my task");
    assert_eq!(dto.status, "pending");
    assert_eq!(dto.org_id, "org");

    let fetched = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dto.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(fetched.id, dto.id);
    assert_eq!(fetched.status, "pending");
}

#[tokio::test]
async fn post_task_with_parent_creates_spawns_edge() {
    let s = mem();
    let app = build_app(&s);

    let parent = app
        .post_task
        .execute(post_cmd("org", "parent"))
        .await
        .unwrap();

    let child = app
        .post_task
        .execute(PostTaskCommand {
            parent_id: Some(parent.id.clone()),
            ..post_cmd("org", "child")
        })
        .await
        .unwrap();

    let edges = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "org".into(),
            rel_type: Some("spawns".into()),
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert!(
        edges
            .items
            .iter()
            .any(|e| e.from_id == parent.id && e.to_id == child.id),
        "expected a spawns edge from parent to child"
    );
}

#[tokio::test]
async fn post_task_with_unmet_deps_is_blocked() {
    let s = mem();
    let app = build_app(&s);

    let dep = app.post_task.execute(post_cmd("org", "dep")).await.unwrap();

    let task = app
        .post_task
        .execute(PostTaskCommand {
            depends_on: Some(vec![dep.id.clone()]),
            ..post_cmd("org", "blocked task")
        })
        .await
        .unwrap();

    assert_eq!(task.status, "blocked");

    let edges = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "org".into(),
            rel_type: Some("depends_on".into()),
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert!(
        edges
            .items
            .iter()
            .any(|e| e.from_id == task.id && e.to_id == dep.id),
        "expected a depends_on edge from task to dep"
    );
}

#[tokio::test]
async fn claim_task_pending_to_claimed() {
    let s = mem();
    let app = build_app(&s);

    let agent_id = seed_agent(&s, "org", "worker-1").await;
    let task = app
        .post_task
        .execute(post_cmd("org", "claimable"))
        .await
        .unwrap();

    let claimed = app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: None,
        })
        .await
        .unwrap();

    assert_eq!(claimed.status, "claimed");
    assert_eq!(
        claimed.assigned_to.as_deref(),
        Some(agent_id.to_string().as_str())
    );
}

#[tokio::test]
async fn complete_last_child_auto_completes_blocked_parent() {
    let s = mem();
    let app = build_app(&s);

    let agent_id = seed_agent(&s, "org", "worker-2").await;

    let parent = app
        .post_task
        .execute(post_cmd("org", "parent"))
        .await
        .unwrap();

    let (parent_dto, children) = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.id.clone(),
            subtasks: vec![SubtaskInput {
                title: "child-1".into(),
                description: "desc".into(),
                acceptance_criteria: None,
                priority: None,
                assigned_roles: None,
                depends_on: None,
            }],
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(parent_dto.status, "blocked");
    assert_eq!(children.len(), 1);

    let child_id = children[0].id.clone();

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: child_id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();

    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: child_id.clone(),
            org_id: "org".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    let parent_after = app
        .get_task
        .execute(GetTaskCommand {
            task_id: parent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();

    assert_eq!(
        parent_after.status, "completed",
        "parent should auto-complete when all children are done"
    );
}

#[tokio::test]
async fn split_task_blocks_parent_and_spawns_children() {
    let s = mem();
    let app = build_app(&s);

    let parent = app
        .post_task
        .execute(post_cmd("org", "big task"))
        .await
        .unwrap();

    let (parent_dto, children) = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.id.clone(),
            subtasks: vec![
                SubtaskInput {
                    title: "sub-a".into(),
                    description: "desc".into(),
                    acceptance_criteria: None,
                    priority: None,
                    assigned_roles: None,
                    depends_on: None,
                },
                SubtaskInput {
                    title: "sub-b".into(),
                    description: "desc".into(),
                    acceptance_criteria: None,
                    priority: None,
                    assigned_roles: None,
                    depends_on: None,
                },
            ],
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(parent_dto.status, "blocked");
    assert_eq!(children.len(), 2);

    let edges = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "org".into(),
            rel_type: Some("spawns".into()),
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();

    let spawns_from_parent: Vec<_> = edges
        .items
        .iter()
        .filter(|e| e.from_id == parent.id)
        .collect();
    assert_eq!(
        spawns_from_parent.len(),
        2,
        "parent should have 2 spawns edges"
    );
}

#[tokio::test]
async fn merge_tasks_cancels_sources_and_creates_merged_from_edges() {
    let s = mem();
    let app = build_app(&s);

    let task_a = app
        .post_task
        .execute(post_cmd("org", "task-a"))
        .await
        .unwrap();
    let task_b = app
        .post_task
        .execute(post_cmd("org", "task-b"))
        .await
        .unwrap();

    let (merged, cancelled) = app
        .merge_tasks
        .execute(MergeTasksCommand {
            org_id: "org".into(),
            task_ids: vec![task_a.id.clone(), task_b.id.clone()],
            title: "merged task".into(),
            description: "combined".into(),
            acceptance_criteria: None,
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(merged.title, "merged task");
    assert_eq!(merged.status, "pending");
    assert_eq!(cancelled.len(), 2);
    assert!(cancelled.iter().all(|t| t.status == "cancelled"));

    let edges = app
        .list_edges
        .execute(ListEdgesCommand {
            org_id: "org".into(),
            rel_type: Some("merged_from".into()),
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();

    let merged_from_edges: Vec<_> = edges
        .items
        .iter()
        .filter(|e| e.from_id == merged.id)
        .collect();
    assert_eq!(
        merged_from_edges.len(),
        2,
        "merged task should have 2 merged_from edges"
    );

    let source_ids: Vec<&str> = merged_from_edges.iter().map(|e| e.to_id.as_str()).collect();
    assert!(source_ids.contains(&task_a.id.as_str()));
    assert!(source_ids.contains(&task_b.id.as_str()));
}

#[tokio::test]
async fn complete_task_cascade_unblocks_blocked_dependent() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "worker-cascade").await;

    let prereq = app
        .post_task
        .execute(post_cmd("org", "prereq"))
        .await
        .unwrap();
    let dependent = app
        .post_task
        .execute(PostTaskCommand {
            depends_on: Some(vec![prereq.id.clone()]),
            ..post_cmd("org", "dependent")
        })
        .await
        .unwrap();
    assert_eq!(dependent.status, "blocked");

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: prereq.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: prereq.id.clone(),
            org_id: "org".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    let after = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(
        after.status, "pending",
        "dependent must auto-unblock after sole prereq completes"
    );
}

#[tokio::test]
async fn complete_task_does_not_unblock_when_other_deps_still_pending() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "worker-multi-dep").await;

    let dep_a = app
        .post_task
        .execute(post_cmd("org", "dep-a"))
        .await
        .unwrap();
    let dep_b = app
        .post_task
        .execute(post_cmd("org", "dep-b"))
        .await
        .unwrap();
    let dependent = app
        .post_task
        .execute(PostTaskCommand {
            depends_on: Some(vec![dep_a.id.clone(), dep_b.id.clone()]),
            ..post_cmd("org", "dependent")
        })
        .await
        .unwrap();
    assert_eq!(dependent.status, "blocked");

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: dep_a.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: dep_a.id.clone(),
            org_id: "org".into(),
            summary: None,
            links: vec![],
        })
        .await
        .unwrap();

    let after_partial = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(
        after_partial.status, "blocked",
        "dependent must stay blocked while one dep still pending"
    );

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: dep_b.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: dep_b.id.clone(),
            org_id: "org".into(),
            summary: None,
            links: vec![],
        })
        .await
        .unwrap();

    let after_all = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(after_all.status, "pending");
}

#[tokio::test]
async fn post_task_records_created_by_when_supplied() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "creator").await;

    let task = app
        .post_task
        .execute(PostTaskCommand {
            created_by: Some(agent_id.to_string()),
            ..post_cmd("org", "owned task")
        })
        .await
        .unwrap();

    assert_eq!(
        task.created_by.as_deref(),
        Some(agent_id.to_string().as_str()),
        "created_by must be set when caller passes the agent id"
    );
}

#[tokio::test]
async fn add_dependency_rejects_self_cycle() {
    let s = mem();
    let app = build_app(&s);
    let t = app
        .post_task
        .execute(post_cmd("org", "self-cycle"))
        .await
        .unwrap();

    let err = app
        .add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: t.id.clone(),
            dependency_id: t.id.clone(),
        })
        .await
        .expect_err("task cannot depend on itself");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got: {err:?}"
    );
}

#[tokio::test]
async fn add_dependency_blocks_pending_task() {
    let s = mem();
    let app = build_app(&s);

    let prereq = app.post_task.execute(post_cmd("org", "prereq")).await.unwrap();
    let dependent = app
        .post_task
        .execute(post_cmd("org", "dependent"))
        .await
        .unwrap();
    assert_eq!(dependent.status, "pending");

    let after = app
        .add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: dependent.id.clone(),
            dependency_id: prereq.id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(after.status, "blocked", "dependent must be blocked after adding dependency on pending task");
}

#[tokio::test]
async fn add_dependency_cascade_unblocks_on_complete() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "worker-cascade-add").await;

    let prereq = app.post_task.execute(post_cmd("org", "prereq")).await.unwrap();
    let dependent = app
        .post_task
        .execute(post_cmd("org", "dependent"))
        .await
        .unwrap();

    app.add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: dependent.id.clone(),
            dependency_id: prereq.id.clone(),
        })
        .await
        .unwrap();

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: prereq.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();

    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: prereq.id.clone(),
            org_id: "org".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    let after = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();

    assert_eq!(
        after.status, "pending",
        "dependent must auto-unblock after sole prereq completes"
    );
}

#[tokio::test]
async fn remove_dependency_unblocks_task() {
    let s = mem();
    let app = build_app(&s);

    let prereq = app.post_task.execute(post_cmd("org", "prereq")).await.unwrap();
    let dependent = app
        .post_task
        .execute(post_cmd("org", "dependent"))
        .await
        .unwrap();

    app.add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: dependent.id.clone(),
            dependency_id: prereq.id.clone(),
        })
        .await
        .unwrap();

    let blocked = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(blocked.status, "blocked");

    app.remove_dependency
        .execute(RemoveDependencyCommand {
            org_id: "org".into(),
            task_id: dependent.id.clone(),
            dependency_id: prereq.id.clone(),
        })
        .await
        .unwrap();

    let after = app
        .get_task
        .execute(GetTaskCommand {
            task_id: dependent.id.clone(),
            org_id: "org".into(),
            relations: None,
        })
        .await
        .unwrap();

    assert_eq!(after.status, "pending", "dependent must unblock when sole dependency is removed");
}

// TODO: indirect cycle detection not wired yet — add_dependency doesn't traverse the graph
// Enable this test once cycle detection is implemented
#[tokio::test]
#[ignore]
async fn add_dependency_rejects_indirect_cycle() {
    let s = mem();
    let app = build_app(&s);
    let a = app.post_task.execute(post_cmd("org", "a")).await.unwrap();
    let b = app.post_task.execute(post_cmd("org", "b")).await.unwrap();

    app.add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: a.id.clone(),
            dependency_id: b.id.clone(),
        })
        .await
        .unwrap();

    let err = app
        .add_dependency
        .execute(AddDependencyCommand {
            org_id: "org".into(),
            task_id: b.id.clone(),
            dependency_id: a.id.clone(),
        })
        .await
        .expect_err("indirect cycle must be rejected");
    assert!(
        matches!(err, Error::Conflict(_)),
        "expected Conflict, got: {err:?}"
    );
}

// ─── task archive and unarchive ─────────────────────────────────────────────

#[tokio::test]
async fn task_archive_and_unarchive() {
    let s = mem();
    let app = build_app(&s);

    let task = app
        .post_task
        .execute(post_cmd("org", "archive me"))
        .await
        .unwrap();
    assert!(!task.archived);

    // Must be completed before archive
    let agent_id = seed_agent(&s, "org", "archiver").await;
    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: task.id.clone(),
            org_id: "org".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    let archived = app
        .archive_task
        .execute(ArchiveTaskCommand {
            org_id: "org".into(),
            task_id: task.id.clone(),
            reason: Some("test".into()),
        })
        .await
        .unwrap();
    assert!(archived.archived);

    let restored = app
        .unarchive_task
        .execute(UnarchiveTaskCommand {
            org_id: "org".into(),
            task_id: task.id,
        })
        .await
        .unwrap();
    assert!(!restored.archived);
}

// ─── task release returns to pending ────────────────────────────────────────

#[tokio::test]
async fn task_release_returns_to_pending() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "releaser").await;

    let task = app
        .post_task
        .execute(post_cmd("org", "release me"))
        .await
        .unwrap();
    let task_id = task.id.clone();

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: task_id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(
        app.get_task
            .execute(GetTaskCommand {
                task_id: task_id.clone(),
                org_id: "org".into(),
                relations: None,
            })
            .await
            .unwrap()
            .task
            .status,
        "in_progress"
    );

    let released = app
        .release_task
        .execute(ReleaseTaskCommand { task_id })
        .await
        .unwrap();
    assert_eq!(released.status, "pending");
    assert!(
        released.assigned_to.is_none(),
        "assigned_to must be cleared on release"
    );
}

// ─── task move to namespace ─────────────────────────────────────────────────

#[tokio::test]
async fn task_move_to_namespace() {
    let s = mem();
    let app = build_app(&s);

    let task = app
        .post_task
        .execute(post_cmd("org", "move me"))
        .await
        .unwrap();
    assert_eq!(task.namespace, "/");

    let moved = app
        .move_task
        .execute(MoveTaskCommand {
            task_id: task.id,
            new_namespace: "/backend".into(),
        })
        .await
        .unwrap();
    assert_eq!(moved.namespace, "/backend");
}

// ─── task update fields ─────────────────────────────────────────────────────

#[tokio::test]
async fn task_update_fields() {
    let s = mem();
    let app = build_app(&s);

    let task = app
        .post_task
        .execute(post_cmd("org", "original"))
        .await
        .unwrap();

    let updated = app
        .update_task
        .execute(UpdateTaskCommand {
            task_id: task.id,
            title: Some("updated title".into()),
            description: Some("updated desc".into()),
            acceptance_criteria: Some("must pass".into()),
            priority: Some("high".into()),
        })
        .await
        .unwrap();
    assert_eq!(updated.title, "updated title");
    assert_eq!(updated.description, "updated desc");
    assert_eq!(updated.acceptance_criteria, Some("must pass".into()));
    assert_eq!(updated.priority, "high");
}

// ─── list tasks filters by status and namespace ─────────────────────────────

#[tokio::test]
async fn list_tasks_filters_by_status_and_namespace() {
    let s = mem();
    let app = build_app(&s);
    let agent_id = seed_agent(&s, "org", "lister").await;

    let frontend = app
        .post_task
        .execute(PostTaskCommand {
            namespace: Some("/frontend".into()),
            ..post_cmd("org", "frontend task")
        })
        .await
        .unwrap();
    let backend = app
        .post_task
        .execute(PostTaskCommand {
            namespace: Some("/backend".into()),
            ..post_cmd("org", "backend task")
        })
        .await
        .unwrap();

    // Complete backend task
    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: backend.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "org".into(),
            start: Some(true),
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: backend.id,
            org_id: "org".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    // List by namespace
    let namespace_page = app
        .list_tasks
        .execute(ListTasksCommand {
            org_id: "org".into(),
            project: None,
            namespace: Some("/frontend".into()),
            status: None,
            assigned_to: None,
            tag: None,
            after: None,
            limit: None,
            archived: None,
        })
        .await
        .unwrap();
    assert_eq!(namespace_page.items.len(), 1);
    assert_eq!(namespace_page.items[0].id, frontend.id);

    // List by status
    let completed_page = app
        .list_tasks
        .execute(ListTasksCommand {
            org_id: "org".into(),
            project: None,
            namespace: None,
            status: Some("completed".into()),
            assigned_to: None,
            tag: None,
            after: None,
            limit: None,
            archived: None,
        })
        .await
        .unwrap();
    assert!(completed_page.items.iter().all(|t| t.status == "completed"));
}
