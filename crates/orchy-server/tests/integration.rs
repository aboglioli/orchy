//! Integration tests — regression tests for bugs fixed this session.
//!
//! 1. send_message resolves alias from_agent_id (not just UUID)
//! 2. lock_resource resolves alias holder_agent_id (not just UUID)
//! 3. edge add (if_not_exists) returns real ID instead of empty string

use std::str::FromStr;
use std::sync::Arc;

use orchy_application::{
    AddEdge, AddEdgeCommand, CheckMailbox, CheckMailboxCommand, LockResource, LockResourceCommand,
    MarkRead, MarkReadCommand, RegisterAgent, RegisterAgentCommand, RenameAlias,
    RenameAliasCommand, SendMessage, SendMessageCommand,
};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::graph::EdgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::TaskStore;
use orchy_core::user::OrgMembershipStore;
use orchy_core::user::UserStore;
use orchy_store_memory::MemoryBackend;

fn org() -> OrganizationId {
    OrganizationId::new("default").unwrap()
}
fn proj() -> ProjectId {
    ProjectId::try_from("test").unwrap()
}
fn ns() -> Namespace {
    Namespace::root()
}

async fn setup() -> Arc<MemoryBackend> {
    Arc::new(MemoryBackend::new())
}

async fn register(b: &Arc<MemoryBackend>, alias: &str) -> AgentId {
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
    AgentStore::save(b.as_ref(), &mut a).await.unwrap();
    id
}

async fn register_with_app(b: &Arc<MemoryBackend>, alias: &str) -> AgentId {
    let register = RegisterAgent::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn TaskStore>,
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

async fn inbox_for(b: &Arc<MemoryBackend>, id: &AgentId, _roles: &[String]) -> Vec<String> {
    let check = CheckMailbox::new(
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn AgentStore>,
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
    let b = setup().await;
    let sender_id = register(&b, "sender").await;
    let receiver_id = register(&b, "receiver").await;

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
    );

    // from_agent_id = alias "sender"
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

    // from_agent_id = UUID
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
    let b = setup().await;
    let agent_id = register(&b, "locker").await;

    let lock = LockResource::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn LockStore>,
    );

    // holder_agent_id = alias
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

    // holder_agent_id = UUID
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
    let b = setup().await;
    let add = AddEdge::new(b.clone() as Arc<dyn EdgeStore>);

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
    let b = setup().await;
    let id1 = register_with_app(&b, "reconnect-agent").await;

    // Send a direct message to the alias
    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // Re-register with same alias
    let id2 = register_with_app(&b, "reconnect-agent").await;
    assert_eq!(id1, id2, "same alias must resume same agent UUID");

    // Message still visible in mailbox
    let bodies = inbox_for(&b, &id1, &[]).await;
    assert!(bodies.contains(&"direct-before-reconnect".to_string()));
}

// ─── alias rename preserves direct messages ───────────────────────────────

#[tokio::test]
async fn alias_rename_preserves_direct_messages() {
    let b = setup().await;
    let agent_id = register_with_app(&b, "original-alias").await;
    let sender_id = register_with_app(&b, "sender").await;

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // Rename the agent
    let rename = RenameAlias::new(b.clone() as Arc<dyn AgentStore>);
    rename
        .execute(RenameAliasCommand {
            agent_id: agent_id.to_string(),
            new_alias: "renamed-alias".into(),
        })
        .await
        .unwrap();

    // Message still in mailbox via UUID target
    let bodies = inbox_for(&b, &agent_id, &[]).await;
    assert!(bodies.contains(&"to-original".to_string()));
}

// ─── namespace mark-read ──────────────────────────────────────────────────

