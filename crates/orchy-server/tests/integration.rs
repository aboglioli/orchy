//! Integration tests — regression tests for bugs fixed this session.
//!
//! 1. send_message resolves alias from_agent_id (not just UUID)
//! 2. lock_resource resolves alias holder_agent_id (not just UUID)
//! 3. edge add (if_not_exists) returns real ID instead of empty string

use std::sync::Arc;

use orchy_application::{AddEdge, AddEdgeCommand, LockResource, LockResourceCommand,
    SendMessage, SendMessageCommand};
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::graph::EdgeStore;
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;
use orchy_store_memory::MemoryBackend;

fn org() -> OrganizationId { OrganizationId::new("default").unwrap() }
fn proj() -> ProjectId { ProjectId::try_from("test").unwrap() }
fn ns() -> Namespace { Namespace::root() }

async fn setup() -> Arc<MemoryBackend> {
    Arc::new(MemoryBackend::new())
}

async fn register(b: &Arc<MemoryBackend>, alias: &str) -> AgentId {
    let alias = Alias::new(alias).unwrap();
    let mut a = Agent::register(org(), proj(), ns(), alias, vec![], String::new(), None, Default::default()).unwrap();
    let id = a.id().clone();
    AgentStore::save(b.as_ref(), &mut a).await.unwrap();
    id
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
    );

    // from_agent_id = alias "sender"
    let msg = send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: "sender".into(),
        to: receiver_id.to_string(),
        body: "hello".into(),
        reply_to: None,
        refs: vec![],
    }).await.expect("send with alias should succeed");
    assert_eq!(msg.from, sender_id.to_string());

    // from_agent_id = UUID
    let msg2 = send.execute(SendMessageCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        from_agent_id: sender_id.to_string(),
        to: format!("@receiver"),
        body: "hello2".into(),
        reply_to: None,
        refs: vec![],
    }).await.expect("send with UUID should succeed");
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
    let r = lock.execute(LockResourceCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        name: "test-lock-1".into(),
        holder_agent_id: "locker".into(),
        ttl_secs: Some(300),
    }).await.expect("lock with alias should succeed");
    assert_eq!(r.holder, agent_id.to_string());

    // holder_agent_id = UUID
    let r2 = lock.execute(LockResourceCommand {
        org_id: "default".into(),
        project: "test".into(),
        namespace: None,
        name: "test-lock-2".into(),
        holder_agent_id: agent_id.to_string(),
        ttl_secs: Some(300),
    }).await.expect("lock with UUID should succeed");
    assert_eq!(r2.holder, agent_id.to_string());
}

// ─── edge idempotent add returns real ID ────────────────────────────────────

#[tokio::test]
async fn edge_add_idempotent_returns_real_id() {
    let b = setup().await;
    let add = AddEdge::new(b.clone() as Arc<dyn EdgeStore>);

    let r1 = add.execute(AddEdgeCommand {
        org_id: "default".into(),
        from_kind: "knowledge".into(),
        from_id: "src".into(),
        to_kind: "knowledge".into(),
        to_id: "dst".into(),
        rel_type: "related_to".into(),
        created_by: None,
        if_not_exists: true,
    }).await.expect("first add should succeed");
    assert!(!r1.id.is_empty(), "edge ID must not be empty on first add");

    let r2 = add.execute(AddEdgeCommand {
        org_id: "default".into(),
        from_kind: "knowledge".into(),
        from_id: "src".into(),
        to_kind: "knowledge".into(),
        to_id: "dst".into(),
        rel_type: "related_to".into(),
        created_by: None,
        if_not_exists: true,
    }).await.expect("idempotent add should succeed");
    assert!(!r2.id.is_empty(), "idempotent add must return real ID, not empty string");
    assert_eq!(r2.id, r1.id, "idempotent add should return same edge ID");
}