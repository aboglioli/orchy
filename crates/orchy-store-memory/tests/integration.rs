use std::collections::HashMap;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
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

fn org() -> OrganizationId {
    OrganizationId::new("default").unwrap()
}

#[tokio::test]
async fn agent_save_and_find() {
    let store = backend();
    let mut agent = Agent::register(
        org(),
        proj("myapp"),
        Namespace::root(),
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
async fn agent_save_updates_existing() {
    let store = backend();
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        vec!["dev".into()],
        "original".into(),
        None,
        HashMap::new(),
    )
    .unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let before = agent.last_heartbeat();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    AgentStore::save(&store, &mut agent).await.unwrap();

    let updated = AgentStore::find_by_id(&store, agent.id())
        .await
        .unwrap()
        .unwrap();
    assert!(updated.last_heartbeat() > before);
}

#[tokio::test]
async fn agent_disconnect_sets_status() {
    let store = backend();
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
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
async fn agent_find_timed_out() {
    let store = backend();
    let mut agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
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
async fn task_save_and_get() {
    let store = backend();
    let mut task = Task::new(
        org(),
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
        org(),
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
        org(),
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

    let page = TaskStore::list(&store, TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
async fn message_save_and_find_pending() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let p = proj("test-project");

    let mut msg = Message::new(
        org(),
        p.clone(),
        Namespace::root(),
        from.clone(),
        MessageTarget::Agent(to.clone()),
        "hello".into(),
        None,
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let o = org();
    let page = MessageStore::find_pending(
        &store,
        &to,
        &[],
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].body(), "hello");
    assert_eq!(page.items[0].status(), MessageStatus::Pending);

    let mut delivered = page.items.into_iter().next().unwrap();
    delivered.deliver().unwrap();
    MessageStore::save(&store, &mut delivered).await.unwrap();

    let page = MessageStore::find_pending(
        &store,
        &to,
        &[],
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn message_find_by_id_and_mark_read() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let p = proj("test-project");

    let mut msg = Message::new(
        org(),
        p.clone(),
        Namespace::root(),
        from.clone(),
        MessageTarget::Agent(to.clone()),
        "hi".into(),
        None,
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
async fn message_find_sent() {
    let store = backend();
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let p = proj("proj");

    let mut msg = Message::new(
        org(),
        p.clone(),
        ns("/backend"),
        sender.clone(),
        MessageTarget::Agent(receiver.clone()),
        "hello".into(),
        None,
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();

    let o = org();
    let sent = MessageStore::find_sent(
        &store,
        &sender,
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(sent.items.len(), 1);
    assert_eq!(sent.items[0].body(), "hello");

    let sent_other = MessageStore::find_sent(
        &store,
        &receiver,
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(sent_other.items.is_empty());
}

#[tokio::test]
async fn message_find_thread() {
    let store = backend();
    let a = AgentId::new();
    let b = AgentId::new();
    let p = proj("proj");

    let mut msg1 = Message::new(
        org(),
        p.clone(),
        Namespace::root(),
        a.clone(),
        MessageTarget::Agent(b.clone()),
        "first".into(),
        None,
    )
    .unwrap();
    MessageStore::save(&store, &mut msg1).await.unwrap();

    let mut msg2 = msg1.reply(b.clone(), "second".into()).unwrap();
    MessageStore::save(&store, &mut msg2).await.unwrap();

    let mut msg3 = msg2.reply(a.clone(), "third".into()).unwrap();
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
        org(),
        p.clone(),
        Namespace::root(),
        sender.clone(),
        MessageTarget::Broadcast,
        "to all".into(),
        None,
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();

    let o = org();
    let pending = MessageStore::find_pending(
        &store,
        &receiver,
        &[],
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(pending.items.len(), 1);
    assert_eq!(pending.items[0].body(), "to all");

    let sender_pending = MessageStore::find_pending(
        &store,
        &sender,
        &[],
        &o,
        &p,
        &Namespace::root(),
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
        &o,
        &p,
        &Namespace::root(),
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(after_read.items.is_empty());
}

#[tokio::test]
async fn task_list_filters_by_parent_id() {
    let store = backend();
    let p = proj("proj");

    let mut parent = Task::new(
        org(),
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
        org(),
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
        org(),
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
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(children.items.len(), 1);
    assert_eq!(children.items[0].title(), "child");
}

#[tokio::test]
async fn task_list_filters_by_assigned_to() {
    let store = backend();
    let agent = AgentId::new();

    let mut task = Task::new(
        org(),
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
    task.claim(agent.clone()).unwrap();
    TaskStore::save(&store, &mut task).await.unwrap();

    let mut other = Task::new(
        org(),
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
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(assigned.items.len(), 1);
    assert_eq!(assigned.items[0].title(), "assigned");
}

#[tokio::test]
async fn knowledge_save_and_find() {
    let store = backend();
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        "decisions/db".into(),
        KnowledgeKind::Decision,
        "Database choice".into(),
        "We chose PostgreSQL".into(),
        vec!["infra".into()],
        None,
        HashMap::new(),
    )
    .unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let fetched = KnowledgeStore::find_by_id(&store, &entry.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.title(), "Database choice");
    assert_eq!(fetched.version().as_u64(), 1);
}

#[tokio::test]
async fn knowledge_optimistic_concurrency_rejects_stale_version() {
    let store = backend();
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        "my-note".into(),
        KnowledgeKind::Note,
        "v1 title".into(),
        "v1 content".into(),
        vec![],
        None,
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    entry
        .update("v2 title".into(), "v2 content".into(), None)
        .unwrap();
    assert_eq!(entry.version().as_u64(), 2);
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let mut stale = KnowledgeStore::find_by_id(&store, &entry.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.version().as_u64(), 2);

    entry
        .update("v3 title".into(), "v3 content".into(), None)
        .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    stale
        .update("stale update".into(), "stale".into(), None)
        .unwrap();
    assert_eq!(stale.version().as_u64(), 3);
    let err = KnowledgeStore::save(&store, &mut stale).await.unwrap_err();
    assert!(
        matches!(
            err,
            orchy_core::error::Error::VersionMismatch {
                expected: 2,
                actual: 3
            }
        ),
        "expected VersionMismatch, got: {err:?}"
    );
}

#[tokio::test]
async fn knowledge_optimistic_concurrency_allows_correct_version() {
    let store = backend();
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        "my-note".into(),
        KnowledgeKind::Note,
        "v1".into(),
        "v1".into(),
        vec![],
        None,
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    entry.update("v2".into(), "v2".into(), None).unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    entry.update("v3".into(), "v3".into(), None).unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    let fetched = KnowledgeStore::find_by_id(&store, &entry.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.version().as_u64(), 3);
    assert_eq!(fetched.title(), "v3");
}

#[tokio::test]
async fn edge_exists_by_pair_detects_duplicate() {
    let store = backend();
    let o = org();

    let edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-1".to_string(),
        ResourceKind::Knowledge,
        "know-1".to_string(),
        RelationType::Produces,
        None,
        None,
    );
    EdgeStore::save(&store, &edge).await.unwrap();

    assert!(
        EdgeStore::exists_by_pair(
            &store,
            &o,
            &ResourceKind::Task,
            "task-1",
            &ResourceKind::Knowledge,
            "know-1",
            &RelationType::Produces,
        )
        .await
        .unwrap()
    );

    assert!(
        !EdgeStore::exists_by_pair(
            &store,
            &o,
            &ResourceKind::Task,
            "task-1",
            &ResourceKind::Knowledge,
            "know-1",
            &RelationType::RelatedTo,
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn edge_list_by_org_returns_all_and_filters_by_rel_type() {
    let store = backend();
    let o = org();

    let e1 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
        None,
    );
    let e2 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t2".to_string(),
        ResourceKind::Task,
        "t3".to_string(),
        RelationType::Spawns,
        None,
        None,
    );
    EdgeStore::save(&store, &e1).await.unwrap();
    EdgeStore::save(&store, &e2).await.unwrap();

    let all = EdgeStore::list_by_org(&store, &o, None, PageParams::default())
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);

    let spawns_only = EdgeStore::list_by_org(
        &store,
        &o,
        Some(&RelationType::Spawns),
        PageParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(spawns_only.items.len(), 1);
    assert_eq!(spawns_only.items[0].from_id(), "t2");
}

#[tokio::test]
async fn delete_knowledge_cleans_up_associated_edges() {
    let store = backend();
    let o = org();

    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("myapp")),
        ns("/"),
        "test-decision".to_string(),
        KnowledgeKind::Decision,
        "Test".to_string(),
        "content".to_string(),
        vec![],
        None,
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    let kid = entry.id().to_string();

    let edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-1".to_string(),
        ResourceKind::Knowledge,
        kid.clone(),
        RelationType::Produces,
        None,
        None,
    );
    EdgeStore::save(&store, &edge).await.unwrap();

    let before = EdgeStore::list_by_org(&store, &o, None, PageParams::default())
        .await
        .unwrap();
    assert_eq!(before.items.len(), 1);

    entry.mark_deleted().unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    KnowledgeStore::delete(&store, &entry.id()).await.unwrap();
    EdgeStore::delete_all_for(&store, &o, &ResourceKind::Knowledge, &kid)
        .await
        .unwrap();

    let after = EdgeStore::list_by_org(&store, &o, None, PageParams::default())
        .await
        .unwrap();
    assert_eq!(after.items.len(), 0);
}

#[tokio::test]
async fn split_task_creates_spawns_edges() {
    use std::sync::Arc;

    use orchy_application::{SplitTask, SplitTaskCommand, SubtaskInput};

    let store = Arc::new(backend());
    let o = org();

    let mut parent = Task::new(
        o.clone(),
        proj("myapp"),
        ns("/"),
        None,
        "Parent task".to_string(),
        "desc".to_string(),
        Priority::Normal,
        vec![],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(store.as_ref(), &mut parent).await.unwrap();
    let parent_id = parent.id().to_string();

    let cmd = SplitTaskCommand {
        task_id: parent_id.clone(),
        subtasks: vec![
            SubtaskInput {
                title: "Sub A".to_string(),
                description: "desc".to_string(),
                priority: None,
                assigned_roles: None,
                depends_on: None,
            },
            SubtaskInput {
                title: "Sub B".to_string(),
                description: "desc".to_string(),
                priority: None,
                assigned_roles: None,
                depends_on: None,
            },
        ],
        created_by: None,
    };

    let split = SplitTask::new(
        store.clone() as Arc<dyn orchy_core::task::TaskStore>,
        store.clone() as Arc<dyn EdgeStore>,
    );
    split.execute(cmd).await.unwrap();

    let edges = EdgeStore::list_by_org(
        store.as_ref(),
        &o,
        Some(&RelationType::Spawns),
        PageParams::default(),
    )
    .await
    .unwrap();
    assert_eq!(edges.items.len(), 2);
    assert!(edges.items.iter().all(|e| e.from_id() == parent_id));
}

#[tokio::test]
async fn delete_by_pair_removes_matching_edge() {
    let store = backend();
    let o = org();
    let edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".into(),
        ResourceKind::Task,
        "t2".into(),
        RelationType::DependsOn,
        None,
        None,
    );
    EdgeStore::save(&store, &edge).await.unwrap();

    EdgeStore::delete_by_pair(
        &store,
        &o,
        &ResourceKind::Task,
        "t1",
        &ResourceKind::Task,
        "t2",
        &RelationType::DependsOn,
    )
    .await
    .unwrap();

    assert!(
        EdgeStore::find_by_id(&store, &edge.id())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn split_task_creates_depends_on_edges_for_subtask_deps() {
    use std::sync::Arc;

    use orchy_application::{PostTask, PostTaskCommand, SplitTask, SplitTaskCommand, SubtaskInput};

    let backend = Arc::new(backend());
    let tasks: Arc<dyn orchy_core::task::TaskStore> = backend.clone();
    let edges: Arc<dyn EdgeStore> = backend.clone();

    let post = PostTask::new(tasks.clone(), edges.clone());
    let split = SplitTask::new(tasks.clone(), edges.clone());

    let dep = post
        .execute(PostTaskCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            title: "Dep".into(),
            description: "desc".into(),
            priority: None,
            assigned_roles: None,
            depends_on: None,
            parent_id: None,
            created_by: None,
        })
        .await
        .unwrap();

    let parent = post
        .execute(PostTaskCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            title: "Parent".into(),
            description: "desc".into(),
            priority: None,
            assigned_roles: None,
            depends_on: None,
            parent_id: None,
            created_by: None,
        })
        .await
        .unwrap();

    let (_, subtasks) = split
        .execute(SplitTaskCommand {
            task_id: parent.id.clone(),
            subtasks: vec![SubtaskInput {
                title: "Sub".into(),
                description: "desc".into(),
                priority: None,
                assigned_roles: None,
                depends_on: Some(vec![dep.id.clone()]),
            }],
            created_by: None,
        })
        .await
        .unwrap();

    let sub = &subtasks[0];
    assert_eq!(sub.status, "blocked");

    let o = OrganizationId::new("test-org").unwrap();
    let dep_edges = EdgeStore::find_from(
        backend.as_ref(),
        &o,
        &ResourceKind::Task,
        &sub.id,
        Some(&RelationType::DependsOn),
    )
    .await
    .unwrap();

    assert_eq!(dep_edges.len(), 1);
    assert_eq!(dep_edges[0].to_id(), dep.id.as_str());
}

#[tokio::test]
async fn delete_by_pair_ignores_different_rel_type() {
    let store = backend();
    let o = org();
    let edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".into(),
        ResourceKind::Task,
        "t2".into(),
        RelationType::Spawns,
        None,
        None,
    );
    EdgeStore::save(&store, &edge).await.unwrap();

    EdgeStore::delete_by_pair(
        &store,
        &o,
        &ResourceKind::Task,
        "t1",
        &ResourceKind::Task,
        "t2",
        &RelationType::DependsOn,
    )
    .await
    .unwrap();

    assert!(
        EdgeStore::find_by_id(&store, &edge.id())
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn knowledge_search_returns_score() {
    let store = backend();
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        "search-target".into(),
        KnowledgeKind::Note,
        "PostgreSQL indexing".into(),
        "We use GIN indexes for full text search".into(),
        vec![],
        None,
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let results =
        KnowledgeStore::search(&store, &org(), "GIN indexes for full text", None, None, 20)
            .await
            .unwrap();

    assert!(!results.is_empty());
    let (_, score) = &results[0];
    assert!(score.is_some());
}

#[tokio::test]
async fn get_graph_include_nodes_hydrates_task_fields() {
    use std::sync::Arc;

    use orchy_application::{GetGraph, GetGraphCommand};

    let backend = Arc::new(backend());
    let tasks: Arc<dyn orchy_core::task::TaskStore> = backend.clone();
    let edges: Arc<dyn EdgeStore> = backend.clone();
    let knowledge: Arc<dyn orchy_core::knowledge::KnowledgeStore> = backend.clone();
    let agents: Arc<dyn orchy_core::agent::AgentStore> = backend.clone();

    let o = org();

    let mut task = Task::new(
        o.clone(),
        proj("proj"),
        Namespace::root(),
        None,
        "Implement login".into(),
        "Build the login endpoint with JWT".into(),
        Priority::High,
        vec!["dev".into()],
        vec![],
        None,
        false,
    )
    .unwrap();
    TaskStore::save(backend.as_ref(), &mut task).await.unwrap();
    let task_id = task.id().to_string();

    let edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        task_id.clone(),
        ResourceKind::Task,
        task_id.clone(),
        RelationType::RelatedTo,
        None,
        None,
    );
    EdgeStore::save(backend.as_ref(), &edge).await.unwrap();

    let get_graph = GetGraph::new(edges, tasks, knowledge, agents);
    let resp = get_graph
        .execute(GetGraphCommand {
            org_id: o.to_string(),
            kind: "task".into(),
            id: task_id.clone(),
            max_depth: None,
            rel_types: None,
            direction: None,
            include_nodes: true,
            node_content_limit: Some(200),
        })
        .await
        .unwrap();

    let nodes = resp
        .nodes
        .expect("nodes should be Some when include_nodes=true");
    let node_key = format!("task:{task_id}");
    let node = nodes.get(&node_key).expect("task node should be present");

    assert_eq!(node.kind, "task");
    assert_eq!(node.label, "Implement login");
    assert!(node.status.is_some(), "status should be set for task node");
    assert_eq!(node.status.as_deref(), Some("pending"));
    assert!(
        node.priority.is_some(),
        "priority should be set for task node"
    );
    assert!(
        node.content.is_some(),
        "content should be set for task node"
    );
    assert!(node.content.as_deref().unwrap().contains("JWT"));
    assert!(
        node.updated_at.is_some(),
        "updated_at should be set for task node"
    );
}
