use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use orchy_application::{
    AddDependency, AddDependencyCommand, AddEdge, AddEdgeCommand, AppendKnowledge,
    AppendKnowledgeCommand, AssignTask, AssignTaskCommand, CancelTask, CancelTaskCommand,
    ChangeKnowledgeKind, ChangeKnowledgeKindCommand, ChangeRoles, ChangeRolesCommand, CheckMailbox,
    CheckMailboxCommand, ClaimMessage, ClaimTask, ClaimTaskCommand, CompleteTask,
    CompleteTaskCommand, ConsolidateKnowledge, ConsolidateKnowledgeCommand, DelegateTask,
    DelegateTaskCommand, FailTask, FailTaskCommand, GetAgentSummary, GetAgentSummaryCommand,
    GetNextTask, GetNextTaskCommand, GetProject, GetProjectCommand, GetTask, GetTaskCommand,
    ListEdges, ListEdgesCommand, LockResource, LockResourceCommand, MarkRead, MarkReadCommand,
    MaterializeNeighborhood, MaterializeNeighborhoodCommand, MergeTasks, MergeTasksCommand,
    MoveKnowledge, MoveKnowledgeCommand, PostTask, PostTaskCommand, PromoteKnowledge,
    PromoteKnowledgeCommand, ReadKnowledge, ReadKnowledgeCommand, RegisterAgent,
    RegisterAgentCommand, ReleaseTask, ReleaseTaskCommand, RemoveDependency,
    RemoveDependencyCommand, RenameAlias, RenameAliasCommand, RenameKnowledge,
    RenameKnowledgeCommand, ReplaceTask, ReplaceTaskCommand, ResolveAgent, ResolveAgentCommand,
    SendMessage, SendMessageCommand, StartTask, StartTaskCommand, SubtaskInput, SwitchContext,
    SwitchContextCommand, TagKnowledge, TagKnowledgeCommand, TagTask, TagTaskCommand, TouchTask,
    TouchTaskCommand, UnclaimMessage, UntagKnowledge, UntagKnowledgeCommand, UntagTask,
    UntagTaskCommand, WriteKnowledge, WriteKnowledgeCommand,
};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::graph::{EdgeStore, RelationOptions, RelationType};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::resource_ref::{ResourceKind, ResourceRef};
use orchy_core::task::TaskStore;
use orchy_core::user::{
    Email, HashedPassword, OrgMembership, OrgMembershipStore, OrgRole, RestoreUser, User, UserId,
    UserStore,
};
use orchy_store_memory::*;

fn org() -> OrganizationId {
    OrganizationId::new("default").unwrap()
}
fn proj() -> ProjectId {
    ProjectId::try_from("test").unwrap()
}
fn ns() -> Namespace {
    Namespace::root()
}

fn state() -> Arc<MemoryState> {
    Arc::new(MemoryState::new())
}

fn agents(s: &Arc<MemoryState>) -> Arc<MemoryAgentStore> {
    Arc::new(MemoryAgentStore::new(s.clone()))
}
fn tasks(s: &Arc<MemoryState>) -> Arc<MemoryTaskStore> {
    Arc::new(MemoryTaskStore::new(s.clone()))
}
fn messages(s: &Arc<MemoryState>) -> Arc<MemoryMessageStore> {
    Arc::new(MemoryMessageStore::new(s.clone()))
}
fn edges(s: &Arc<MemoryState>) -> Arc<MemoryEdgeStore> {
    Arc::new(MemoryEdgeStore::new(s.clone()))
}
fn locks(s: &Arc<MemoryState>) -> Arc<MemoryLockStore> {
    Arc::new(MemoryLockStore::new(s.clone()))
}
fn users(s: &Arc<MemoryState>) -> Arc<MemoryUserStore> {
    Arc::new(MemoryUserStore::new(s.clone()))
}
fn memberships(s: &Arc<MemoryState>) -> Arc<MemoryOrgMembershipStore> {
    Arc::new(MemoryOrgMembershipStore::new(s.clone()))
}
fn projects(s: &Arc<MemoryState>) -> Arc<MemoryProjectStore> {
    Arc::new(MemoryProjectStore::new(s.clone()))
}
fn knowledge(s: &Arc<MemoryState>) -> Arc<MemoryKnowledgeStore> {
    Arc::new(MemoryKnowledgeStore::new(s.clone()))
}

async fn register(agent_store: &Arc<MemoryAgentStore>, alias: &str) -> AgentId {
    let alias = Alias::new(alias).unwrap();
    let mut a = Agent::register(
        org(),
        proj(),
        ns(),
        alias,
        vec![],
        String::new(),
        None,
        Default::default(),
        None,
    )
    .unwrap();
    let id = a.id().clone();
    agent_store.save(&mut a).await.unwrap();
    id
}

async fn register_with_app(s: &Arc<MemoryState>, alias: &str) -> AgentId {
    register_with_auth_user(s, alias, None).await
}

async fn register_with_auth_user(
    s: &Arc<MemoryState>,
    alias: &str,
    auth_user_id: Option<&UserId>,
) -> AgentId {
    let register = RegisterAgent::new(
        agents(s) as Arc<dyn AgentStore>,
        messages(s) as Arc<dyn MessageStore>,
        tasks(s) as Arc<dyn TaskStore>,
    );
    let resp = register
        .execute(RegisterAgentCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            alias: alias.into(),
            roles: vec![],
            description: String::new(),
            agent_type: None,
            metadata: Default::default(),
            auth_user_id: auth_user_id.map(ToString::to_string),
        })
        .await
        .unwrap();
    AgentId::from_str(&resp.agent.id).unwrap()
}

