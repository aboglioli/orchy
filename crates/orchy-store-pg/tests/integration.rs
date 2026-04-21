use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, Alias};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_pg::PgBackend;

const PG_URL: &str = "postgres://orchy:orchy@localhost:5432/orchy";

async fn backend() -> PgBackend {
    let b = PgBackend::new(PG_URL, None).await.unwrap();
    b.truncate_all().await.unwrap();
    b
}

fn proj(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

fn org() -> OrganizationId {
    OrganizationId::new("default").unwrap()
}

#[tokio::test]
#[ignore]
async fn agent_save_and_find() {
    let store = backend().await;
    let mut agent = Agent::register(
        org(),
        proj("myapp"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec!["coder".into()],
        "test agent".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    assert_eq!(agent.status(), AgentStatus::Online);
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = AgentStore::find_by_id(&store, agent.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
#[ignore]
async fn agent_save_updates_existing() {
    let store = backend().await;
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec!["dev".into()],
        "original".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let before = agent.last_seen();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let updated = AgentStore::find_by_id(&store, agent.id())
        .await
        .unwrap()
        .unwrap();
    assert!(updated.last_seen() > before);
}

#[tokio::test]
#[ignore]
async fn agent_disconnect_sets_status() {
    let store = backend().await;
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    agent.disconnect().unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let fetched = AgentStore::find_by_id(&store, agent.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status(), AgentStatus::Disconnected);
}

#[tokio::test]
#[ignore]
async fn agent_find_timed_out() {
    let store = backend().await;
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    agent.disconnect().unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(!timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
#[ignore]
async fn task_save_and_get() {
    let store = backend().await;

    let mut task = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "Do thing".into(),
        "Details".into(),
        None,
        Priority::High,
        vec!["dev".into()],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut task).await.unwrap();

    let fetched = TaskStore::find_by_id(&store, &task.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
}

#[tokio::test]
#[ignore]
async fn task_list_sorted_by_priority() {
    let store = backend().await;

    let mut low = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "low".into(),
        "".into(),
        None,
        Priority::Low,
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut low).await.unwrap();

    let mut critical = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "critical".into(),
        "".into(),
        None,
        Priority::Critical,
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut critical).await.unwrap();

    let page = TaskStore::list(&store, TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
#[ignore]
async fn message_save_and_find_pending() {
    let store = backend().await;

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "receiver".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut to_agent).await.unwrap();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        from_agent.id().clone(),
        MessageTarget::Agent(to_agent.id().clone()),
        "hello".into(),
        None,
        vec![],
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let p = proj("test-project");
    let messages = MessageStore::find_pending(
        &store,
        to_agent.id(),
        &[],
        &org(),
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(messages.items.len(), 1);
    assert_eq!(messages.items[0].body(), "hello");
    assert_eq!(messages.items[0].status(), MessageStatus::Pending);

    let mut delivered = messages.items.into_iter().next().unwrap();
    delivered.deliver().unwrap();
    MessageStore::save(&store, &mut delivered).await.unwrap();

    let messages = MessageStore::find_pending(
        &store,
        to_agent.id(),
        &[],
        &org(),
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(messages.items.is_empty());
}

#[tokio::test]
#[ignore]
async fn message_find_by_id_and_mark_read() {
    let store = backend().await;

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut to_agent).await.unwrap();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        from_agent.id().clone(),
        MessageTarget::Agent(to_agent.id().clone()),
        "hi".into(),
        None,
        vec![],
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();

    let mut fetched = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    fetched.mark_read().unwrap();
    MessageStore::save(&store, &mut fetched).await.unwrap();

    let read = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
#[ignore]
async fn message_find_pending_includes_broadcast_until_agent_reads_it() {
    let store = backend().await;
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let p = proj("proj");

    let mut msg = Message::new(
        org(),
        p.clone(),
        Namespace::root(),
        sender.clone(),
        MessageTarget::Broadcast,
        "to all".into(),
        None,
        vec![],
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();

    let pending = MessageStore::find_pending(
        &store,
        &receiver,
        &[],
        &org(),
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(pending.items.len(), 1);

    let sender_pending = MessageStore::find_pending(
        &store,
        &sender,
        &[],
        &org(),
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(sender_pending.items.is_empty());

    MessageStore::mark_read_for_agent(&store, &msg.id(), &receiver)
        .await
        .unwrap();

    let after_read = MessageStore::find_pending(
        &store,
        &receiver,
        &[],
        &org(),
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(after_read.items.is_empty());
}
