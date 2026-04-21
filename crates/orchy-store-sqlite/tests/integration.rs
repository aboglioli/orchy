use std::collections::HashMap;

use chrono::Utc;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, Alias};
use orchy_core::edge::{Edge, EdgeStore, RelationType, TraversalDirection};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
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

fn org(s: &str) -> OrganizationId {
    OrganizationId::new(s).unwrap()
}

#[tokio::test]
async fn agent_save_and_find() {
    let store = backend();
    let mut agent = Agent::register(
        org("default"),
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
async fn agent_save_updates_existing() {
    let store = backend();
    let mut agent = Agent::register(
        org("default"),
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
async fn agent_disconnect_sets_status() {
    let store = backend();
    let mut agent = Agent::register(
        org("default"),
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
async fn agent_find_timed_out() {
    let store = backend();
    let mut agent = Agent::register(
        org("default"),
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
async fn task_save_and_get() {
    let store = backend();
    let mut task = Task::new(
        org("default"),
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
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
}

#[tokio::test]
async fn task_save_overwrites_existing() {
    let store = backend();
    let mut task = Task::new(
        org("default"),
        proj("proj"),
        Namespace::root(),
        "original".into(),
        "desc".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();

    TaskStore::save(&store, &mut task).await.unwrap();

    let mut updated = Task::restore(RestoreTask {
        id: task.id(),
        org_id: org("default"),
        project: proj("proj"),
        namespace: Namespace::root(),
        title: "updated".into(),
        description: "new desc".into(),
        acceptance_criteria: None,
        status: TaskStatus::Completed,
        priority: Priority::High,
        assigned_roles: vec![],
        assigned_to: None,
        assigned_at: None,
        stale_after_secs: None,
        last_activity_at: Utc::now(),
        tags: vec![],
        result_summary: Some("done".into()),
        created_by: None,
        created_at: task.created_at(),
        updated_at: task.updated_at(),
    });
    TaskStore::save(&store, &mut updated).await.unwrap();

    let fetched = TaskStore::find_by_id(&store, &task.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.title(), "updated");
    assert_eq!(fetched.status(), TaskStatus::Completed);
    assert_eq!(fetched.result_summary(), Some("done"));
}

#[tokio::test]
async fn task_list_sorted_by_priority() {
    let store = backend();

    let mut low = Task::new(
        org("default"),
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
        org("default"),
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
async fn message_save_and_find_unread() {
    let store = backend();

    let from = AgentId::new();
    let to = AgentId::new();

    let o = org("default");
    let p = proj("test-project");

    let mut msg = Message::new(
        o.clone(),
        p.clone(),
        Namespace::root(),
        from.clone(),
        MessageTarget::Agent(to.clone()),
        "hello".into(),
        None,
        vec![],
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let page = MessageStore::find_unread(
        &store,
        &to,
        &[],
        &o,
        &p,
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

    let page = MessageStore::find_unread(
        &store,
        &to,
        &[],
        &o,
        &p,
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

    let o = org("default");
    let p = proj("test-project");

    let mut msg = Message::new(
        o.clone(),
        p.clone(),
        Namespace::root(),
        from.clone(),
        MessageTarget::Agent(to.clone()),
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
async fn message_find_sent() {
    let store = backend();
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let o = org("default");
    let p = proj("proj");

    let mut msg = Message::new(
        o.clone(),
        p.clone(),
        ns("/backend"),
        sender.clone(),
        MessageTarget::Agent(receiver.clone()),
        "hello".into(),
        None,
        vec![],
    )
    .unwrap();
    MessageStore::save(&store, &mut msg).await.unwrap();

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
    let o = org("default");
    let p = proj("proj");

    let mut msg1 = Message::new(
        o.clone(),
        p.clone(),
        Namespace::root(),
        a.clone(),
        MessageTarget::Agent(b.clone()),
        "first".into(),
        None,
        vec![],
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
async fn message_find_unread_includes_broadcast() {
    let store = backend();
    let sender = AgentId::new();
    let receiver = AgentId::new();
    let o = org("default");
    let p = proj("proj");

    let mut msg = Message::new(
        o.clone(),
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

    let pending = MessageStore::find_unread(
        &store,
        &receiver,
        &[],
        &o,
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert_eq!(pending.items.len(), 1);
    assert_eq!(pending.items[0].body(), "to all");

    let sender_pending = MessageStore::find_unread(
        &store,
        &sender,
        &[],
        &o,
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(sender_pending.items.is_empty());

    MessageStore::mark_read(&store, &receiver, &[msg.id()])
        .await
        .unwrap();

    let after_read = MessageStore::find_unread(
        &store,
        &receiver,
        &[],
        &o,
        &p,
        PageParams::unbounded(),
    )
    .await
    .unwrap();
    assert!(after_read.items.is_empty());
}

#[tokio::test]
async fn task_list_filters_by_assigned_to() {
    let store = backend();
    let agent = AgentId::new();

    let mut task = Task::new(
        org("default"),
        proj("proj"),
        Namespace::root(),
        "assigned".into(),
        "".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    task.claim(agent.clone()).unwrap();
    TaskStore::save(&store, &mut task).await.unwrap();

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
async fn knowledge_search_fts_finds_content() {
    let store = backend();
    let o = org("default");
    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        "auth/jwt".into(),
        KnowledgeKind::Note,
        "JWT notes".into(),
        "Use RS256 for asymmetric cryptography verification.".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let hits = KnowledgeStore::search(&store, &o, "cryptography", None, None, 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.path(), "auth/jwt");
}

#[tokio::test]
async fn knowledge_metadata_merge_and_remove() {
    let store = backend();
    let o = org("default");

    let mut md = HashMap::new();
    md.insert("a".into(), "1".into());
    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        "meta-test".into(),
        KnowledgeKind::Note,
        "t".into(),
        "body".into(),
        vec![],
        md,
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let mut entry = KnowledgeStore::find_by_path(
        &store,
        &o,
        Some(&proj("p")),
        &Namespace::root(),
        "meta-test",
    )
    .await
    .unwrap()
    .unwrap();
    entry.update("t".into(), "body2".into()).unwrap();
    entry.remove_metadata("a").unwrap();
    entry.set_metadata("b".into(), "2".into()).unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let entry = KnowledgeStore::find_by_path(
        &store,
        &o,
        Some(&proj("p")),
        &Namespace::root(),
        "meta-test",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(entry.metadata().get("a"), None);
    assert_eq!(entry.metadata().get("b").map(String::as_str), Some("2"));

    let mut entry = KnowledgeStore::find_by_path(
        &store,
        &o,
        Some(&proj("p")),
        &Namespace::root(),
        "meta-test",
    )
    .await
    .unwrap()
    .unwrap();
    entry.remove_metadata("b").unwrap();
    entry.set_metadata("c".into(), "3".into()).unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let entry = KnowledgeStore::find_by_path(
        &store,
        &o,
        Some(&proj("p")),
        &Namespace::root(),
        "meta-test",
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(entry.metadata().get("b"), None);
    assert_eq!(entry.metadata().get("c").map(String::as_str), Some("3"));
}

#[tokio::test]
async fn knowledge_optimistic_concurrency_rejects_stale_version() {
    let store = backend();
    let o = org("default");

    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        "my-note".into(),
        KnowledgeKind::Note,
        "v1 title".into(),
        "v1 content".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    entry
        .update("v2 title".into(), "v2 content".into())
        .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    let mut stale = KnowledgeStore::find_by_id(&store, &entry.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stale.version().as_u64(), 2);

    entry
        .update("v3 title".into(), "v3 content".into())
        .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    stale.update("stale update".into(), "stale".into()).unwrap();
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
        org("default"),
        Some(proj("p")),
        Namespace::root(),
        "my-note".into(),
        KnowledgeKind::Note,
        "v1".into(),
        "v1".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    entry.update("v2".into(), "v2".into()).unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    entry.update("v3".into(), "v3".into()).unwrap();
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
async fn edge_valid_until_persisted_and_filtered() {
    let store = backend();
    let org = org("default");
    let mut edge = Edge::new(
        org.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    EdgeStore::save(&store, &mut edge).await.unwrap();

    let found = EdgeStore::find_from(&store, &org, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let mut invalidated = found.into_iter().next().unwrap();
    invalidated.invalidate().unwrap();
    EdgeStore::save(&store, &mut invalidated).await.unwrap();

    let found = EdgeStore::find_from(&store, &org, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn edge_traverse_both_reaches_edges_connected_via_incoming_neighbor() {
    let store = backend();
    let o = org("default");

    let mut edge_to_root = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "neighbor".into(),
        ResourceKind::Task,
        "root".into(),
        RelationType::RelatedTo,
        None,
    )
    .unwrap();
    let mut edge_from_neighbor = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "neighbor".into(),
        ResourceKind::Task,
        "leaf".into(),
        RelationType::RelatedTo,
        None,
    )
    .unwrap();

    EdgeStore::save(&store, &mut edge_to_root).await.unwrap();
    EdgeStore::save(&store, &mut edge_from_neighbor)
        .await
        .unwrap();

    let hops = EdgeStore::find_neighbors(
        &store,
        &o,
        &ResourceKind::Task,
        "root",
        &[],
        &[],
        TraversalDirection::Both,
        3,
        None,
        100,
    )
    .await
    .unwrap();

    assert!(
        hops.iter()
            .any(|h| h.edge.from_id() == "neighbor" && h.edge.to_id() == "leaf"),
        "expected traversal to include neighbor -> leaf edge"
    );
}

#[tokio::test]
async fn edge_as_of_returns_historical_snapshot() {
    use chrono::Duration;
    let store = backend();
    let org = org("default");
    let mut edge = Edge::new(
        org.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    EdgeStore::save(&store, &mut edge).await.unwrap();

    let snapshot_time = edge.created_at();

    let found = EdgeStore::find_from(
        &store,
        &org,
        &ResourceKind::Task,
        "t1",
        &[],
        Some(snapshot_time),
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);

    edge.invalidate().unwrap();
    EdgeStore::save(&store, &mut edge).await.unwrap();

    let found = EdgeStore::find_from(
        &store,
        &org,
        &ResourceKind::Task,
        "t1",
        &[],
        Some(snapshot_time),
    )
    .await
    .unwrap();
    assert_eq!(found.len(), 1);

    let after = edge.valid_until().unwrap() + Duration::seconds(1);
    let found = EdgeStore::find_from(&store, &org, &ResourceKind::Task, "t1", &[], Some(after))
        .await
        .unwrap();
    assert!(found.is_empty());
}
