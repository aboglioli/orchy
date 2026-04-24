use std::collections::HashMap;

use chrono::Utc;

use orchy_core::agent::{Agent, AgentId, AgentStore as _, Alias};
use orchy_core::graph::{Edge, EdgeStore as _, RelationType, TraversalDirection};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeKind, KnowledgePath, KnowledgeStore as _,
};
use orchy_core::message::{Message, MessageStatus, MessageStore as _, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskStatus, TaskStore as _};
use orchy_store_sqlite::{
    SqliteAgentStore, SqliteDatabase, SqliteEdgeStore, SqliteEventQuery, SqliteKnowledgeStore,
    SqliteMessageStore, SqliteTaskStore,
};

struct Stores {
    agent: SqliteAgentStore,
    task: SqliteTaskStore,
    message: SqliteMessageStore,
    knowledge: SqliteKnowledgeStore,
    edge: SqliteEdgeStore,
    events: SqliteEventQuery,
}

fn backend() -> Stores {
    let db = SqliteDatabase::new(":memory:", None).unwrap();
    db.apply_schema().unwrap();
    let conn = db.conn();
    Stores {
        agent: SqliteAgentStore::new(conn.clone()),
        task: SqliteTaskStore::new(conn.clone()),
        message: SqliteMessageStore::new(conn.clone()),
        knowledge: SqliteKnowledgeStore::new(conn.clone()),
        edge: SqliteEdgeStore::new(conn.clone()),
        events: SqliteEventQuery::new(conn),
    }
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
    let s = backend();
    let mut agent = Agent::register(
        org("default"),
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
    s.agent.save(&mut agent).await.unwrap();

    assert_eq!(agent.derived_status(30, 300), "active");
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = s.agent.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
async fn agent_save_updates_existing() {
    let s = backend();
    let mut agent = Agent::register(
        org("default"),
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
    s.agent.save(&mut agent).await.unwrap();

    let before = agent.last_seen();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    s.agent.save(&mut agent).await.unwrap();

    let updated = s.agent.find_by_id(agent.id()).await.unwrap().unwrap();
    assert!(updated.last_seen() > before);
}

#[tokio::test]
async fn agent_save_and_fetch_roundtrip() {
    let s = backend();
    let mut agent = Agent::register(
        org("default"),
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
    s.agent.save(&mut agent).await.unwrap();

    s.agent.save(&mut agent).await.unwrap();

    let fetched = s.agent.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.alias().as_str(), "test-agent");
}

#[tokio::test]
async fn agent_find_timed_out() {
    let s = backend();
    let mut agent = Agent::register(
        org("default"),
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
    s.agent.save(&mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = s.agent.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    s.agent.save(&mut agent).await.unwrap();
    let timed_out = s.agent.find_timed_out(0).await.unwrap();
    // agent was saved with current timestamp and is still timed out at threshold 0
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
async fn task_save_and_get() {
    let s = backend();
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

    s.task.save(&mut task).await.unwrap();

    let fetched = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
}

#[tokio::test]
async fn task_save_persists_event_log() {
    let s = backend();
    let organization = org("default");
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

    s.task.save(&mut task).await.unwrap();

    let events = s
        .events
        .query_events(
            organization.as_str(),
            chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            10,
        )
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic, "task.created");
}

#[tokio::test]
async fn task_save_overwrites_existing() {
    let s = backend();
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

    s.task.save(&mut task).await.unwrap();

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
        archived_at: None,
        created_by: None,
        created_at: task.created_at(),
        updated_at: task.updated_at(),
    });
    s.task.save(&mut updated).await.unwrap();

    let fetched = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.title(), "updated");
    assert_eq!(fetched.status(), TaskStatus::Completed);
    assert_eq!(fetched.result_summary(), Some("done"));
}

#[tokio::test]
async fn task_list_sorted_by_priority() {
    let s = backend();

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
    s.task.save(&mut low).await.unwrap();

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
    s.task.save(&mut critical).await.unwrap();

    let page = s
        .task
        .list(TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
async fn message_save_and_find_unread() {
    let s = backend();

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
    s.message.save(&mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let page = s
        .message
        .find_unread(
            &to,
            &[],
            &Namespace::root(),
            None,
            &o,
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].body(), "hello");
    assert_eq!(page.items[0].status(), MessageStatus::Pending);

    let msg_id = page.items[0].id();
    s.message.mark_read(&to, &[msg_id]).await.unwrap();

    let page = s
        .message
        .find_unread(
            &to,
            &[],
            &Namespace::root(),
            None,
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
    let s = backend();

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
    s.message.save(&mut msg).await.unwrap();

    let mut fetched = s.message.find_by_id(&msg.id()).await.unwrap().unwrap();
    fetched.mark_read().unwrap();
    s.message.save(&mut fetched).await.unwrap();

    let read = s.message.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
async fn message_find_by_id_preserves_claim_state() {
    let s = backend();
    let claimer = AgentId::new();

    let mut msg = Message::new(
        org("default"),
        proj("test-project"),
        Namespace::root(),
        AgentId::new(),
        MessageTarget::Broadcast,
        "claimable".into(),
        None,
        vec![],
    )
    .unwrap();
    msg.claim(claimer.clone()).unwrap();
    s.message.save(&mut msg).await.unwrap();

    let fetched = s.message.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(fetched.claimed_by(), Some(&claimer));

    let mut fetched = fetched;
    fetched.unclaim(&claimer).unwrap();
    s.message.save(&mut fetched).await.unwrap();

    let unclaimed = s.message.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert!(unclaimed.claimed_by().is_none());
}

#[tokio::test]
async fn message_find_sent() {
    let s = backend();
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
    s.message.save(&mut msg).await.unwrap();

    let sent = s
        .message
        .find_sent(&sender, &o, &p, &Namespace::root(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(sent.items.len(), 1);
    assert_eq!(sent.items[0].body(), "hello");

    let sent_other = s
        .message
        .find_sent(
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
    let s = backend();
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
    s.message.save(&mut msg1).await.unwrap();

    let mut msg2 = msg1.reply(b.clone(), "second".into()).unwrap();
    s.message.save(&mut msg2).await.unwrap();

    let mut msg3 = msg2.reply(a.clone(), "third".into()).unwrap();
    s.message.save(&mut msg3).await.unwrap();

    let thread = s.message.find_thread(&msg3.id(), None).await.unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(thread[0].body(), "first");
    assert_eq!(thread[1].body(), "second");
    assert_eq!(thread[2].body(), "third");

    let limited = s.message.find_thread(&msg3.id(), Some(2)).await.unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].body(), "second");
    assert_eq!(limited[1].body(), "third");
}

#[tokio::test]
async fn message_find_unread_includes_broadcast() {
    let s = backend();
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
    s.message.save(&mut msg).await.unwrap();

    let pending = s
        .message
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &o,
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 1);
    assert_eq!(pending.items[0].body(), "to all");

    let sender_pending = s
        .message
        .find_unread(
            &sender,
            &[],
            &Namespace::root(),
            None,
            &o,
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(sender_pending.items.is_empty());

    s.message.mark_read(&receiver, &[msg.id()]).await.unwrap();

    let after_read = s
        .message
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
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
    let s = backend();
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
    s.task.save(&mut task).await.unwrap();

    let assigned = s
        .task
        .list(
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
async fn task_archive_and_unarchive() {
    let s = backend();
    let mut task = Task::new(
        org("default"),
        proj("test-project"),
        Namespace::root(),
        "Archive test task".to_string(),
        "Description".to_string(),
        None,
        Priority::Normal,
        vec!["coder".to_string()],
        None,
        false,
    )
    .unwrap();
    s.task.save(&mut task).await.unwrap();

    let agent_id = "01234567-89ab-cdef-0123-456789abcdef"
        .parse::<AgentId>()
        .unwrap();
    let mut task = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    task.claim(agent_id.clone()).unwrap();
    task.start(&agent_id).unwrap();
    task.complete(Some("done".into())).unwrap();
    s.task.save(&mut task).await.unwrap();

    let mut fetched = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    fetched.archive(Some("completed".into())).unwrap();
    s.task.save(&mut fetched).await.unwrap();

    let archived = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    assert!(archived.is_archived());
    assert!(archived.archived_at().is_some());

    let filter = TaskFilter {
        org_id: Some(org("default")),
        project: Some(proj("test-project")),
        include_archived: Some(false),
        ..Default::default()
    };
    let page = s.task.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().all(|task| !task.is_archived()));

    let filter = TaskFilter {
        org_id: Some(org("default")),
        project: Some(proj("test-project")),
        include_archived: Some(true),
        ..Default::default()
    };
    let page = s.task.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().any(|task| task.is_archived()));

    let mut restored = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    restored.unarchive().unwrap();
    s.task.save(&mut restored).await.unwrap();
    let active = s.task.find_by_id(&task.id()).await.unwrap().unwrap();
    assert!(!active.is_archived());
}

#[tokio::test]
async fn knowledge_search_fts_finds_content() {
    let s = backend();
    let o = org("default");
    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        KnowledgePath::new("auth/jwt").unwrap(),
        KnowledgeKind::Note,
        "JWT notes".into(),
        "Use RS256 for asymmetric cryptography verification.".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let hits = s
        .knowledge
        .search(&o, "cryptography", None, None, 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0.path().as_str(), "auth/jwt");
}

#[tokio::test]
async fn knowledge_archive_and_unarchive() {
    let s = backend();
    let mut entry = Knowledge::new(
        org("default"),
        Some(proj("test-project")),
        Namespace::root(),
        "test-arch".parse::<KnowledgePath>().unwrap(),
        KnowledgeKind::Note,
        "archive test".to_string(),
        "content".to_string(),
        vec![],
        Default::default(),
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let mut fetched = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    fetched.archive(Some("test reason".into())).unwrap();
    s.knowledge.save(&mut fetched).await.unwrap();

    let archived = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert!(archived.is_archived());
    assert!(archived.archived_at().is_some());

    let filter = KnowledgeFilter {
        org_id: Some(org("default")),
        project: Some(proj("test-project")),
        include_archived: Some(false),
        ..Default::default()
    };
    let page = s
        .knowledge
        .list(filter, PageParams::default())
        .await
        .unwrap();
    assert!(page.items.iter().all(|entry| !entry.is_archived()));

    let filter = KnowledgeFilter {
        org_id: Some(org("default")),
        project: Some(proj("test-project")),
        include_archived: Some(true),
        ..Default::default()
    };
    let page = s
        .knowledge
        .list(filter, PageParams::default())
        .await
        .unwrap();
    assert!(page.items.iter().any(|entry| entry.is_archived()));

    let mut restored = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    restored.unarchive().unwrap();
    s.knowledge.save(&mut restored).await.unwrap();
    let active = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert!(!active.is_archived());
}

#[tokio::test]
async fn knowledge_find_by_path_returns_archived() {
    let s = backend();
    let mut entry = Knowledge::new(
        org("default"),
        Some(proj("test-project")),
        Namespace::root(),
        "test-arch-path".parse::<KnowledgePath>().unwrap(),
        KnowledgeKind::Note,
        "find_by_path archive test".to_string(),
        "content".to_string(),
        vec![],
        Default::default(),
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let mut fetched = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    fetched.archive(None).unwrap();
    s.knowledge.save(&mut fetched).await.unwrap();

    let found = s
        .knowledge
        .find_by_path(
            &org("default"),
            Some(&proj("test-project")),
            &Namespace::root(),
            &"test-arch-path".parse::<KnowledgePath>().unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(found.is_archived());
}

#[tokio::test]
async fn knowledge_metadata_merge_and_remove() {
    let s = backend();
    let o = org("default");

    let mut md = HashMap::new();
    md.insert("a".into(), "1".into());
    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        KnowledgePath::new("meta-test").unwrap(),
        KnowledgeKind::Note,
        "t".into(),
        "body".into(),
        vec![],
        md,
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let mut entry = s
        .knowledge
        .find_by_path(
            &o,
            Some(&proj("p")),
            &Namespace::root(),
            &KnowledgePath::new("meta-test").unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    entry.update("t".into(), "body2".into()).unwrap();
    entry.remove_metadata("a").unwrap();
    entry.set_metadata("b".into(), "2".into()).unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let entry = s
        .knowledge
        .find_by_path(
            &o,
            Some(&proj("p")),
            &Namespace::root(),
            &KnowledgePath::new("meta-test").unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.metadata().get("a"), None);
    assert_eq!(entry.metadata().get("b").map(String::as_str), Some("2"));

    let mut entry = s
        .knowledge
        .find_by_path(
            &o,
            Some(&proj("p")),
            &Namespace::root(),
            &KnowledgePath::new("meta-test").unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    entry.remove_metadata("b").unwrap();
    entry.set_metadata("c".into(), "3".into()).unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    let entry = s
        .knowledge
        .find_by_path(
            &o,
            Some(&proj("p")),
            &Namespace::root(),
            &KnowledgePath::new("meta-test").unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.metadata().get("b"), None);
    assert_eq!(entry.metadata().get("c").map(String::as_str), Some("3"));
}

#[tokio::test]
async fn knowledge_optimistic_concurrency_rejects_stale_version() {
    let s = backend();
    let o = org("default");

    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        KnowledgePath::new("my-note").unwrap(),
        KnowledgeKind::Note,
        "v1 title".into(),
        "v1 content".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    entry
        .update("v2 title".into(), "v2 content".into())
        .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    let mut stale = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert_eq!(stale.version().as_u64(), 2);

    entry
        .update("v3 title".into(), "v3 content".into())
        .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    stale.update("stale update".into(), "stale".into()).unwrap();
    assert_eq!(stale.version().as_u64(), 3);
    let err = s.knowledge.save(&mut stale).await.unwrap_err();
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
    let s = backend();

    let mut entry = Knowledge::new(
        org("default"),
        Some(proj("p")),
        Namespace::root(),
        KnowledgePath::new("my-note").unwrap(),
        KnowledgeKind::Note,
        "v1".into(),
        "v1".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    s.knowledge.save(&mut entry).await.unwrap();

    entry.update("v2".into(), "v2".into()).unwrap();
    s.knowledge.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    entry.update("v3".into(), "v3".into()).unwrap();
    s.knowledge.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    let fetched = s.knowledge.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert_eq!(fetched.version().as_u64(), 3);
    assert_eq!(fetched.title(), "v3");
}

#[tokio::test]
async fn edge_valid_until_persisted_and_filtered() {
    let s = backend();
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
    s.edge.save(&mut edge).await.unwrap();

    let found = s
        .edge
        .find_from(&org, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let mut invalidated = found.into_iter().next().unwrap();
    invalidated.invalidate().unwrap();
    s.edge.save(&mut invalidated).await.unwrap();

    let found = s
        .edge
        .find_from(&org, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn edge_traverse_both_reaches_edges_connected_via_incoming_neighbor() {
    let s = backend();
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

    s.edge.save(&mut edge_to_root).await.unwrap();
    s.edge.save(&mut edge_from_neighbor).await.unwrap();

    let hops = s
        .edge
        .find_neighbors(
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
    let s = backend();
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
    s.edge.save(&mut edge).await.unwrap();

    let snapshot_time = edge.created_at();

    let found = s
        .edge
        .find_from(&org, &ResourceKind::Task, "t1", &[], Some(snapshot_time))
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    edge.invalidate().unwrap();
    s.edge.save(&mut edge).await.unwrap();

    let found = s
        .edge
        .find_from(&org, &ResourceKind::Task, "t1", &[], Some(snapshot_time))
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let after = edge.valid_until().unwrap() + Duration::seconds(1);
    let found = s
        .edge
        .find_from(&org, &ResourceKind::Task, "t1", &[], Some(after))
        .await
        .unwrap();
    assert!(found.is_empty());
}
