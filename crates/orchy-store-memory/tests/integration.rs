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

fn proj(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

#[tokio::test]
async fn agent_save_and_find() {
    let store = backend();
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
async fn agent_save_updates_existing() {
    let store = backend();
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
async fn agent_disconnect_sets_status() {
    let store = backend();
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
async fn agent_find_timed_out() {
    let store = backend();
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
async fn task_save_and_get() {
    let store = backend();
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
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
}

#[tokio::test]
async fn task_list_sorted_by_priority() {
    let store = backend();

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
async fn memory_save_and_find_by_key() {
    let store = backend();

    let mut entry = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "config".into(),
        "hello world".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &mut entry).await.unwrap();

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
    MemoryStore::save(&store, &mut entry).await.unwrap();

    entry.update("v2".into(), None).unwrap();
    MemoryStore::save(&store, &mut entry).await.unwrap();

    let read = MemoryStore::find_by_key(&store, &proj("app"), &Namespace::root(), "k")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.value(), "v2");
}

#[tokio::test]
async fn memory_list_with_namespace_prefix() {
    let store = backend();

    let mut entry_a =
        MemoryEntry::new(proj("app"), ns("/tasks"), "a".into(), "x".into(), None).unwrap();
    MemoryStore::save(&store, &mut entry_a).await.unwrap();

    let mut entry_b =
        MemoryEntry::new(proj("app"), ns("/other"), "b".into(), "y".into(), None).unwrap();
    MemoryStore::save(&store, &mut entry_b).await.unwrap();

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
async fn memory_search_by_substring() {
    let store = backend();

    let mut entry1 = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "notes".into(),
        "the quick brown fox".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &mut entry1).await.unwrap();

    let mut entry2 = MemoryEntry::new(
        proj("app"),
        Namespace::root(),
        "other".into(),
        "lazy dog".into(),
        None,
    )
    .unwrap();
    MemoryStore::save(&store, &mut entry2).await.unwrap();

    let results = MemoryStore::search(&store, "quick", None, None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key(), "notes");
}

#[tokio::test]
async fn memory_delete() {
    let store = backend();

    let mut entry =
        MemoryEntry::new(proj("app"), Namespace::root(), "k".into(), "v".into(), None).unwrap();
    MemoryStore::save(&store, &mut entry).await.unwrap();

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

    let mut msg = Message::new(
        p.clone(),
        Namespace::root(),
        from,
        MessageTarget::Agent(to),
        "hello".into(),
        None,
    );
    MessageStore::save(&store, &mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let messages = MessageStore::find_pending(&store, &to, &p, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body(), "hello");
    assert_eq!(messages[0].status(), MessageStatus::Pending);

    let mut delivered = messages.into_iter().next().unwrap();
    delivered.deliver();
    MessageStore::save(&store, &mut delivered).await.unwrap();

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

    let mut msg = Message::new(
        p.clone(),
        Namespace::root(),
        from,
        MessageTarget::Agent(to),
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

#[tokio::test]
async fn context_save_and_find_latest() {
    let store = backend();
    let agent = AgentId::new();

    let mut snap1 = ContextSnapshot::new(
        proj("proj"),
        agent,
        Namespace::root(),
        "first snapshot".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap1).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let mut snap2 = ContextSnapshot::new(
        proj("proj"),
        agent,
        Namespace::root(),
        "second snapshot".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap2).await.unwrap();

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

    let mut snap1 = ContextSnapshot::new(
        proj("proj"),
        agent1,
        Namespace::root(),
        "a1".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap1).await.unwrap();

    let mut snap2 = ContextSnapshot::new(
        proj("other"),
        agent2,
        ns("/sub"),
        "a2".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap2).await.unwrap();

    let by_agent = ContextStore::list(&store, Some(&agent1), &Namespace::root())
        .await
        .unwrap();
    assert_eq!(by_agent.len(), 1);
    assert_eq!(by_agent[0].summary(), "a1");

    let by_ns = ContextStore::list(&store, None, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(by_ns.len(), 2);
}

#[tokio::test]
async fn context_search_by_substring() {
    let store = backend();
    let agent = AgentId::new();

    let mut snap1 = ContextSnapshot::new(
        proj("test-project"),
        agent,
        Namespace::root(),
        "working on authentication module".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap1).await.unwrap();

    let mut snap2 = ContextSnapshot::new(
        proj("test-project"),
        agent,
        Namespace::root(),
        "fixing database migrations".into(),
        HashMap::new(),
    );
    ContextStore::save(&store, &mut snap2).await.unwrap();

    let results = ContextStore::search(&store, "auth", None, &Namespace::root(), None, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].summary().contains("authentication"));
}

#[tokio::test]
async fn skill_save_and_find_by_name() {
    let store = backend();
    let p = proj("test-project");

    let mut skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "commit-conventions".to_string(),
        "How to write commit messages".to_string(),
        "Use conventional commits".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut skill).await.unwrap();

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

    let mut skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "style".to_string(),
        "v1".to_string(),
        "old content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut skill).await.unwrap();

    let mut updated = Skill::new(
        p.clone(),
        Namespace::root(),
        "style".to_string(),
        "v2".to_string(),
        "new content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut updated).await.unwrap();

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
    let p = proj("proj-a");

    let mut s1 = Skill::new(
        p.clone(),
        Namespace::root(),
        "style".to_string(),
        "A style".to_string(),
        "A content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut s1).await.unwrap();

    let mut s2 = Skill::new(
        p.clone(),
        ns("/backend"),
        "arch".to_string(),
        "Backend arch".to_string(),
        "Hexagonal".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut s2).await.unwrap();

    let pb = proj("proj-b");
    let mut s3 = Skill::new(
        pb.clone(),
        Namespace::root(),
        "style".to_string(),
        "B style".to_string(),
        "B content".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut s3).await.unwrap();

    let all_a = SkillStore::list(
        &store,
        SkillFilter {
            project: Some(p.clone()),
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

    let mut skill = Skill::new(
        p.clone(),
        Namespace::root(),
        "temp".to_string(),
        "temporary".to_string(),
        "will be deleted".to_string(),
        None,
    )
    .unwrap();
    SkillStore::save(&store, &mut skill).await.unwrap();

    SkillStore::delete(&store, &p, &Namespace::root(), "temp")
        .await
        .unwrap();

    let read = SkillStore::find_by_name(&store, &p, &Namespace::root(), "temp")
        .await
        .unwrap();
    assert!(read.is_none());
}

#[tokio::test]
async fn message_find_sent() {
    let store = backend();
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let p = proj("proj");

    let mut msg = Message::new(
        p.clone(),
        ns("/backend"),
        sender,
        MessageTarget::Agent(receiver),
        "hello".into(),
        None,
    );
    MessageStore::save(&store, &mut msg).await.unwrap();

    let sent = MessageStore::find_sent(&store, &sender, &p, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body(), "hello");

    let sent_other = MessageStore::find_sent(&store, &receiver, &p, &Namespace::root())
        .await
        .unwrap();
    assert!(sent_other.is_empty());
}

#[tokio::test]
async fn message_find_thread() {
    let store = backend();
    let a = AgentId::new();
    let b = AgentId::new();
    let p = proj("proj");

    let mut msg1 = Message::new(
        p.clone(),
        Namespace::root(),
        a,
        MessageTarget::Agent(b),
        "first".into(),
        None,
    );
    MessageStore::save(&store, &mut msg1).await.unwrap();

    let mut msg2 = msg1.reply(b, "second".into());
    MessageStore::save(&store, &mut msg2).await.unwrap();

    let mut msg3 = msg2.reply(a, "third".into());
    MessageStore::save(&store, &mut msg3).await.unwrap();

    let thread = MessageStore::find_thread(&store, &msg3.id(), None)
        .await
        .unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(thread[0].body(), "first");
    assert_eq!(thread[1].body(), "second");
    assert_eq!(thread[2].body(), "third");

    let limited = MessageStore::find_thread(&store, &msg3.id(), Some(2))
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].body(), "second");
    assert_eq!(limited[1].body(), "third");
}

#[tokio::test]
async fn message_find_pending_includes_broadcast() {
    let store = backend();
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let p = proj("proj");

    let mut msg = Message::new(
        p.clone(),
        Namespace::root(),
        sender,
        MessageTarget::Broadcast,
        "to all".into(),
        None,
    );
    MessageStore::save(&store, &mut msg).await.unwrap();

    let pending = MessageStore::find_pending(&store, &receiver, &p, &Namespace::root())
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].body(), "to all");
}

#[tokio::test]
async fn task_list_filters_by_parent_id() {
    let store = backend();
    let p = proj("proj");

    let mut parent = Task::new(
        p.clone(),
        Namespace::root(),
        None,
        "parent".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut parent).await.unwrap();

    let mut child = Task::new(
        p.clone(),
        Namespace::root(),
        Some(parent.id()),
        "child".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut child).await.unwrap();

    let mut unrelated = Task::new(
        p.clone(),
        Namespace::root(),
        None,
        "other".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut unrelated).await.unwrap();

    let children = TaskStore::list(
        &store,
        TaskFilter {
            parent_id: Some(parent.id()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].title(), "child");
}

#[tokio::test]
async fn task_list_filters_by_assigned_to() {
    let store = backend();
    let agent = AgentId::new();

    let mut task = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "assigned".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    task.claim(agent).unwrap();
    TaskStore::save(&store, &mut task).await.unwrap();

    let mut other = Task::new(
        proj("proj"),
        Namespace::root(),
        None,
        "unassigned".into(),
        "".into(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(&store, &mut other).await.unwrap();

    let assigned = TaskStore::list(
        &store,
        TaskFilter {
            assigned_to: Some(agent),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0].title(), "assigned");
}