async fn seed_user_membership(s: &Arc<MemoryState>, email: &str) -> UserId {
    let user_id = UserId::new();
    let mut user = User::restore(RestoreUser {
        id: user_id,
        email: Email::new(email).unwrap(),
        password_hash: HashedPassword::new("hashed-password").unwrap(),
        is_active: true,
        is_platform_admin: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    });
    users(s).save(&mut user).await.unwrap();

    let membership = OrgMembership::new(user_id, org(), OrgRole::Member);
    memberships(s).save(&membership).await.unwrap();
    user_id
}

async fn inbox_for(s: &Arc<MemoryState>, id: &AgentId, _roles: &[String]) -> Vec<String> {
    let check = CheckMailbox::new(
        messages(s) as Arc<dyn MessageStore>,
        agents(s) as Arc<dyn AgentStore>,
    );
    let page = check
        .execute(CheckMailboxCommand {
            agent_id: id.to_string(),
            org_id: "default".into(),
            project: "test".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();
    page.items.into_iter().map(|m| m.body).collect()
}

// ─── send_message resolves alias ─────────────────────────────────────────────

#[tokio::test]
async fn send_message_resolves_alias_for_from() {
    let s = state();
    let a = agents(&s);
    let sender_id = register(&a, "sender").await;
    let receiver_id = register(&a, "receiver").await;

    let send = SendMessage::new(
        a.clone() as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );

    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: "sender".into(),
            to: receiver_id.to_string(),
            body: "hello".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .expect("send with alias should succeed");
    assert_eq!(msg.from, sender_id.to_string());

    let msg2 = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: "@receiver".to_string(),
            body: "hello2".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .expect("send with UUID should succeed");
    assert_eq!(msg2.from, sender_id.to_string());
}

// ─── lock_resource resolves alias ────────────────────────────────────────────

#[tokio::test]
async fn lock_resource_resolves_alias() {
    let s = state();
    let a = agents(&s);
    let agent_id = register(&a, "locker").await;

    let lock = LockResource::new(
        a.clone() as Arc<dyn AgentStore>,
        locks(&s) as Arc<dyn LockStore>,
    );

    let r = lock
        .execute(LockResourceCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            name: "test-lock-1".into(),
            holder_agent_id: "locker".into(),
            ttl_secs: Some(300),
        })
        .await
        .expect("lock with alias should succeed");
    assert_eq!(r.holder, agent_id.to_string());

    let r2 = lock
        .execute(LockResourceCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            name: "test-lock-2".into(),
            holder_agent_id: agent_id.to_string(),
            ttl_secs: Some(300),
        })
        .await
        .expect("lock with UUID should succeed");
    assert_eq!(r2.holder, agent_id.to_string());
}

// ─── edge idempotent add returns real ID ────────────────────────────────────

#[tokio::test]
async fn edge_add_idempotent_returns_real_id() {
    let s = state();
    let add = AddEdge::new(edges(&s) as Arc<dyn EdgeStore>);

    let r1 = add
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "knowledge".into(),
            from_id: "src".into(),
            to_kind: "knowledge".into(),
            to_id: "dst".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: true,
        })
        .await
        .expect("first add should succeed");
    assert!(!r1.id.is_empty(), "edge ID must not be empty on first add");

    let r2 = add
        .execute(AddEdgeCommand {
            org_id: "default".into(),
            from_kind: "knowledge".into(),
            from_id: "src".into(),
            to_kind: "knowledge".into(),
            to_id: "dst".into(),
            rel_type: "related_to".into(),
            created_by: None,
            if_not_exists: true,
        })
        .await
        .expect("idempotent add should succeed");
    assert!(
        !r2.id.is_empty(),
        "idempotent add must return real ID, not empty string"
    );
    assert_eq!(r2.id, r1.id, "idempotent add should return same edge ID");
}

// ─── reconnect stability: same alias resumes same UUID ──────────────────────

#[tokio::test]
async fn reconnect_with_same_alias_preserves_uuid_and_mailbox() {
    let s = state();
    let id1 = register_with_app(&s, "reconnect-agent").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: id1.to_string(),
        to: "@reconnect-agent".into(),
        body: "direct-before-reconnect".into(),
        reply_to: None,
        refs: vec![],
    })
    .await
    .unwrap();

    let id2 = register_with_app(&s, "reconnect-agent").await;
    assert_eq!(id1, id2, "same alias must resume same agent UUID");

    let bodies = inbox_for(&s, &id1, &[]).await;
    assert!(bodies.contains(&"direct-before-reconnect".to_string()));
}

// ─── alias rename preserves direct messages ───────────────────────────────

#[tokio::test]
async fn alias_rename_preserves_direct_messages() {
    let s = state();
    let agent_id = register_with_app(&s, "original-alias").await;
    let sender_id = register_with_app(&s, "sender").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: sender_id.to_string(),
        to: "@original-alias".into(),
        body: "to-original".into(),
        reply_to: None,
        refs: vec![],
    })
    .await
    .unwrap();

    let rename = RenameAlias::new(agents(&s) as Arc<dyn AgentStore>);
    rename
        .execute(RenameAliasCommand {
            agent_id: agent_id.to_string(),
            new_alias: "renamed-alias".into(),
        })
        .await
        .unwrap();

    let bodies = inbox_for(&s, &agent_id, &[]).await;
    assert!(bodies.contains(&"to-original".to_string()));
}

// ─── namespace mark-read ──────────────────────────────────────────────────

