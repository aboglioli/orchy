use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::memory::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{Skill, SkillFilter, SkillStore};
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_sqlite::SqliteBackend;

fn backend() -> SqliteBackend {
    let store = SqliteBackend::new(":memory:", None).unwrap();
    store.apply_schema().unwrap();
    store
}

fn ns(s: &str) -> Namespace {
    Namespace::try_from(s).unwrap()
}

fn proj(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

#[tokio::test]
async fn agent_save_and_find() {
    let store = backend();
    let agent = Agent::register(
        proj("myapp"),
        Namespace::root(),
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
        proj("test-project"),
        Namespace::root(),
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
        proj("test-project"),
        Namespace::root(),
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
        proj("test-project"),
        Namespace::root(),
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
async fn task_save_overwrites_existing() {
    let store = backend();
    let task = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "original".into(),
        "desc".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();

    TaskStore::save(&store, &task).await.unwrap();

    let updated = Task::restore(RestoreTask {
        id: task.id(),
        project: proj("proj"),
        namespace: Namespace::root(),
        parent_id: None,
        title: "updated".into(),
        description: "new desc".into(),
        status: TaskStatus::Completed,
        priority: Priority::High,
        assigned_roles: vec![],
        assigned_to: None,
        assigned_at: None,
        depends_on: vec![],
        result_summary: Some("done".into()),
        notes: vec![],
        created_by: None,
        created_at: task.created_at(),
        updated_at: task.updated_at(),
    });
    TaskStore::save(&store, &updated).await.unwrap();

    let fetched = TaskStore::find_by_id(&store, &task.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.title(), "updated");
    assert_eq!(fetched.status(), TaskStatus::Completed);
    assert_eq!(fetched.result_summary(), Some("done"));
}

#[tokio::test]
async fn task_dependency_stored() {
    let store = backend();

    let dep = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "dep".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &dep).await.unwrap();

    let task = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "main".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![dep.id()],
        None,
        true,
    )
    .unwrap();
    TaskStore::save(&store, &task).await.unwrap();

    let fetched = TaskStore::find_by_id(&store, &task.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.status(), TaskStatus::Blocked);
    assert_eq!(fetched.depends_on(), &[dep.id()]);
}

#[tokio::test]
async fn task_list_sorted_by_priority() {
    let store = backend();

    let low = Task::new(
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
    TaskStore::save(&store, &low).await.unwrap();

    let critical = Task::new(
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

    let entry = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "config".into(),
        "hello world".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &entry).await.unwrap();

    let read = MemoryStore::find_by_key(&store, &proj("app"), &Namespace::root(), "config")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value(), "hello world");
}

#[tokio::test]
async fn memory_save_updates_existing() {
    let store = backend();

    let mut entry = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "k".into(),
        "v1".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &entry).await.unwrap();

    entry.update("v2".into(), None);
    MemoryStore::save(&store, &entry).await.unwrap();

    let read = MemoryStore::find_by_key(&store, &proj("app"), &Namespace::root(), "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value(), "v2");
}

#[tokio::test]
async fn memory_list_with_namespace_prefix() {
    let store = backend();

    let entry_a = MemoryEntry::new(proj("app"), ns("/tasks"), "a".into(), "x".into(), None).unwrap();
    MemoryStore::save(&store, &entry_a).await.unwrap();

    let entry_b = MemoryEntry::new(proj("app"), ns("/other"), "b".into(), "y".into(), None).unwrap();
    MemoryStore::save(&store, &entry_b).await.unwrap();

    let all = MemoryStore::list(
        &store,
        MemoryFilter {
            namespace: Some(Namespace::root()),
            project: Some(proj("app")),
        },
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 2);

    let tasks_only = MemoryStore::list(
        &store,
        MemoryFilter {
            namespace: Some(ns("/tasks")),
            project: Some(proj("app")),
        },
    )
    .await
    .unwrap();
    assert_eq!(tasks_only.len(), 1);
    assert_eq!(tasks_only[0].key(), "a");
}

#[tokio::test]
async fn memory_search_by_keyword() {
    let store = backend();

    let entry1 = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "notes".into(),
        "the quick brown fox".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &entry1).await.unwrap();

    let entry2 = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "other".into(),
        "lazy dog".into(),
        None,
    )
    .unwrap();
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

    let entry = MemoryEntry::new(proj("app"), Namespace::root(), "k".into(), "v".into(), None).unwrap();
    MemoryStore::save(&store, &entry).await.unwrap();

    MemoryStore::delete(&store, &proj("app"), &Namespace::root(), "k")
        .await
        .unwrap();
    let result = MemoryStore::find_by_key(&store, &proj("app"), &Namespace::root(), "k")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn message_save_and_find_pending() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let p = proj("test-project");

    let msg = Message::new(
        p.clone(),
        Namespace::root(),
        from,
        MessageTarget::Agent(to),
        "hello".into(),
        None,
    );
    MessageStore::save(&store, &msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let messages = MessageStore::find_pending(&store, &to, &p, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body(), "hello");
    assert_eq!(messages[0].status(), MessageStatus::Pending);

    let mut delivered = messages.into_iter().next().unwrap();
    delivered.deliver();
    MessageStore::save(&store, &delivered).await.unwrap();

    let messages = MessageStore::find_pending(&store, &to, &p, &Namespace::root())
        .await
        .unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
async fn message_find_by_id_and_mark_read() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let p = proj("test-project");

    let msg = Message::new(
        p.clone(),
        Namespace::root(),
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

    let snap1 = ContextSnapshot::new(
        proj("proj"),
        agent,
        Namespace::root(),
        "first snapshot".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap1).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let snap2 = ContextSnapshot::new(
        proj("proj"),
        agent,
        Namespace::root(),
        "second snapshot".into(),
        HashMap::new(),
    );
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

    let snap1 = ContextSnapshot::new(
        proj("proj"),
        agent1,
        Namespace::root(),
        "a1".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap1).await.unwrap();

    let snap2 = ContextSnapshot::new(
        proj("other"),
        agent2,
        ns("/sub"),
        "a2".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap2).await.unwrap();

    let all = ContextStore::list(&store, None, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    let by_agent = ContextStore::list(&store, Some(&agent1), &Namespace::root())
        .await
        .unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].summary(), "a1");
}

