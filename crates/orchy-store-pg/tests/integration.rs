use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentStatus, AgentStore};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_pg::PgBackend;

const PG_URL: &str = "postgres://orchy:orchy@localhost:5432/orchy";

async fn backend() -> PgBackend {
    let b = PgBackend::new(PG_URL, None).await.unwrap();
    b.truncate_all().await.unwrap();
    b
}

fn ns(s: &str) -> Namespace {
    Namespace::try_from(s).unwrap()
}

fn proj(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

#[tokio::test]
#[ignore]
async fn agent_save_and_find() {
    let store = backend().await;
    let mut agent = Agent::register(
        proj("myapp"),
        Namespace::root(),
        vec!["coder".into()],
        "test agent".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut agent).await.unwrap();

    assert_eq!(agent.status(), AgentStatus::Online);
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = AgentStore::find_by_id(&store, &agent.id())
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
        proj("test-project"),
        Namespace::root(),
        vec!["dev".into()],
        "original".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut agent).await.unwrap();

    let before = agent.last_heartbeat();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let updated = AgentStore::find_by_id(&store, &agent.id())
        .await
        .unwrap()
        .unwrap();
    assert!(updated.last_heartbeat() > before);
}

#[tokio::test]
#[ignore]
async fn agent_disconnect_sets_status() {
    let store = backend().await;
    let mut agent = Agent::register(
        proj("test-project"),
        Namespace::root(),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut agent).await.unwrap();

    agent.disconnect();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let fetched = AgentStore::find_by_id(&store, &agent.id())
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
        proj("test-project"),
        Namespace::root(),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    agent.disconnect();
    AgentStore::save(&store, &mut agent).await.unwrap();
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(!timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
#[ignore]
async fn task_save_and_get() {
    let store = backend().await;

    let mut task = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "Do thing".into(),
        "Details".into(),
        Priority::High,
        vec!["dev".into()],
        vec![],
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
        proj("proj"),
        Namespace::root(),
        None,
        "low".into(),
        "".into(),
        Priority::Low,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut low).await.unwrap();

    let mut critical = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "critical".into(),
        "".into(),
        Priority::Critical,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut critical).await.unwrap();

    let tasks = TaskStore::list(&store, TaskFilter::default())
        .await
        .unwrap();
    assert_eq!(tasks[0].title(), "critical");
    assert_eq!(tasks[1].title(), "low");
}

#[tokio::test]
#[ignore]
async fn message_save_and_find_pending() {
    let store = backend().await;

    let from_agent = Agent::register(
        proj("test-project"),
        Namespace::root(),
        vec![],
        "sender".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut from_agent).await.unwrap();

    let to_agent = Agent::register(
        proj("test-project"),
        Namespace::root(),
        vec![],
        "receiver".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut to_agent).await.unwrap();

    let mut msg = Message::new(
        proj("test-project"),
        Namespace::root(),
        from_agent.id(),
        MessageTarget::Agent(to_agent.id()),
        "hello".into(),
        None,
    );
    MessageStore::save(&store, &mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let p = proj("test-project");
    let messages = MessageStore::find_pending(&store, &to_agent.id(), &p, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body(), "hello");
    assert_eq!(messages[0].status(), MessageStatus::Pending);

    let mut delivered = messages.into_iter().next().unwrap();
    delivered.deliver();
    MessageStore::save(&store, &mut delivered).await.unwrap();

    let messages = MessageStore::find_pending(&store, &to_agent.id(), &p, &Namespace::root())
        .await
        .unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
#[ignore]
async fn message_find_by_id_and_mark_read() {
    let store = backend().await;

    let from_agent = Agent::register(
        proj("test-project"),
        Namespace::root(),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut from_agent).await.unwrap();

    let to_agent = Agent::register(
        proj("test-project"),
        Namespace::root(),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &mut to_agent).await.unwrap();

    let mut msg = Message::new(
        proj("test-project"),
        Namespace::root(),
        from_agent.id(),
        MessageTarget::Agent(to_agent.id()),
        "hi".into(),
        None,
    );
    MessageStore::save(&store, &mut msg).await.unwrap();

    let mut fetched = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    fetched.mark_read();
    MessageStore::save(&store, &mut fetched).await.unwrap();

    let read = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}