#[tokio::test]
async fn namespace_targeted_message_can_be_marked_read() {
    let s = state();
    let sender_id = register_with_app(&s, "ns-sender").await;
    let recipient_id = register_with_app(&s, "ns-recipient").await;

    {
        let a = agents(&s);
        let mut agent = a.find_by_id(&recipient_id).await.unwrap().unwrap();
        agent
            .switch_context(None, Namespace::try_from("/backend".to_string()).unwrap())
            .unwrap();
        a.save(&mut agent).await.unwrap();
    }

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: "ns:/backend".into(),
            body: "ns-message".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let mark_read = MarkRead::new(
        messages(&s) as Arc<dyn MessageStore>,
        agents(&s) as Arc<dyn AgentStore>,
    );
    mark_read
        .execute(MarkReadCommand {
            agent_id: recipient_id.to_string(),
            message_ids: vec![msg.id],
        })
        .await
        .expect("mark_read should succeed for namespace-targeted message");

    let bodies = inbox_for(&s, &recipient_id, &[]).await;
    assert!(!bodies.contains(&"ns-message".to_string()));
}

// ─── future logical audience visibility ─────────────────────────────────────

#[tokio::test]
async fn future_agent_sees_old_role_message() {
    let s = state();
    let sender_id = register_with_app(&s, "role-sender").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: sender_id.to_string(),
        to: "role:reviewer".into(),
        body: "role-message".into(),
        reply_to: None,
        refs: vec![],
    })
    .await
    .unwrap();

    let recipient_id = register_with_app(&s, "future-reviewer").await;
    {
        let a = agents(&s);
        let mut agent = a.find_by_id(&recipient_id).await.unwrap().unwrap();
        agent.change_roles(vec!["reviewer".to_string()]).unwrap();
        a.save(&mut agent).await.unwrap();
    }

    let bodies = inbox_for(&s, &recipient_id, &["reviewer".to_string()]).await;
    assert!(bodies.contains(&"role-message".to_string()));
}

#[tokio::test]
async fn future_agent_sees_old_broadcast_message() {
    let s = state();
    let sender_id = register_with_app(&s, "bc-sender").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: sender_id.to_string(),
        to: "broadcast".into(),
        body: "broadcast-message".into(),
        reply_to: None,
        refs: vec![],
    })
    .await
    .unwrap();

    let recipient_id = register_with_app(&s, "future-listener").await;

    let bodies = inbox_for(&s, &recipient_id, &[]).await;
    assert!(bodies.contains(&"broadcast-message".to_string()));
}

#[tokio::test]
async fn future_agent_sees_old_namespace_message() {
    let s = state();
    let sender_id = register_with_app(&s, "ns-sender2").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: sender_id.to_string(),
        to: "ns:/backend".into(),
        body: "ns-old-message".into(),
        reply_to: None,
        refs: vec![],
    })
    .await
    .unwrap();

    let recipient_id = register_with_app(&s, "future-backend-dev").await;
    {
        let a = agents(&s);
        let mut agent = a.find_by_id(&recipient_id).await.unwrap().unwrap();
        agent
            .switch_context(None, Namespace::try_from("/backend".to_string()).unwrap())
            .unwrap();
        a.save(&mut agent).await.unwrap();
    }

    let bodies = inbox_for(&s, &recipient_id, &[]).await;
    assert!(bodies.contains(&"ns-old-message".to_string()));
}

// ─── alias uniqueness per org/project ───────────────────────────────────────

#[tokio::test]
async fn direct_mark_read_hides_message_from_inbox() {
    let s = state();
    let sender_id = register_with_app(&s, "direct-sender").await;
    let recipient_id = register_with_app(&s, "direct-recipient").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: recipient_id.to_string(),
            body: "direct-read".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let bodies = inbox_for(&s, &recipient_id, &[]).await;
    assert!(bodies.contains(&"direct-read".to_string()));

    let mark_read = MarkRead::new(
        messages(&s) as Arc<dyn MessageStore>,
        agents(&s) as Arc<dyn AgentStore>,
    );
    mark_read
        .execute(MarkReadCommand {
            agent_id: recipient_id.to_string(),
            message_ids: vec![msg.id],
        })
        .await
        .unwrap();

    let bodies = inbox_for(&s, &recipient_id, &[]).await;
    assert!(!bodies.contains(&"direct-read".to_string()));
}