#[tokio::test]
async fn namespace_targeted_message_can_be_marked_read() {
    let b = setup().await;
    let sender_id = register_with_app(&b, "ns-sender").await;
    let recipient_id = register_with_app(&b, "ns-recipient").await;

    // Give recipient a namespace
    {
        use orchy_core::agent::AgentStore;
        let mut agent = AgentStore::find_by_id(b.as_ref(), &recipient_id)
            .await
            .unwrap()
            .unwrap();
        use orchy_core::namespace::Namespace;
        agent
            .switch_context(None, Namespace::try_from("/backend".to_string()).unwrap())
            .unwrap();
        AgentStore::save(b.as_ref(), &mut agent).await.unwrap();
    }

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // Mark read should succeed for namespace-targeted messages
    let mark_read = MarkRead::new(
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn AgentStore>,
    );
    mark_read
        .execute(MarkReadCommand {
            agent_id: recipient_id.to_string(),
            message_ids: vec![msg.id],
        })
        .await
        .expect("mark_read should succeed for namespace-targeted message");

    // Should not appear in inbox anymore
    let bodies = inbox_for(&b, &recipient_id, &[]).await;
    assert!(!bodies.contains(&"ns-message".to_string()));
}

// ─── future logical audience visibility ─────────────────────────────────────

#[tokio::test]
async fn future_agent_sees_old_role_message() {
    let b = setup().await;
    let sender_id = register_with_app(&b, "role-sender").await;

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // New agent with matching role created later
    let recipient_id = register_with_app(&b, "future-reviewer").await;
    {
        use orchy_core::agent::AgentStore;
        let mut agent = AgentStore::find_by_id(b.as_ref(), &recipient_id)
            .await
            .unwrap()
            .unwrap();
        agent.change_roles(vec!["reviewer".to_string()]).unwrap();
        AgentStore::save(b.as_ref(), &mut agent).await.unwrap();
    }

    let bodies = inbox_for(&b, &recipient_id, &["reviewer".to_string()]).await;
    assert!(bodies.contains(&"role-message".to_string()));
}

#[tokio::test]
async fn future_agent_sees_old_broadcast_message() {
    let b = setup().await;
    let sender_id = register_with_app(&b, "bc-sender").await;

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // New agent created later
    let recipient_id = register_with_app(&b, "future-listener").await;

    let bodies = inbox_for(&b, &recipient_id, &[]).await;
    assert!(bodies.contains(&"broadcast-message".to_string()));
}

#[tokio::test]
async fn future_agent_sees_old_namespace_message() {
    let b = setup().await;
    let sender_id = register_with_app(&b, "ns-sender2").await;

    let send = SendMessage::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn UserStore>,
        b.clone() as Arc<dyn OrgMembershipStore>,
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

    // New agent in matching namespace created later
    let recipient_id = register_with_app(&b, "future-backend-dev").await;
    {
        use orchy_core::agent::AgentStore;
        let mut agent = AgentStore::find_by_id(b.as_ref(), &recipient_id)
            .await
            .unwrap()
            .unwrap();
        use orchy_core::namespace::Namespace;
        agent
            .switch_context(None, Namespace::try_from("/backend".to_string()).unwrap())
            .unwrap();
        AgentStore::save(b.as_ref(), &mut agent).await.unwrap();
    }

    let bodies = inbox_for(&b, &recipient_id, &[]).await;
    assert!(bodies.contains(&"ns-old-message".to_string()));
}

// ─── alias uniqueness per org/project ───────────────────────────────────────

#[tokio::test]
async fn alias_uniqueness_enforced_per_org_and_project() {
    let b = setup().await;
    let _id1 = register_with_app(&b, "unique-alias").await;

    // Second agent with same alias in same org/project should conflict
    let register = RegisterAgent::new(
        b.clone() as Arc<dyn AgentStore>,
        b.clone() as Arc<dyn MessageStore>,
        b.clone() as Arc<dyn TaskStore>,
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

    // Should resume existing agent, not create a new one
    assert!(
        result.is_ok(),
        "re-registering same alias should resume, not conflict: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap().agent.alias, "unique-alias");
}