#[tokio::test]
async fn context_search_by_keyword() {
    let store = backend();
    let agent = AgentId::new();

    let snap1 = ContextSnapshot::new(
        proj("test-project"),
        agent,
        Namespace::root(),
        "working on authentication module".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap1).await.unwrap();

    let snap2 = ContextSnapshot::new(
        proj("test-project"),
        agent,
        Namespace::root(),
        "fixing database migrations".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &snap2).await.unwrap();

    let results =
        ContextStore::search(&store, "authentication", None, &Namespace::root(), None, 10)
            .await
            .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].summary().contains("authentication"));
}

#[tokio::test]
async fn skill_save_and_find_by_name() {
    let store = backend();
    let p = proj("test-project");

    let skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "commit-conventions".to_string(),
        "How to write commit messages".to_string(),
        "Use conventional commits".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &skill).await.unwrap();

    let read = SkillStore::find_by_name(&store, &p, &Namespace::root(), "commit-conventions")
        .await
        .unwrap();
    assert!(read.is_some());
    assert_eq!(read.unwrap().content(), "Use conventional commits");

    let missing = SkillStore::find_by_name(&store, &p, &Namespace::root(), "nonexistent")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn skill_save_updates_existing() {
    let store = backend();
    let p = proj("test-project");

    let skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "style".to_string(),
        "v1".to_string(),
        "old content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &skill).await.unwrap();

    let updated = Skill::new(
        p.clone(),
        Namespace::root(),
        "style".to_string(),
        "v2".to_string(),
        "new content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &updated).await.unwrap();

    let read = SkillStore::find_by_name(&store, &p, &Namespace::root(), "style")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.content(), "new content");
    assert_eq!(read.description(), "v2");
}

#[tokio::test]
async fn skill_list_filters_by_namespace() {
    let store = backend();
    let pa = proj("proj-a");

    let s1 = Skill::new(
        pa.clone(),
        Namespace::root(),
        "style".to_string(),
        "A style".to_string(),
        "A content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &s1).await.unwrap();

    let s2 = Skill::new(
        pa.clone(),
        ns("/backend"),
        "arch".to_string(),
        "Backend arch".to_string(),
        "Hexagonal".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &s2).await.unwrap();

    let pb = proj("proj-b");
    let s3 = Skill::new(
        pb.clone(),
        Namespace::root(),
        "style".to_string(),
        "B style".to_string(),
        "B content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &s3).await.unwrap();

    let all_a = SkillStore::list(
        &store,
        SkillFilter {
            project: Some(pa.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(all_a.len(), 2);

    let only_b = SkillStore::list(
        &store,
        SkillFilter {
            project: Some(pb.clone()),
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
    let p = proj("test-project");

    let skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "temp".to_string(),
        "temporary".to_string(),
        "will be deleted".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &skill).await.unwrap();

    SkillStore::delete(&store, &p, &Namespace::root(), "temp")
        .await
        .unwrap();

    let read = SkillStore::find_by_name(&store, &p, &Namespace::root(), "temp")
        .await
        .unwrap();
    assert!(read.is_none());
}