#[tokio::test]
async fn mark_read_fails_for_unknown_message() {
    let s = state();
    let recipient_id = register_with_app(&s, "mark-read-recipient").await;

    let mark_read = MarkRead::new(
        messages(&s) as Arc<dyn MessageStore>,
        agents(&s) as Arc<dyn AgentStore>,
    );
    let result = mark_read
        .execute(MarkReadCommand {
            agent_id: recipient_id.to_string(),
            message_ids: vec!["019dbe0b-e8de-7930-a9c8-67e522bb2bd6".into()],
        })
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn get_next_task_returns_unassigned_pending_task() {
    let s = state();
    let agent_id = register_with_app(&s, "next-agent").await;

    let post_task = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    post_task
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "unassigned task".into(),
            description: "available to anyone".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let next = GetNextTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let found = next
        .execute(GetNextTaskCommand {
            org_id: Some("default".into()),
            project: Some("test".into()),
            namespace: None,
            roles: vec![],
            claim: Some(true),
            agent_id: Some(agent_id.to_string()),
        })
        .await
        .unwrap();

    assert!(found.is_some());
    assert_eq!(found.unwrap().title, "unassigned task");
}

#[tokio::test]
async fn get_project_synthesizes_missing_project_projection() {
    let s = state();
    let _agent_id = register_with_app(&s, "project-agent").await;

    let get_project = GetProject::new(projects(&s) as Arc<dyn ProjectStore>);
    let project = get_project
        .execute(GetProjectCommand {
            org_id: "default".into(),
            project: "test".into(),
        })
        .await
        .unwrap();

    assert_eq!(project.id, "test");
    assert_eq!(project.org_id, "default");
}

#[tokio::test]
async fn get_agent_summary_includes_synthesized_project() {
    let s = state();
    let agent_id = register_with_app(&s, "summary-agent").await;

    let summary = GetAgentSummary::new(
        agents(&s) as Arc<dyn AgentStore>,
        projects(&s) as Arc<dyn ProjectStore>,
        messages(&s) as Arc<dyn MessageStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        knowledge(&s) as Arc<dyn orchy_core::knowledge::KnowledgeStore>,
    );
    let result = summary
        .execute(GetAgentSummaryCommand {
            org_id: "default".into(),
            agent_id: agent_id.to_string(),
        })
        .await
        .unwrap();

    assert!(result.project.is_some());
    assert_eq!(result.project.unwrap().id, "test");
}

#[tokio::test]
async fn user_target_messages_reach_future_user_owned_agents() {
    let s = state();
    let sender_id = register_with_app(&s, "user-target-sender").await;
    let user_id = seed_user_membership(&s, "member@example.com").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: format!("user:{user_id}"),
            body: "hello future agent".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let user_agent_id = register_with_auth_user(&s, "user-target-recipient", Some(&user_id)).await;
    let bodies = inbox_for(&s, &user_agent_id, &[]).await;

    assert!(bodies.contains(&"hello future agent".to_string()));
    assert_eq!(msg.to, format!("user:{user_id}"));
}

#[tokio::test]
async fn unclaim_succeeds_for_claimed_user_target_message() {
    let s = state();
    let sender_id = register_with_app(&s, "claim-sender").await;
    let user_id = seed_user_membership(&s, "claimer@example.com").await;
    let claimer_id = register_with_auth_user(&s, "claim-recipient-a", Some(&user_id)).await;
    let sibling_id = register_with_auth_user(&s, "claim-recipient-b", Some(&user_id)).await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: format!("user:{user_id}"),
            body: "claim me".into(),
            reply_to: None,
            refs: vec![],
        })
        .await
        .unwrap();

    let claim = ClaimMessage::new(messages(&s) as Arc<dyn MessageStore>);
    claim
        .execute(claimer_id.clone(), msg.id.parse().unwrap())
        .await
        .unwrap();

    let sibling_bodies = inbox_for(&s, &sibling_id, &[]).await;
    assert!(!sibling_bodies.contains(&"claim me".to_string()));

    let unclaim = UnclaimMessage::new(messages(&s) as Arc<dyn MessageStore>);
    unclaim
        .execute(claimer_id.clone(), msg.id.parse().unwrap())
        .await
        .unwrap();

    let sibling_bodies = inbox_for(&s, &sibling_id, &[]).await;
    assert!(sibling_bodies.contains(&"claim me".to_string()));
}

#[tokio::test]
async fn alias_uniqueness_enforced_per_org_and_project() {
    let s = state();
    let _id1 = register_with_app(&s, "unique-alias").await;

    let register = RegisterAgent::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        tasks(&s) as Arc<dyn TaskStore>,
    );
    let result = register
        .execute(RegisterAgentCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            alias: "unique-alias".into(),
            roles: vec![],
            description: String::new(),
            agent_type: None,
            metadata: Default::default(),
            auth_user_id: None,
        })
        .await;

    assert!(
        result.is_ok(),
        "re-registering same alias should resume, not conflict: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().agent.alias, "unique-alias");
}

// ─── knowledge operations ──────────────────────────────────────────────────

#[tokio::test]
async fn knowledge_append_creates_or_extends() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    let resp = write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "append-test".into(),
            kind: "note".into(),
            title: "Append Test".into(),
            content: "original".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();
    assert_eq!(resp.version, 1);

    let append = AppendKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let appended = append
        .execute(AppendKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "append-test".into(),
            kind: "note".into(),
            value: "appended".into(),
            separator: None,
            metadata: None,
            metadata_remove: None,
        })
        .await
        .unwrap();

    assert!(appended.content.contains("original"));
    assert!(appended.content.contains("appended"));
    assert_eq!(appended.version, 2);
}

#[tokio::test]
async fn knowledge_rename_changes_path() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "old-path".into(),
            kind: "note".into(),
            title: "Rename Test".into(),
            content: "body".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let rename = RenameKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>);
    let renamed = rename
        .execute(RenameKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "old-path".into(),
            new_path: "new-path".into(),
        })
        .await
        .unwrap();
    assert_eq!(renamed.path, "new-path");

    let read = ReadKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let found = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "new-path".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(found.knowledge.is_some());

    let not_found = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "old-path".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(not_found.knowledge.is_none());
}

#[tokio::test]
async fn knowledge_move_changes_namespace() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "move-test".into(),
            kind: "note".into(),
            title: "Move Test".into(),
            content: "body".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let mv = MoveKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>);
    let moved = mv
        .execute(MoveKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "move-test".into(),
            new_namespace: "/backend".into(),
        })
        .await
        .unwrap();
    assert_eq!(moved.namespace, "/backend");

    let read = ReadKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let found = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: Some("/backend".into()),
            path: "move-test".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(found.knowledge.is_some());
}

#[tokio::test]
async fn knowledge_tag_and_untag() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "tag-test".into(),
            kind: "note".into(),
            title: "Tag Test".into(),
            content: "body".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let tag = TagKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>);
    tag.execute(TagKnowledgeCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        path: "tag-test".into(),
        tag: "rust".into(),
    })
    .await
    .unwrap();

    tag.execute(TagKnowledgeCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        path: "tag-test".into(),
        tag: "backend".into(),
    })
    .await
    .unwrap();

    let untag = UntagKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>);
    untag
        .execute(UntagKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "tag-test".into(),
            tag: "backend".into(),
        })
        .await
        .unwrap();

    let read = ReadKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let resp = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "tag-test".into(),
            relations: None,
        })
        .await
        .unwrap();
    let entry = resp.knowledge.unwrap();
    assert!(entry.tags.contains(&"rust".to_string()));
    assert!(!entry.tags.contains(&"backend".to_string()));
}

