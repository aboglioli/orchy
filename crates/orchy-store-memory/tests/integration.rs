use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::memory::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{Skill, SkillFilter, SkillStore};
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_memory::MemoryBackend;

fn backend() -> MemoryBackend {
    MemoryBackend::new()
}

fn ns(s: &str) -> Namespace {
    Namespace::try_from(s).unwrap()
}

fn project(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

#[tokio::test]
async fn agent_save_and_find() {
    let store = backend();
    let agent = Agent::register(
        project("myapp"),
        ns("myapp"),
        vec!["coder".into()],
        "test agent".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &agent).await.unwrap();

    assert_eq!(agent.status(), AgentStatus::Online);
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = AgentStore::find_by_id(&store, &agent.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
async fn agent_save_updates_existing() {
    let store = backend();
    let mut agent = Agent::register(
        project("test-project"),
        ns("test-project"),
        vec!["dev".into()],
        "original".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &agent).await.unwrap();

    let before = agent.last_heartbeat();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat();
    AgentStore::save(&store, &agent).await.unwrap();

    let updated = AgentStore::find_by_id(&store, &agent.id())
        .await
        .unwrap()
        .unwrap();
    assert!(updated.last_heartbeat() > before);
}

#[tokio::test]
async fn agent_disconnect_sets_status() {
    let store = backend();
    let mut agent = Agent::register(
        project("test-project"),
        ns("test-project"),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &agent).await.unwrap();

    agent.disconnect();
    AgentStore::save(&store, &agent).await.unwrap();

    let fetched = AgentStore::find_by_id(&store, &agent.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status(), AgentStatus::Disconnected);
}

#[tokio::test]
async fn agent_find_timed_out() {
    let store = backend();
    let mut agent = Agent::register(
        project("test-project"),
        ns("test-project"),
        vec![],
        "".into(),
        HashMap::new(),
    );
    AgentStore::save(&store, &agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    agent.disconnect();
    AgentStore::save(&store, &agent).await.unwrap();
    let timed_out = AgentStore::find_timed_out(&store, 0).await.unwrap();
    assert!(!timed_out.iter().any(|a| a.id() == agent.id()));
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

    let fetched = TaskStore::find_by_id(&store, &task.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
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
async fn memory_save_and_find_by_key() {
    let store = backend();

    let entry = MemoryEntry::new(ns("app"), "config".into(), "hello world".into(), None);
    MemoryStore::save(&store, &entry).await.unwrap();

    let read = MemoryStore::find_by_key(&store, &ns("app"), "config")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value(), "hello world");
}

#[tokio::test]
async fn memory_save_updates_existing() {
    let store = backend();

    let mut entry = MemoryEntry::new(ns("app"), "k".into(), "v1".into(), None);
    MemoryStore::save(&store, &entry).await.unwrap();

    entry.update("v2".into(), None);
    MemoryStore::save(&store, &entry).await.unwrap();

    let read = MemoryStore::find_by_key(&store, &ns("app"), "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value(), "v2");
}

#[tokio::test]
async fn memory_list_with_namespace_prefix() {
    let store = backend();

    let entry_a = MemoryEntry::new(ns("app/tasks"), "a".into(), "x".into(), None);
    MemoryStore::save(&store, &entry_a).await.unwrap();

    let entry_b = MemoryEntry::new(ns("app/other"), "b".into(), "y".into(), None);
    MemoryStore::save(&store, &entry_b).await.unwrap();

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
    assert_eq!(tasks_only[0].key(), "a");
}

#[tokio::test]
async fn memory_search_by_substring() {
    let store = backend();

    let entry1 = MemoryEntry::new(
        ns("app"),
        "notes".into(),
        "the quick brown fox".into(),
        None,
    );
    MemoryStore::save(&store, &entry1).await.unwrap();

    let entry2 = MemoryEntry::new(ns("app"), "other".into(), "lazy dog".into(), None);
    MemoryStore::save(&store, &entry2).await.unwrap();

    let results = MemoryStore::search(&store, "quick", None, None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key(), "notes");
}

#[tokio::test]
async fn memory_delete() {
    let store = backend();

    let entry = MemoryEntry::new(ns("app"), "k".into(), "v".into(), None);
    MemoryStore::save(&store, &entry).await.unwrap();

    MemoryStore::delete(&store, &ns("app"), "k").await.unwrap();
    let result = MemoryStore::find_by_key(&store, &ns("app"), "k")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn message_save_and_find_pending() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let project_ns = ns("test-project");

    let msg = Message::new(
        project_ns.clone(),
        from,
        MessageTarget::Agent(to),
        "hello".into(),
        None,
    );
    MessageStore::save(&store, &msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let messages = MessageStore::find_pending(&store, &to, &project_ns)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body(), "hello");
    assert_eq!(messages[0].status(), MessageStatus::Pending);

    let mut delivered = messages.into_iter().next().unwrap();
    delivered.deliver();
    MessageStore::save(&store, &delivered).await.unwrap();

    let messages = MessageStore::find_pending(&store, &to, &project_ns)
        .await
        .unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
async fn message_find_by_id_and_mark_read() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let project_ns = ns("test-project");

    let msg = Message::new(
        project_ns.clone(),
        from,
        MessageTarget::Agent(to),
        "hi".into(),
        None,
    );
    MessageStore::save(&store, &msg).await.unwrap();

    let mut fetched = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    fetched.mark_read();
    MessageStore::save(&store, &fetched).await.unwrap();

    let read = MessageStore::find_by_id(&store, &msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
async fn context_save_and_find_latest() {
    let store = backend();
    let agent = AgentId::new();

    let snap1 = ContextSnapshot::new(agent, ns("proj"), "first snapshot".into(), HashMap::new());
    ContextStore::save(&store, &snap1).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let snap2 = ContextSnapshot::new(agent, ns("proj"), "second snapshot".into(), HashMap::new());
    ContextStore::save(&store, &snap2).await.unwrap();

    let loaded = ContextStore::find_latest(&store, &agent)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.summary(), "second snapshot");
}

#[tokio::test]
async fn context_list_filters() {
    let store = backend();
    let agent1 = AgentId::new();
    let agent2 = AgentId::new();

    let snap1 = ContextSnapshot::new(agent1, ns("proj"), "a1".into(), HashMap::new());
    ContextStore::save(&store, &snap1).await.unwrap();

    let snap2 = ContextSnapshot::new(agent2, ns("other"), "a2".into(), HashMap::new());
    ContextStore::save(&store, &snap2).await.unwrap();

    let proj_ns = ns("proj");

    let by_agent = ContextStore::list(&store, Some(&agent1), &proj_ns)
        .await
        .unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].summary(), "a1");

    let by_ns = ContextStore::list(&store, None, &proj_ns).await.unwrap();
    assert_eq!(by_ns.len(), 1);
}

#[tokio::test]
async fn context_search_by_substring() {
    let store = backend();
    let agent = AgentId::new();
    let project_ns = ns("test-project");

    let snap1 = ContextSnapshot::new(
        agent,
        project_ns.clone(),
        "working on authentication module".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap1).await.unwrap();

    let snap2 = ContextSnapshot::new(
        agent,
        project_ns.clone(),
        "fixing database migrations".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap2).await.unwrap();

    let results = ContextStore::search(&store, "auth", None, &project_ns, None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].summary().contains("authentication"));
}

#[tokio::test]
async fn skill_save_and_find_by_name() {
    let store = backend();
    let project_ns = ns("test-project");

    let skill = Skill::new(
        project_ns.clone(),
        "commit-conventions".to_string(),
        "How to write commit messages".to_string(),
        "Use conventional commits".to_string(),
        None,
    );
    SkillStore::save(&store, &skill).await.unwrap();

    let read = SkillStore::find_by_name(&store, &project_ns, "commit-conventions")
        .await
        .unwrap();
    assert!(read.is_some());
    assert_eq!(read.unwrap().content(), "Use conventional commits");

    let missing = SkillStore::find_by_name(&store, &project_ns, "nonexistent")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn skill_save_updates_existing() {
    let store = backend();
    let project_ns = ns("test-project");

    let skill = Skill::new(
        project_ns.clone(),
        "style".to_string(),
        "v1".to_string(),
        "old content".to_string(),
        None,
    );
    SkillStore::save(&store, &skill).await.unwrap();

    let updated = Skill::new(
        project_ns.clone(),
        "style".to_string(),
        "v2".to_string(),
        "new content".to_string(),
        None,
    );
    SkillStore::save(&store, &updated).await.unwrap();

    let read = SkillStore::find_by_name(&store, &project_ns, "style")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.content(), "new content");
    assert_eq!(read.description(), "v2");
}

#[tokio::test]
async fn skill_list_filters_by_namespace() {
    let store = backend();

    let s1 = Skill::new(
        ns("proj-a"),
        "style".to_string(),
        "A style".to_string(),
        "A content".to_string(),
        None,
    );
    SkillStore::save(&store, &s1).await.unwrap();

    let s2 = Skill::new(
        ns("proj-a/backend"),
        "arch".to_string(),
        "Backend arch".to_string(),
        "Hexagonal".to_string(),
        None,
    );
    SkillStore::save(&store, &s2).await.unwrap();

    let s3 = Skill::new(
        ns("proj-b"),
        "style".to_string(),
        "B style".to_string(),
        "B content".to_string(),
        None,
    );
    SkillStore::save(&store, &s3).await.unwrap();

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
    assert_eq!(only_b[0].name(), "style");
}

#[tokio::test]
async fn skill_delete() {
    let store = backend();
    let project_ns = ns("test-project");

    let skill = Skill::new(
        project_ns.clone(),
        "temp".to_string(),
        "temporary".to_string(),
        "will be deleted".to_string(),
        None,
    );
    SkillStore::save(&store, &skill).await.unwrap();

    SkillStore::delete(&store, &project_ns, "temp")
        .await
        .unwrap();

    let read = SkillStore::find_by_name(&store, &project_ns, "temp")
        .await
        .unwrap();
    assert!(read.is_none());
}
