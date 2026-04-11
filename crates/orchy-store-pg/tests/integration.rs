use std::collections::HashMap;

use orchy_core::entities::*;
use orchy_core::store::{AgentStore, ContextStore, MemoryStore, MessageStore, TaskStore};
use orchy_core::value_objects::*;
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

// === Agent lifecycle ===

#[tokio::test]
#[ignore]
async fn agent_register_and_get() {
    let store = backend().await;
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: Some(ns("myapp")),
            roles: vec!["coder".into()],
            description: "test agent".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    assert_eq!(agent.status, AgentStatus::Online);
    assert_eq!(agent.roles, vec!["coder".to_string()]);

    let fetched = AgentStore::get(&store, &agent.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, agent.id);
}

#[tokio::test]
#[ignore]
async fn agent_heartbeat_updates_timestamp() {
    let store = backend().await;
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let before = agent.last_heartbeat;
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    AgentStore::heartbeat(&store, &agent.id).await.unwrap();

    let updated = AgentStore::get(&store, &agent.id).await.unwrap().unwrap();
    assert!(updated.last_heartbeat > before);
}

#[tokio::test]
#[ignore]
async fn agent_disconnect_sets_status() {
    let store = backend().await;
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    AgentStore::disconnect(&store, &agent.id).await.unwrap();
    let fetched = AgentStore::get(&store, &agent.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, AgentStatus::Disconnected);
}

#[tokio::test]
#[ignore]
async fn agent_find_timed_out() {
    let store = backend().await;
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id == agent.id));

    AgentStore::disconnect(&store, &agent.id).await.unwrap();
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(!timed_out.iter().any(|a| a.id == agent.id));
}

// === Task lifecycle ===

#[tokio::test]
#[ignore]
async fn task_create_and_claim() {
    let store = backend().await;

    // Register agent first (FK constraint)
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: Some(ns("proj")),
            roles: vec!["dev".into()],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let task = TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "Do thing".into(),
            description: "Details".into(),
            priority: Priority::High,
            assigned_roles: vec!["dev".into()],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(task.status, TaskStatus::Pending);

    let claimed = TaskStore::claim(&store, &task.id, &agent.id).await.unwrap();
    assert_eq!(claimed.status, TaskStatus::Claimed);
    assert_eq!(claimed.claimed_by, Some(agent.id));
}

#[tokio::test]
#[ignore]
async fn task_claim_fails_when_not_pending() {
    let store = backend().await;

    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let task = TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "t".into(),
            description: "".into(),
            priority: Priority::Normal,
            assigned_roles: vec![],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    TaskStore::claim(&store, &task.id, &agent.id).await.unwrap();

    let agent2 = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let result = TaskStore::claim(&store, &task.id, &agent2.id).await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn task_complete() {
    let store = backend().await;

    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let task = TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "t".into(),
            description: "".into(),
            priority: Priority::Normal,
            assigned_roles: vec![],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    TaskStore::claim(&store, &task.id, &agent.id).await.unwrap();
    TaskStore::update_status(&store, &task.id, TaskStatus::InProgress)
        .await
        .unwrap();

    let completed = TaskStore::complete(&store, &task.id, Some("done".into()))
        .await
        .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(completed.result_summary, Some("done".into()));
}

#[tokio::test]
#[ignore]
async fn task_dependency_blocking() {
    let store = backend().await;

    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let dep = TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "dep".into(),
            description: "".into(),
            priority: Priority::Normal,
            assigned_roles: vec![],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    let task = TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "main".into(),
            description: "".into(),
            priority: Priority::Normal,
            assigned_roles: vec![],
            depends_on: vec![dep.id],
            created_by: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(task.status, TaskStatus::Blocked);

    TaskStore::claim(&store, &dep.id, &agent.id).await.unwrap();
    TaskStore::update_status(&store, &dep.id, TaskStatus::InProgress)
        .await
        .unwrap();
    TaskStore::complete(&store, &dep.id, None).await.unwrap();

    TaskStore::update_status(&store, &task.id, TaskStatus::Pending)
        .await
        .unwrap();
    let fetched = TaskStore::get(&store, &task.id).await.unwrap().unwrap();
    assert_eq!(fetched.status, TaskStatus::Pending);
}

#[tokio::test]
#[ignore]
async fn task_list_sorted_by_priority() {
    let store = backend().await;

    TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "low".into(),
            description: "".into(),
            priority: Priority::Low,
            assigned_roles: vec![],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    TaskStore::create(
        &store,
        CreateTask {
            namespace: ns("proj"),
            title: "critical".into(),
            description: "".into(),
            priority: Priority::Critical,
            assigned_roles: vec![],
            depends_on: vec![],
            created_by: None,
        },
    )
    .await
    .unwrap();

    let tasks = TaskStore::list(&store, TaskFilter::default()).await.unwrap();
    assert_eq!(tasks[0].title, "critical");
    assert_eq!(tasks[1].title, "low");
}

// === Memory lifecycle ===

#[tokio::test]
#[ignore]
async fn memory_write_and_read() {
    let store = backend().await;

    let entry = MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "config".into(),
            value: "hello world".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(entry.version, Version::initial());

    let read = MemoryStore::read(&store, &ns("app"), "config")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value, "hello world");
}