#[tokio::test]
async fn knowledge_change_kind() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    let original = write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "kind-test".into(),
            kind: "note".into(),
            title: "Kind Test".into(),
            content: "body".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();
    assert_eq!(original.kind, "note");

    let change = ChangeKnowledgeKind::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let changed = change
        .execute(ChangeKnowledgeKindCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "kind-test".into(),
            new_kind: "decision".into(),
            version: None,
        })
        .await
        .unwrap();
    assert_eq!(changed.kind, "decision");
    assert!(changed.version > original.version);
}

#[tokio::test]
async fn knowledge_promote_to_skill() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "promote-source".into(),
            kind: "decision".into(),
            title: "Use RS256".into(),
            content: "RS256 supports key rotation".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let promote = PromoteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
    );
    let skill = promote
        .execute(PromoteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            source_path: "promote-source".into(),
            target_path: "promote-skill".into(),
            target_title: Some("JWT Algorithm Skill".into()),
            instruction: None,
        })
        .await
        .unwrap();
    assert_eq!(skill.kind, "skill");
    assert_eq!(skill.path, "promote-skill");

    let read = ReadKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let skill_entry = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "promote-skill".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(skill_entry.knowledge.is_some());

    let source_entry = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "promote-source".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(source_entry.knowledge.unwrap().archived);
}

#[tokio::test]
async fn knowledge_consolidate_merges_entries() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    let src1 = write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "consolidate-a".into(),
            kind: "note".into(),
            title: "Note A".into(),
            content: "content-a".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();
    let src2 = write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "consolidate-b".into(),
            kind: "note".into(),
            title: "Note B".into(),
            content: "content-b".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: None,
        })
        .await
        .unwrap();

    let consolidate = ConsolidateKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
    );
    let merged = consolidate
        .execute(ConsolidateKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            source_paths: vec!["consolidate-a".into(), "consolidate-b".into()],
            target_path: "consolidated".into(),
            target_title: "Consolidated Notes".into(),
            target_kind: Some("summary".into()),
        })
        .await
        .unwrap();
    assert!(merged.content.contains("content-a"));
    assert!(merged.content.contains("content-b"));
    assert_eq!(merged.kind, "summary");

    let read = ReadKnowledge::new(k.clone() as Arc<dyn KnowledgeStore>, None);
    let src_a = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "consolidate-a".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(src_a.knowledge.unwrap().archived);

    let src_b = read
        .execute(ReadKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "consolidate-b".into(),
            relations: None,
        })
        .await
        .unwrap();
    assert!(src_b.knowledge.unwrap().archived);

    let org_id = org();
    let edge1 = e
        .exists_by_pair(
            &org_id,
            &ResourceKind::Knowledge,
            &merged.id,
            &ResourceKind::Knowledge,
            &src1.id,
            &RelationType::MergedFrom,
        )
        .await
        .unwrap();
    assert!(edge1, "merged_from edge to source A must exist");

    let edge2 = e
        .exists_by_pair(
            &org_id,
            &ResourceKind::Knowledge,
            &merged.id,
            &ResourceKind::Knowledge,
            &src2.id,
            &RelationType::MergedFrom,
        )
        .await
        .unwrap();
    assert!(edge2, "merged_from edge to source B must exist");
}

// ─── agent change roles ─────────────────────────────────────────────────────

#[tokio::test]
async fn agent_change_roles_updates_roles() {
    let s = state();
    let a = agents(&s);
    let agent_id = register(&a, "role-agent").await;

    let change_roles = ChangeRoles::new(a.clone() as Arc<dyn AgentStore>);
    change_roles
        .execute(ChangeRolesCommand {
            agent_id: agent_id.to_string(),
            roles: vec!["coder".to_string()],
        })
        .await
        .unwrap();

    let agent = a.find_by_id(&agent_id).await.unwrap().unwrap();
    assert_eq!(agent.roles(), &["coder"]);

    change_roles
        .execute(ChangeRolesCommand {
            agent_id: agent_id.to_string(),
            roles: vec!["coder".to_string(), "reviewer".to_string()],
        })
        .await
        .unwrap();

    let agent = a.find_by_id(&agent_id).await.unwrap().unwrap();
    assert_eq!(agent.roles(), &["coder", "reviewer"]);
}

// ─── agent switch context ───────────────────────────────────────────────────

#[tokio::test]
async fn agent_switch_context_changes_project_and_namespace() {
    let s = state();
    let agent_id = register_with_app(&s, "switch-agent").await;

    let p = projects(&s);
    let target_org = org();
    let target_project = ProjectId::try_from("proj-b").unwrap();
    let mut project = orchy_core::project::Project::new(
        target_org.clone(),
        target_project.clone(),
        String::new(),
    )
    .unwrap();
    p.save(&mut project).await.unwrap();

    let switch = SwitchContext::new(
        agents(&s) as Arc<dyn AgentStore>,
        p as Arc<dyn ProjectStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        locks(&s) as Arc<dyn LockStore>,
    );
    switch
        .execute(SwitchContextCommand {
            org_id: "default".into(),
            agent_id: agent_id.to_string(),
            project: Some("proj-b".into()),
            namespace: Some("/backend".into()),
        })
        .await
        .unwrap();

    let a = agents(&s);
    let agent = a.find_by_id(&agent_id).await.unwrap().unwrap();
    assert_eq!(agent.project().to_string(), "proj-b");
    assert_eq!(agent.namespace().to_string(), "/backend");
}

// ─── task lifecycle ─────────────────────────────────────────────────────────

