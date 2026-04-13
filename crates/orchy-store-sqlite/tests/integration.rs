use std::collections::HashMap;

use orchy_core::agent::{AgentId, AgentStatus, AgentStore, RegisterAgent};
use orchy_core::memory::{
    ContextStore, CreateSnapshot, MemoryFilter, MemoryStore, Version, WriteMemory,
};
use orchy_core::message::{CreateMessage, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::Namespace;
use orchy_core::skill::{SkillFilter, SkillStore, WriteSkill};
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_sqlite::SqliteBackend;

fn backend() -> SqliteBackend {
    SqliteBackend::new(":memory:", None).unwrap()
}

fn ns(s: &str) -> Namespace {
    Namespace::try_from(s).unwrap()
}

#[tokio::test]
async fn agent_register_and_get() {
    let store = backend();
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: ns("myapp"),
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
async fn agent_heartbeat_updates_timestamp() {
    let store = backend();
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: ns("test-project"),
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
async fn agent_disconnect_sets_status() {
    let store = backend();
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: ns("test-project"),
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
async fn agent_find_timed_out() {
    let store = backend();
    let agent = AgentStore::register(
        &store,
        RegisterAgent {
            namespace: ns("test-project"),
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

#[tokio::test]
async fn task_save_and_get() {
    let store = backend();
    let task = Task::new(
        ns("proj"),
        "Do thing".into(),
        "Details".into(),
        Priority::High,
        vec!["dev".into()],
        vec![],
        None,
        false,
    );

    TaskStore::save(&store, &task).await.unwrap();

    let fetched = TaskStore::get(&store, &task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
}

#[tokio::test]
async fn task_save_overwrites_existing() {
    let store = backend();
    let task = Task::new(
        ns("proj"),
        "original".into(),
        "desc".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    );

    TaskStore::save(&store, &task).await.unwrap();

    let updated = Task::restore(
        task.id(),
        ns("proj"),
        "updated".into(),
        "new desc".into(),
        TaskStatus::Completed,
        Priority::High,
        vec![],
        None,
        None,
        vec![],
        Some("done".into()),
        vec![],
        None,
        task.created_at(),
        task.updated_at(),
    );
    TaskStore::save(&store, &updated).await.unwrap();

    let fetched = TaskStore::get(&store, &task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.title(), "updated");
    assert_eq!(fetched.status(), TaskStatus::Completed);
    assert_eq!(fetched.result_summary(), Some("done"));
}

#[tokio::test]
async fn task_dependency_stored() {
    let store = backend();

    let dep = Task::new(
        ns("proj"),
        "dep".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    );
    TaskStore::save(&store, &dep).await.unwrap();

    let task = Task::new(
        ns("proj"),
        "main".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![dep.id()],
        None,
        true,
    );
    TaskStore::save(&store, &task).await.unwrap();

    let fetched = TaskStore::get(&store, &task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Blocked);
    assert_eq!(fetched.depends_on(), &[dep.id()]);
}

#[tokio::test]
async fn task_list_sorted_by_priority() {
    let store = backend();

    let low = Task::new(
        ns("proj"),
        "low".into(),
        "".into(),
        Priority::Low,
        vec![],
        vec![],
        None,
        false,
    );
    TaskStore::save(&store, &low).await.unwrap();

    let critical = Task::new(
        ns("proj"),
        "critical".into(),
        "".into(),
        Priority::Critical,
        vec![],
        vec![],
        None,
        false,
    );
    TaskStore::save(&store, &critical).await.unwrap();

    let tasks = TaskStore::list(&store, TaskFilter::default())
        .await
        .unwrap();
    assert_eq!(tasks[0].title(), "critical");
    assert_eq!(tasks[1].title(), "low");
}

#[tokio::test]
async fn memory_write_and_read() {
    let store = backend();

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
async fn memory_version_check_success() {
    let store = backend();

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
async fn memory_version_check_failure() {
    let store = backend();

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
async fn memory_list_with_namespace_prefix() {
    let store = backend();

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
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 2);

    let tasks_only = MemoryStore::list(
        &store,
        MemoryFilter {
            namespace: Some(ns("app/tasks")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tasks_only.len(), 1);
    assert_eq!(tasks_only[0].key, "a");
}

#[tokio::test]
async fn memory_search_by_keyword() {
    let store = backend();

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
async fn memory_delete() {
    let store = backend();

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

#[tokio::test]
async fn message_send_and_check() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let project_ns = ns("test-project");

    let msg = MessageStore::send(
        &store,
        CreateMessage {
            namespace: project_ns.clone(),
            from,
            to: MessageTarget::Agent(to),
            body: "hello".into(),
            reply_to: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(msg.status, MessageStatus::Pending);

    let messages = MessageStore::check(&store, &to, &project_ns).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "hello");
    assert_eq!(messages[0].status, MessageStatus::Delivered);

    // Second check returns nothing
    let messages = MessageStore::check(&store, &to, &project_ns).await.unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
async fn message_mark_read() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let project_ns = ns("test-project");

    let msg = MessageStore::send(
        &store,
        CreateMessage {
            namespace: project_ns.clone(),
            from,
            to: MessageTarget::Agent(to),
            body: "hi".into(),
            reply_to: None,
        },
    )
    .await
    .unwrap();

    MessageStore::check(&store, &to, &project_ns).await.unwrap();
    MessageStore::mark_read(&store, &[msg.id]).await.unwrap();
}

#[tokio::test]
async fn context_save_and_load() {
    let store = backend();
    let agent = AgentId::new();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent,
            namespace: ns("proj"),
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
            agent_id: agent,
            namespace: ns("proj"),
            summary: "second snapshot".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let loaded = ContextStore::load(&store, &agent).await.unwrap().unwrap();
    assert_eq!(loaded.summary, "second snapshot");
}

#[tokio::test]
async fn context_list_filters() {
    let store = backend();
    let agent1 = AgentId::new();
    let agent2 = AgentId::new();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent1,
            namespace: ns("proj"),
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
            agent_id: agent2,
            namespace: ns("other"),
            summary: "a2".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let all = ContextStore::list(&store, None, &ns("proj")).await.unwrap();
    assert_eq!(all.len(), 1);

    let by_agent = ContextStore::list(&store, Some(&agent1), &ns("proj"))
        .await
        .unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].summary, "a1");

    let by_ns = ContextStore::list(&store, None, &ns("other"))
        .await
        .unwrap();
    assert_eq!(by_ns.len(), 1);
}

#[tokio::test]
async fn context_search_by_keyword() {
    let store = backend();
    let agent = AgentId::new();

    ContextStore::save(
        &store,
        CreateSnapshot {
            agent_id: agent,
            namespace: ns("test-project"),
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
            agent_id: agent,
            namespace: ns("test-project"),
            summary: "fixing database migrations".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata: HashMap::new(),
        },
    )
    .await
    .unwrap();

    let results = ContextStore::search(
        &store,
        "authentication",
        None,
        &ns("test-project"),
        None,
        10,
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].summary.contains("authentication"));
}

#[tokio::test]
async fn skill_write_and_read() {
    let store = backend();
    let project_ns = ns("test-project");

    let skill = SkillStore::write(
        &store,
        WriteSkill {
            namespace: project_ns.clone(),
            name: "commit-conventions".to_string(),
            description: "How to write commit messages".to_string(),
            content: "Use conventional commits".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(skill.name, "commit-conventions");
    assert_eq!(skill.namespace, project_ns);

    let read = SkillStore::read(&store, &project_ns, "commit-conventions")
        .await
        .unwrap();
    assert!(read.is_some());
    assert_eq!(read.unwrap().content, "Use conventional commits");

    let missing = SkillStore::read(&store, &project_ns, "nonexistent")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn skill_write_updates_existing() {
    let store = backend();
    let project_ns = ns("test-project");

    SkillStore::write(
        &store,
        WriteSkill {
            namespace: project_ns.clone(),
            name: "style".to_string(),
            description: "v1".to_string(),
            content: "old content".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    let updated = SkillStore::write(
        &store,
        WriteSkill {
            namespace: project_ns.clone(),
            name: "style".to_string(),
            description: "v2".to_string(),
            content: "new content".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(updated.content, "new content");
    assert_eq!(updated.description, "v2");
}

#[tokio::test]
async fn skill_list_filters_by_namespace() {
    let store = backend();

    SkillStore::write(
        &store,
        WriteSkill {
            namespace: ns("proj-a"),
            name: "style".to_string(),
            description: "A style".to_string(),
            content: "A content".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    SkillStore::write(
        &store,
        WriteSkill {
            namespace: ns("proj-a/backend"),
            name: "arch".to_string(),
            description: "Backend arch".to_string(),
            content: "Hexagonal".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    SkillStore::write(
        &store,
        WriteSkill {
            namespace: ns("proj-b"),
            name: "style".to_string(),
            description: "B style".to_string(),
            content: "B content".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    let all_a = SkillStore::list(
        &store,
        SkillFilter {
            namespace: Some(ns("proj-a")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(all_a.len(), 2);

    let only_b = SkillStore::list(
        &store,
        SkillFilter {
            namespace: Some(ns("proj-b")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(only_b.len(), 1);
    assert_eq!(only_b[0].name, "style");
}

#[tokio::test]
async fn skill_delete() {
    let store = backend();
    let project_ns = ns("test-project");

    SkillStore::write(
        &store,
        WriteSkill {
            namespace: project_ns.clone(),
            name: "temp".to_string(),
            description: "temporary".to_string(),
            content: "will be deleted".to_string(),
            written_by: None,
        },
    )
    .await
    .unwrap();

    SkillStore::delete(&store, &project_ns, "temp")
        .await
        .unwrap();

    let read = SkillStore::read(&store, &project_ns, "temp").await.unwrap();
    assert!(read.is_none());
}