#[tokio::test]
#[ignore]
async fn memory_version_check_success() {
    let store = backend().await;

    let entry = MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "k".into(),
            value: "v1".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    let updated = MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "k".into(),
            value: "v2".into(),
            expected_version: Some(entry.version),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.version, Version::initial().next());
    assert_eq!(updated.value, "v2");
}

#[tokio::test]
#[ignore]
async fn memory_version_check_failure() {
    let store = backend().await;

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "k".into(),
            value: "v1".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    let result = MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "k".into(),
            value: "v2".into(),
            expected_version: Some(Version::from(99)),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await;

    assert!(result.is_err());
}

#[tokio::test]
#[ignore]
async fn memory_list_with_namespace_prefix() {
    let store = backend().await;

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app/tasks"),
            key: "a".into(),
            value: "x".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app/other"),
            key: "b".into(),
            value: "y".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    let all = MemoryStore::list(
        &store,
        MemoryFilter {
            namespace: Some(ns("app")),
        },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 2);

    let tasks_only = MemoryStore::list(
        &store,
        MemoryFilter {
            namespace: Some(ns("app/tasks")),
        },
    )
    .await
    .unwrap();
    assert_eq!(tasks_only.len(), 1);
    assert_eq!(tasks_only[0].key, "a");
}

#[tokio::test]
#[ignore]
async fn memory_search_by_keyword() {
    let store = backend().await;

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "notes".into(),
            value: "the quick brown fox".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "other".into(),
            value: "lazy dog".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    let results = MemoryStore::search(&store, "quick", None, None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key, "notes");
}

#[tokio::test]
#[ignore]
async fn memory_delete() {
    let store = backend().await;

    MemoryStore::write(
        &store,
        WriteMemory {
            namespace: ns("app"),
            key: "k".into(),
            value: "v".into(),
            expected_version: None,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by: None,
        },
    )
    .await
    .unwrap();

    MemoryStore::delete(&store, &ns("app"), "k").await.unwrap();
    let result = MemoryStore::read(&store, &ns("app"), "k").await.unwrap();
    assert!(result.is_none());
}

// === Message lifecycle ===

#[tokio::test]
#[ignore]
async fn message_send_and_check() {
    let store = backend().await;

    // Register agents (FK constraint)
    let from_agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "sender".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let to_agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "receiver".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let msg = MessageStore::send(
        &store,
        CreateMessage {
            namespace: None,
            from: from_agent.id,
            to: MessageTarget::Agent(to_agent.id),
            body: "hello".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(msg.status, MessageStatus::Pending);

    let messages = MessageStore::check(&store, &to_agent.id, None).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "hello");
    assert_eq!(messages[0].status, MessageStatus::Delivered);

    // Second check returns nothing
    let messages = MessageStore::check(&store, &to_agent.id, None).await.unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
#[ignore]
async fn message_mark_read() {
    let store = backend().await;

    let from_agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let to_agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let msg = MessageStore::send(
        &store,
        CreateMessage {
            namespace: None,
            from: from_agent.id,
            to: MessageTarget::Agent(to_agent.id),
            body: "hi".into(),
        },
    )
    .await
    .unwrap();

    MessageStore::check(&store, &to_agent.id, None).await.unwrap();
    MessageStore::mark_read(&store, &[msg.id]).await.unwrap();
}

// === Context lifecycle ===

#[tokio::test]
#[ignore]
async fn context_save_and_load() {
    let store = backend().await;

    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: Some(ns("proj")),
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent.id,
            namespace: Some(ns("proj")),
            summary: "first snapshot".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent.id,
            namespace: Some(ns("proj")),
            summary: "second snapshot".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let loaded = ContextStore::load(&store, &agent.id).await.unwrap().unwrap();
    assert_eq!(loaded.summary, "second snapshot");
}

#[tokio::test]
#[ignore]
async fn context_list_filters() {
    let store = backend().await;

    let agent1 = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: Some(ns("proj")),
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let agent2 = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: Some(ns("other")),
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent1.id,
            namespace: Some(ns("proj")),
            summary: "a1".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent2.id,
            namespace: Some(ns("other")),
            summary: "a2".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let all = ContextStore::list(&store, None, None).await.unwrap();
    assert_eq!(all.len(), 2);

    let by_agent = ContextStore::list(&store, Some(&agent1.id), None)
        .await
        .unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].summary, "a1");

    let by_ns = ContextStore::list(&store, None, Some(&ns("proj")))
        .await
        .unwrap();
    assert_eq!(by_ns.len(), 1);
}

#[tokio::test]
#[ignore]
async fn context_search_by_keyword() {
    let store = backend().await;

    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: None,
            roles: vec![],
            description: "".into(),
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent.id,
            namespace: None,
            summary: "working on authentication module".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent.id,
            namespace: None,
            summary: "fixing database migrations".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let results = ContextStore::search(&store, "authentication", None, None, None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].summary.contains("authentication"));
}