#[tokio::test]
async fn task_fail_records_reason() {
    let s = state();
    let agent_id = register_with_app(&s, "fail-agent").await;

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "will fail".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: None,
        })
        .await
        .unwrap();

    let start = StartTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
    );
    start
        .execute(StartTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
        })
        .await
        .unwrap();

    let fail = FailTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let failed = fail
        .execute(FailTaskCommand {
            task_id: task.id.clone(),
            org_id: "default".into(),
            reason: Some("compilation error".into()),
        })
        .await
        .unwrap();

    assert_eq!(failed.status, "failed");
    assert_eq!(failed.result_summary.as_deref(), Some("compilation error"));
}

#[tokio::test]
async fn task_cancel_from_pending() {
    let s = state();

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "will cancel".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let cancel = CancelTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let cancelled = cancel
        .execute(CancelTaskCommand {
            task_id: task.id.clone(),
            org_id: "default".into(),
            reason: Some("no longer needed".into()),
        })
        .await
        .unwrap();

    assert_eq!(cancelled.status, "cancelled");
}

#[tokio::test]
async fn task_release_returns_to_pending() {
    let s = state();
    let agent_id = register_with_app(&s, "release-agent").await;

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "will release".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: None,
        })
        .await
        .unwrap();

    let release = ReleaseTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let released = release
        .execute(ReleaseTaskCommand {
            task_id: task.id.clone(),
        })
        .await
        .unwrap();

    assert_eq!(released.status, "pending");
    assert!(released.assigned_to.is_none());
}

