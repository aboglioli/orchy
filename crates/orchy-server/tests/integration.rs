use std::str::FromStr;
use std::sync::Arc;

use orchy_application::{
    AddEdge, AddEdgeCommand, CheckMailbox, CheckMailboxCommand, GetAgentSummary,
    GetAgentSummaryCommand, GetNextTask, GetNextTaskCommand, GetProject, GetProjectCommand,
    LockResource, LockResourceCommand, MarkRead, MarkReadCommand, PostTask, PostTaskCommand,
    RegisterAgent, RegisterAgentCommand, RenameAlias, RenameAliasCommand, SendMessage,
    SendMessageCommand,
};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::graph::EdgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::OrgMembershipStore;
use orchy_core::user::UserStore;
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
            auth_user_id: None,
        })
        .await
        .unwrap();
    AgentId::from_str(&resp.agent.id).unwrap()
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
