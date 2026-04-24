use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentStore, Alias};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_pg::*;

const PG_URL: &str = "postgres://orchy:orchy@localhost:5432/orchy";

async fn pool() -> sqlx::PgPool {
    let b = PgDatabase::new(PG_URL, None).await.unwrap();
    b.run_migrations(&PgDatabase::migrations_dir())
        .await
        .unwrap();
    b.truncate_all().await.unwrap();
    b.pool()
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
    let p = pool().await;
    let agents = PgAgentStore::new(p);
    let mut agent = Agent::register(
        org(),
        proj("myapp"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec!["coder".into()],
        "test agent".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut agent).await.unwrap();

    assert_eq!(agent.derived_status(30, 300), "active");
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = agents.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
#[ignore]
async fn agent_save_updates_existing() {
    let p = pool().await;
    let agents = PgAgentStore::new(p);
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec!["dev".into()],
        "original".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut agent).await.unwrap();

    let before = agent.last_seen();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    agents.save(&mut agent).await.unwrap();

    let updated = agents.find_by_id(agent.id()).await.unwrap().unwrap();
    assert!(updated.last_seen() > before);
}

#[tokio::test]
#[ignore]
async fn agent_save_and_fetch_roundtrip() {
    let p = pool().await;
    let agents = PgAgentStore::new(p);
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut agent).await.unwrap();
    agents.save(&mut agent).await.unwrap();
    let _fetched = agents.find_by_id(agent.id()).await.unwrap().unwrap();
}

#[tokio::test]
#[ignore]
async fn agent_find_timed_out() {
    let p = pool().await;
    let agents = PgAgentStore::new(p);
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("test-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = agents.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    agents.save(&mut agent).await.unwrap();
    let timed_out = agents.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
#[ignore]
async fn task_save_and_get() {
    let p = pool().await;
    let tasks = PgTaskStore::new(p);

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
    tasks.save(&mut task).await.unwrap();

    let fetched = tasks.find_by_id(&task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
}

#[tokio::test]
#[ignore]
async fn task_save_persists_event_log() {
    let p = pool().await;
    let tasks = PgTaskStore::new(p.clone());
    let event_query = PgEventQuery::new(p);
    let organization = org();
    let mut task = Task::new(
        organization.clone(),
        proj("proj"),
        Namespace::root(),
        "Write event".into(),
        "verify tx writer".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    tasks.save(&mut task).await.unwrap();

    let events = event_query
        .query_events(
            organization.as_str(),
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            10,
        )
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic, "task.created");
}

#[tokio::test]
#[ignore]
async fn task_list_sorted_by_priority() {
    let p = pool().await;
    let tasks = PgTaskStore::new(p);

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
    tasks.save(&mut low).await.unwrap();

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
    tasks.save(&mut critical).await.unwrap();

    let page = tasks
        .list(TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
#[ignore]
async fn message_save_and_find_unread() {
    let p = pool().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("sender-agent").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("receiver-agent").unwrap(),
        vec![],
        "receiver".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut to_agent).await.unwrap();

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
    messages.save(&mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let pr = proj("test-project");
    let unread = messages
        .find_unread(
            to_agent.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(unread.items.len(), 1);
    assert_eq!(unread.items[0].body(), "hello");
    assert_eq!(unread.items[0].status(), MessageStatus::Pending);

    let msg_id = unread.items[0].id();
    messages.mark_read(to_agent.id(), &[msg_id]).await.unwrap();

    let after = messages
        .find_unread(
            to_agent.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(after.items.is_empty());
}

#[tokio::test]
#[ignore]
async fn message_find_by_id_and_mark_read() {
    let p = pool().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("from-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("to-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut to_agent).await.unwrap();

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
    messages.save(&mut msg).await.unwrap();

    let mut fetched = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    fetched.mark_read().unwrap();
    messages.save(&mut fetched).await.unwrap();

    let read = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
#[ignore]
async fn message_find_by_id_preserves_claim_state() {
    let p = pool().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut sender_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("claim-sender").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut sender_agent).await.unwrap();

    let mut claimer_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("claim-claimer").unwrap(),
        vec![],
        "claimer".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut claimer_agent).await.unwrap();
    let claimer = claimer_agent.id().clone();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        sender_agent.id().clone(),
        MessageTarget::Broadcast,
        "claimable".into(),
        None,
        vec![],
    )
    .unwrap();
    msg.claim(claimer.clone()).unwrap();
    messages.save(&mut msg).await.unwrap();

    let fetched = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(fetched.claimed_by(), Some(&claimer));

    let mut fetched = fetched;
    fetched.unclaim(&claimer).unwrap();
    messages.save(&mut fetched).await.unwrap();

    let unclaimed = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert!(unclaimed.claimed_by().is_none());
}

#[tokio::test]
#[ignore]
async fn message_find_unread_includes_broadcast_until_agent_reads_it() {
    let p = pool().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);
    let pr = proj("proj");

    let mut sender_agent = Agent::register(
        org(),
        pr.clone(),
        Namespace::root(),
        Alias::new("bcast-sender").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut sender_agent).await.unwrap();
    let sender = sender_agent.id().clone();

    let mut receiver_agent = Agent::register(
        org(),
        pr.clone(),
        Namespace::root(),
        Alias::new("bcast-receiver").unwrap(),
        vec![],
        "receiver".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut receiver_agent).await.unwrap();
    let receiver = receiver_agent.id().clone();

    let mut msg = Message::new(
        org(),
        pr.clone(),
        Namespace::root(),
        sender.clone(),
        MessageTarget::Broadcast,
        "to all".into(),
        None,
        vec![],
    )
    .unwrap();
    messages.save(&mut msg).await.unwrap();

    let pending = messages
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 1);

    let sender_pending = messages
        .find_unread(
            &sender,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(sender_pending.items.is_empty());

    messages.mark_read(&receiver, &[msg.id()]).await.unwrap();

    let after_read = messages
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(after_read.items.is_empty());
}