#[tokio::test]
async fn task_delegate_creates_subtask_without_blocking_parent() {
    let s = state();
    let agent_id = register_with_app(&s, "delegate-agent").await;

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let parent = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "parent task".into(),
            description: "parent desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: parent.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: Some(true),
        })
        .await
        .unwrap();

    let delegate = DelegateTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let subtask = delegate
        .execute(DelegateTaskCommand {
            task_id: parent.id.clone(),
            title: "subtask".into(),
            description: "subtask desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: Some(agent_id.to_string()),
        })
        .await
        .unwrap();

    assert_eq!(subtask.status, "pending");

    let get = GetTask::new(tasks(&s) as Arc<dyn TaskStore>, None);
    let parent_now = get
        .execute(GetTaskCommand {
            task_id: parent.id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .unwrap();
    assert_eq!(parent_now.status, "in_progress");

    let e = edges(&s);
    let spawns = e
        .exists_by_pair(
            &org(),
            &ResourceKind::Task,
            &parent.id,
            &ResourceKind::Task,
            &subtask.id,
            &RelationType::Spawns,
        )
        .await
        .unwrap();
    assert!(spawns, "spawns edge from parent to subtask must exist");
}

#[tokio::test]
async fn task_merge_combines_pending_tasks() {
    let s = state();

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let t1 = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "merge source 1".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();
    let t2 = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "merge source 2".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let merge = MergeTasks::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let (merged, cancelled) = merge
        .execute(MergeTasksCommand {
            org_id: "default".into(),
            task_ids: vec![t1.id.clone(), t2.id.clone()],
            title: "merged task".into(),
            description: "combined".into(),
            acceptance_criteria: None,
            created_by: None,
        })
        .await
        .unwrap();

    assert_eq!(merged.status, "pending");
    assert_eq!(cancelled.len(), 2);
    for c in &cancelled {
        assert_eq!(c.status, "cancelled");
    }

    let e = edges(&s);
    let edge1 = e
        .exists_by_pair(
            &org(),
            &ResourceKind::Task,
            &merged.id,
            &ResourceKind::Task,
            &t1.id,
            &RelationType::MergedFrom,
        )
        .await
        .unwrap();
    assert!(edge1, "merged_from edge to source 1 must exist");

    let edge2 = e
        .exists_by_pair(
            &org(),
            &ResourceKind::Task,
            &merged.id,
            &ResourceKind::Task,
            &t2.id,
            &RelationType::MergedFrom,
        )
        .await
        .unwrap();
    assert!(edge2, "merged_from edge to source 2 must exist");
}

#[tokio::test]
async fn task_replace_cancels_original_creates_replacements() {
    let s = state();

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let original = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "original task".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let replace = ReplaceTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let (cancelled_original, replacements) = replace
        .execute(ReplaceTaskCommand {
            task_id: original.id.clone(),
            reason: Some("split into smaller tasks".into()),
            replacements: vec![
                SubtaskInput {
                    title: "replacement 1".into(),
                    description: "desc 1".into(),
                    acceptance_criteria: None,
                    priority: None,
                    assigned_roles: None,
                    depends_on: None,
                },
                SubtaskInput {
                    title: "replacement 2".into(),
                    description: "desc 2".into(),
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

    assert_eq!(cancelled_original.status, "cancelled");
    assert_eq!(replacements.len(), 2);

    let e = edges(&s);
    for r in &replacements {
        assert_eq!(r.status, "pending");
        let supersedes = e
            .exists_by_pair(
                &org(),
                &ResourceKind::Task,
                &r.id,
                &ResourceKind::Task,
                &original.id,
                &RelationType::Supersedes,
            )
            .await
            .unwrap();
        assert!(supersedes, "supersedes edge must exist for {}", r.id);
    }
}

#[tokio::test]
async fn task_tag_and_untag() {
    let s = state();

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "tag test".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let tag = TagTask::new(tasks(&s) as Arc<dyn TaskStore>);
    tag.execute(TagTaskCommand {
        task_id: task.id.clone(),
        tag: "rust".into(),
    })
    .await
    .unwrap();

    tag.execute(TagTaskCommand {
        task_id: task.id.clone(),
        tag: "backend".into(),
    })
    .await
    .unwrap();

    let untag = UntagTask::new(tasks(&s) as Arc<dyn TaskStore>);
    untag
        .execute(UntagTaskCommand {
            task_id: task.id.clone(),
            tag: "backend".into(),
        })
        .await
        .unwrap();

    let get = GetTask::new(tasks(&s) as Arc<dyn TaskStore>, None);
    let result = get
        .execute(GetTaskCommand {
            task_id: task.id.clone(),
            org_id: None,
            relations: None,
        })
        .await
        .unwrap();

    assert!(result.tags.contains(&"rust".to_string()));
    assert!(!result.tags.contains(&"backend".to_string()));
}

#[tokio::test]
async fn task_assign_transfers_to_another_agent() {
    let s = state();
    let agent1 = register_with_app(&s, "assign-agent-1").await;
    let agent2 = register_with_app(&s, "assign-agent-2").await;

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "assign test".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent1.to_string(),
            org_id: "default".into(),
            start: None,
        })
        .await
        .unwrap();

    let assign = AssignTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
    );
    let assigned = assign
        .execute(AssignTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent2.to_string(),
        })
        .await
        .unwrap();

    assert_eq!(assigned.assigned_to.as_deref(), Some(&*agent2.to_string()));
}

#[tokio::test]
async fn task_add_and_remove_dependency() {
    let s = state();

    let post = PostTask::new(tasks(&s) as Arc<dyn TaskStore>);
    let task_a = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "task A (dependent)".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();
    let task_b = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "task B (dependency)".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let add_dep = AddDependency::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let after_dep = add_dep
        .execute(AddDependencyCommand {
            org_id: "default".into(),
            task_id: task_a.id.clone(),
            dependency_id: task_b.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(after_dep.status, "blocked");

    let agent_id = register_with_app(&s, "dep-completer").await;
    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task_b.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: Some(true),
        })
        .await
        .unwrap();

    let complete = CompleteTask::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    complete
        .execute(CompleteTaskCommand {
            task_id: task_b.id.clone(),
            org_id: "default".into(),
            summary: Some("done".into()),
            links: vec![],
        })
        .await
        .unwrap();

    let remove_dep = RemoveDependency::new(
        tasks(&s) as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    let after_remove = remove_dep
        .execute(RemoveDependencyCommand {
            org_id: "default".into(),
            task_id: task_a.id.clone(),
            dependency_id: task_b.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(after_remove.status, "pending");
}

// ─── knowledge write with task_id creates produces edge ─────────────────────

#[tokio::test]
async fn knowledge_write_with_task_id_creates_produces_edge() {
    let s = state();
    let agent_id = register_with_app(&s, "know-task-agent").await;
    let k = knowledge(&s);
    let e = edges(&s);
    let t = tasks(&s);

    let post = PostTask::new(t.clone() as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "auth task".into(),
            description: "implement auth".into(),
            acceptance_criteria: Some("tests pass".into()),
            priority: None,
            assigned_roles: None,
            created_by: Some(agent_id.to_string()),
        })
        .await
        .unwrap();

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    let knowledge_entry = write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "auth/decision".into(),
            kind: "decision".into(),
            title: "Use RS256".into(),
            content: "RS256 for key rotation".into(),
            tags: None,
            version: None,
            agent_id: Some(agent_id.to_string()),
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: Some(task.id.clone()),
        })
        .await
        .unwrap();

    let has_edge = e
        .exists_by_pair(
            &org(),
            &ResourceKind::Task,
            &task.id,
            &ResourceKind::Knowledge,
            &knowledge_entry.id,
            &RelationType::Produces,
        )
        .await
        .unwrap();
    assert!(
        has_edge,
        "write_knowledge with task_id must create produces edge"
    );
}

// ─── message refs preserved ─────────────────────────────────────────────────

#[tokio::test]
async fn message_refs_preserved_in_delivery() {
    let s = state();
    let sender_id = register_with_app(&s, "ref-sender").await;
    let receiver_id = register_with_app(&s, "ref-receiver").await;

    let send = SendMessage::new(
        agents(&s) as Arc<dyn AgentStore>,
        messages(&s) as Arc<dyn MessageStore>,
        users(&s) as Arc<dyn UserStore>,
        memberships(&s) as Arc<dyn OrgMembershipStore>,
    );
    let msg = send
        .execute(SendMessageCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            from_agent_id: sender_id.to_string(),
            to: receiver_id.to_string(),
            body: "check these refs".into(),
            reply_to: None,
            refs: vec![
                ResourceRef::task("task-123").with_display("Auth task"),
                ResourceRef::knowledge("auth/jwt").with_display("JWT decision"),
            ],
        })
        .await
        .unwrap();

    assert_eq!(msg.refs.len(), 2);

    let check = CheckMailbox::new(
        messages(&s) as Arc<dyn MessageStore>,
        agents(&s) as Arc<dyn AgentStore>,
    );
    let inbox = check
        .execute(CheckMailboxCommand {
            agent_id: receiver_id.to_string(),
            org_id: "default".into(),
            project: "test".into(),
            after: None,
            limit: None,
        })
        .await
        .unwrap();

    let received = inbox
        .items
        .iter()
        .find(|m| m.body == "check these refs")
        .unwrap();
    assert_eq!(received.refs.len(), 2);
}

// ─── task touch keeps alive ─────────────────────────────────────────────────

#[tokio::test]
async fn task_touch_updates_timestamp() {
    let s = state();
    let agent_id = register_with_app(&s, "touch-agent").await;
    let t = tasks(&s);

    let post = PostTask::new(t.clone() as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "long task".into(),
            description: "takes a while".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        t.clone() as Arc<dyn TaskStore>,
        edges(&s) as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: None,
        })
        .await
        .unwrap();

    let touch = TouchTask::new(t.clone() as Arc<dyn TaskStore>);
    let touched = touch
        .execute(TouchTaskCommand {
            task_id: task.id.clone(),
            agent_id: Some(agent_id.to_string()),
        })
        .await
        .unwrap();

    assert_eq!(touched.status, "claimed");
    assert!(touched.updated_at >= task.updated_at);
}

// ─── task complete with summary ─────────────────────────────────────────────

#[tokio::test]
async fn task_complete_preserves_summary() {
    let s = state();
    let agent_id = register_with_app(&s, "complete-agent").await;
    let t = tasks(&s);
    let e = edges(&s);

    let post = PostTask::new(t.clone() as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "completable".into(),
            description: "will be completed".into(),
            acceptance_criteria: Some("must have summary".into()),
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();
    assert_eq!(
        task.acceptance_criteria.as_deref(),
        Some("must have summary")
    );

    let claim = ClaimTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        t.clone() as Arc<dyn TaskStore>,
        e.clone() as Arc<dyn EdgeStore>,
    );
    claim
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
            org_id: "default".into(),
            start: None,
        })
        .await
        .unwrap();

    let start = StartTask::new(
        agents(&s) as Arc<dyn AgentStore>,
        t.clone() as Arc<dyn TaskStore>,
    );
    start
        .execute(StartTaskCommand {
            task_id: task.id.clone(),
            agent_id: agent_id.to_string(),
        })
        .await
        .unwrap();

    let complete = CompleteTask::new(t.clone() as Arc<dyn TaskStore>, e as Arc<dyn EdgeStore>);
    let done = complete
        .execute(CompleteTaskCommand {
            task_id: task.id.clone(),
            org_id: "default".into(),
            summary: Some("implemented with RS256, all tests green".into()),
            links: vec![],
        })
        .await
        .unwrap();

    assert_eq!(done.status, "completed");
    assert_eq!(
        done.result_summary.as_deref(),
        Some("implemented with RS256, all tests green")
    );
}

// ─── resolve_agent by alias and id ──────────────────────────────────────────

#[tokio::test]
async fn resolve_agent_finds_by_alias_and_id() {
    let s = state();
    let agent_id = register_with_app(&s, "resolve-me").await;

    let resolve = ResolveAgent::new(agents(&s) as Arc<dyn AgentStore>);

    let by_alias = resolve
        .execute(ResolveAgentCommand {
            org_id: "default".into(),
            project: "test".into(),
            id_or_alias: "resolve-me".into(),
        })
        .await
        .unwrap();
    assert_eq!(by_alias.id, agent_id.to_string());
    assert_eq!(by_alias.alias, "resolve-me");

    let by_id = resolve
        .execute(ResolveAgentCommand {
            org_id: "default".into(),
            project: "test".into(),
            id_or_alias: agent_id.to_string(),
        })
        .await
        .unwrap();
    assert_eq!(by_id.id, agent_id.to_string());
}

// ─── list_edges returns edges ───────────────────────────────────────────────

#[tokio::test]
async fn list_edges_returns_created_edges() {
    let s = state();
    let e = edges(&s);

    let add = AddEdge::new(e.clone() as Arc<dyn EdgeStore>);
    add.execute(AddEdgeCommand {
        org_id: "default".into(),
        from_kind: "knowledge".into(),
        from_id: "list-edge-a".into(),
        to_kind: "knowledge".into(),
        to_id: "list-edge-b".into(),
        rel_type: "related_to".into(),
        created_by: None,
        if_not_exists: true,
    })
    .await
    .unwrap();

    let list = ListEdges::new(e as Arc<dyn EdgeStore>);
    let page = list
        .execute(ListEdgesCommand {
            org_id: "default".into(),
            rel_type: Some("related_to".into()),
            after: None,
            limit: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert!(!page.items.is_empty());
    let found = page
        .items
        .iter()
        .any(|edge| edge.from_id == "list-edge-a" && edge.to_id == "list-edge-b");
    assert!(found, "created edge must appear in list_edges");
}

// ─── query_relations traverses graph ────────────────────────────────────────

#[tokio::test]
async fn query_relations_traverses_task_knowledge_graph() {
    let s = state();
    let k = knowledge(&s);
    let e = edges(&s);
    let t = tasks(&s);
    let a = agents(&s);
    let m = messages(&s);

    let post = PostTask::new(t.clone() as Arc<dyn TaskStore>);
    let task = post
        .execute(PostTaskCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            title: "graph-task".into(),
            description: "for graph test".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let write = WriteKnowledge::new(
        k.clone() as Arc<dyn KnowledgeStore>,
        e.clone() as Arc<dyn EdgeStore>,
        None,
    );
    write
        .execute(WriteKnowledgeCommand {
            org_id: "default".into(),
            project: "test".into(),
            namespace: None,
            path: "graph/decision".into(),
            kind: "decision".into(),
            title: "Graph Decision".into(),
            content: "decided something".into(),
            tags: None,
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: Some(task.id.clone()),
        })
        .await
        .unwrap();

    let materializer = MaterializeNeighborhood::new(
        e as Arc<dyn EdgeStore>,
        t as Arc<dyn TaskStore>,
        k as Arc<dyn KnowledgeStore>,
        a as Arc<dyn AgentStore>,
        m as Arc<dyn MessageStore>,
    );
    let neighborhood = materializer
        .execute(MaterializeNeighborhoodCommand {
            org_id: "default".into(),
            anchor_kind: "task".into(),
            anchor_id: task.id.clone(),
            options: RelationOptions {
                rel_types: None,
                target_kinds: vec![],
                direction: Default::default(),
                max_depth: 1,
                limit: 50,
            },
            as_of: None,
            project: Some("test".into()),
            namespace: None,
            semantic_query: None,
        })
        .await
        .unwrap();

    assert_eq!(neighborhood.anchor.kind().to_string(), "task");
    assert!(
        !neighborhood.relations.is_empty(),
        "task with linked knowledge must have relations"
    );
    let has_produces = neighborhood
        .relations
        .iter()
        .any(|r| r.rel_type == RelationType::Produces);
    assert!(has_produces, "must find produces relation to knowledge");
}
