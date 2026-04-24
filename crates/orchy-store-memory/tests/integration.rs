use std::collections::HashMap;
use std::sync::Arc;

use chrono::Duration;
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeKind, KnowledgePath, KnowledgeStore,
};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_memory::*;

fn state() -> Arc<MemoryState> {
    Arc::new(MemoryState::new())
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
    let s = state();
    let store = MemoryAgentStore::new(s);
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
    store.save(&mut agent).await.unwrap();

    assert_eq!(agent.derived_status(30, 300), "active");
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = store.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
async fn agent_save_updates_existing() {
    let s = state();
    let store = MemoryAgentStore::new(s);
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
    store.save(&mut agent).await.unwrap();

    let before = agent.last_seen();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    store.save(&mut agent).await.unwrap();

    let updated = store.find_by_id(agent.id()).await.unwrap().unwrap();
    assert!(updated.last_seen() > before);
}

#[tokio::test]
async fn agent_save_and_fetch_roundtrip() {
    let s = state();
    let store = MemoryAgentStore::new(s);
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
    store.save(&mut agent).await.unwrap();

    store.save(&mut agent).await.unwrap();

    let fetched = store.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.alias().as_str(), "test-agent");
}

#[tokio::test]
async fn agent_find_timed_out() {
    let s = state();
    let store = MemoryAgentStore::new(s);
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
    store.save(&mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = store.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    store.save(&mut agent).await.unwrap();
    let timed_out = store.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
async fn task_save_and_get() {
    let s = state();
    let store = MemoryTaskStore::new(s);
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

    store.save(&mut task).await.unwrap();

    let fetched = store.find_by_id(&task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
    assert_eq!(fetched.description(), "Details");
    assert_eq!(fetched.priority(), Priority::High);
    assert_eq!(fetched.assigned_roles(), &["dev".to_string()]);
}

#[tokio::test]
async fn task_save_persists_event_log() {
    let s = state();
    let task_store = MemoryTaskStore::new(s.clone());
    let event_query = MemoryEventQuery::new(s);
    let organization = org();
    let mut task = Task::new(
        organization.clone(),
        proj("proj"),
        Namespace::root(),
        "Write event".into(),
        "verify writer".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();

    task_store.save(&mut task).await.unwrap();

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
async fn task_list_sorted_by_priority() {
    let s = state();
    let store = MemoryTaskStore::new(s);

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
    store.save(&mut low).await.unwrap();

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
    store.save(&mut critical).await.unwrap();

    let page = store
        .list(TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
async fn message_save_and_find_unread() {
    let s = state();
    let store = MemoryMessageStore::new(s);

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
        vec![],
    )
    .unwrap();
    store.save(&mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let page = store
        .find_unread(
            &to,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].body(), "hello");
    assert_eq!(page.items[0].status(), MessageStatus::Pending);

    let msg_id = page.items[0].id();
    store.mark_read(&to, &[msg_id]).await.unwrap();

    let page = store
        .find_unread(
            &to,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(page.items.is_empty());
}

#[tokio::test]
async fn message_find_by_id_and_mark_read() {
    let s = state();
    let store = MemoryMessageStore::new(s);

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
        vec![],
    )
    .unwrap();
    store.save(&mut msg).await.unwrap();

    let mut fetched = store.find_by_id(&msg.id()).await.unwrap().unwrap();
    fetched.mark_read().unwrap();
    store.save(&mut fetched).await.unwrap();

    let read = store.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
async fn message_find_sent() {
    let s = state();
    let store = MemoryMessageStore::new(s);
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
        vec![],
    )
    .unwrap();
    store.save(&mut msg).await.unwrap();

    let sent = store
        .find_sent(
            &sender,
            &org(),
            &p,
            &Namespace::root(),
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(sent.items.len(), 1);
    assert_eq!(sent.items[0].body(), "hello");

    let sent_other = store
        .find_sent(
            &receiver,
            &org(),
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
    let s = state();
    let store = MemoryMessageStore::new(s);
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
        vec![],
    )
    .unwrap();
    store.save(&mut msg1).await.unwrap();

    let mut msg2 = msg1.reply(b.clone(), "second".into()).unwrap();
    store.save(&mut msg2).await.unwrap();

    let mut msg3 = msg2.reply(a.clone(), "third".into()).unwrap();
    store.save(&mut msg3).await.unwrap();

    let thread = store.find_thread(&msg3.id(), None).await.unwrap();
    assert_eq!(thread.len(), 3);
    assert_eq!(thread[0].body(), "first");
    assert_eq!(thread[1].body(), "second");
    assert_eq!(thread[2].body(), "third");

    let limited = store.find_thread(&msg3.id(), Some(2)).await.unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].body(), "second");
    assert_eq!(limited[1].body(), "third");
}

#[tokio::test]
async fn message_find_unread_includes_broadcast() {
    let s = state();
    let store = MemoryMessageStore::new(s);
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
        vec![],
    )
    .unwrap();
    store.save(&mut msg).await.unwrap();

    let pending = store
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 1);
    assert_eq!(pending.items[0].body(), "to all");

    let sender_pending = store
        .find_unread(
            &sender,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(sender_pending.items.is_empty());

    store.mark_read(&receiver, &[msg.id()]).await.unwrap();

    let after_read = store
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &p,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(after_read.items.is_empty());
}

#[tokio::test]
async fn task_list_filters_by_assigned_to() {
    let s = state();
    let store = MemoryTaskStore::new(s);
    let agent = AgentId::new();

    let mut task = Task::new(
        org(),
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
    store.save(&mut task).await.unwrap();

    let mut other = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "unassigned".into(),
        "".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    store.save(&mut other).await.unwrap();

    let assigned = store
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
    let s = state();
    let store = MemoryTaskStore::new(s);
    let mut task = Task::new(
        org(),
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
    store.save(&mut task).await.unwrap();

    let agent_id = "01234567-89ab-cdef-0123-456789abcdef"
        .parse::<AgentId>()
        .unwrap();
    let mut task = store.find_by_id(&task.id()).await.unwrap().unwrap();
    task.claim(agent_id.clone()).unwrap();
    task.start(&agent_id).unwrap();
    task.complete(Some("done".into())).unwrap();
    store.save(&mut task).await.unwrap();

    let mut fetched = store.find_by_id(&task.id()).await.unwrap().unwrap();
    fetched.archive(Some("completed".into())).unwrap();
    store.save(&mut fetched).await.unwrap();

    let archived = store.find_by_id(&task.id()).await.unwrap().unwrap();
    assert!(archived.is_archived());
    assert!(archived.archived_at().is_some());

    let filter = TaskFilter {
        org_id: Some(org()),
        project: Some(proj("test-project")),
        include_archived: Some(false),
        ..Default::default()
    };
    let page = store.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().all(|task| !task.is_archived()));

    let filter = TaskFilter {
        org_id: Some(org()),
        project: Some(proj("test-project")),
        include_archived: Some(true),
        ..Default::default()
    };
    let page = store.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().any(|task| task.is_archived()));

    let mut restored = store.find_by_id(&task.id()).await.unwrap().unwrap();
    restored.unarchive().unwrap();
    store.save(&mut restored).await.unwrap();
    let active = store.find_by_id(&task.id()).await.unwrap().unwrap();
    assert!(!active.is_archived());
}

#[tokio::test]
async fn knowledge_save_and_find() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        KnowledgePath::new("decisions/db").unwrap(),
        KnowledgeKind::Decision,
        "Database choice".into(),
        "We chose PostgreSQL".into(),
        vec!["infra".into()],
        HashMap::new(),
    )
    .unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    store.save(&mut entry).await.unwrap();

    let fetched = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert_eq!(fetched.title(), "Database choice");
    assert_eq!(fetched.version().as_u64(), 1);
}

#[tokio::test]
async fn knowledge_archive_and_unarchive() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
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
    store.save(&mut entry).await.unwrap();

    let mut fetched = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    fetched.archive(Some("test reason".into())).unwrap();
    store.save(&mut fetched).await.unwrap();

    let archived = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert!(archived.is_archived());
    assert!(archived.archived_at().is_some());

    let filter = KnowledgeFilter {
        org_id: Some(org()),
        project: Some(proj("test-project")),
        include_archived: Some(false),
        ..Default::default()
    };
    let page = store.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().all(|entry| !entry.is_archived()));

    let filter = KnowledgeFilter {
        org_id: Some(org()),
        project: Some(proj("test-project")),
        include_archived: Some(true),
        ..Default::default()
    };
    let page = store.list(filter, PageParams::default()).await.unwrap();
    assert!(page.items.iter().any(|entry| entry.is_archived()));

    let mut restored = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    restored.unarchive().unwrap();
    store.save(&mut restored).await.unwrap();
    let active = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert!(!active.is_archived());
}

#[tokio::test]
async fn knowledge_find_by_path_returns_archived() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
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
    store.save(&mut entry).await.unwrap();

    let mut fetched = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    fetched.archive(None).unwrap();
    store.save(&mut fetched).await.unwrap();

    let found = store
        .find_by_path(
            &org(),
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
async fn knowledge_optimistic_concurrency_rejects_stale_version() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        KnowledgePath::new("my-note").unwrap(),
        KnowledgeKind::Note,
        "v1 title".into(),
        "v1 content".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    store.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 1);

    entry
        .update("v2 title".into(), "v2 content".into())
        .unwrap();
    assert_eq!(entry.version().as_u64(), 2);
    store.save(&mut entry).await.unwrap();

    let mut stale = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert_eq!(stale.version().as_u64(), 2);

    entry
        .update("v3 title".into(), "v3 content".into())
        .unwrap();
    store.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    stale.update("stale update".into(), "stale".into()).unwrap();
    assert_eq!(stale.version().as_u64(), 3);
    let err = store.save(&mut stale).await.unwrap_err();
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
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        KnowledgePath::new("my-note").unwrap(),
        KnowledgeKind::Note,
        "v1".into(),
        "v1".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    store.save(&mut entry).await.unwrap();

    entry.update("v2".into(), "v2".into()).unwrap();
    store.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 2);

    entry.update("v3".into(), "v3".into()).unwrap();
    store.save(&mut entry).await.unwrap();
    assert_eq!(entry.version().as_u64(), 3);

    let fetched = store.find_by_id(&entry.id()).await.unwrap().unwrap();
    assert_eq!(fetched.version().as_u64(), 3);
    assert_eq!(fetched.title(), "v3");
}

#[tokio::test]
async fn edge_exists_by_pair_detects_duplicate() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();

    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-1".to_string(),
        ResourceKind::Knowledge,
        "know-1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    store.save(&mut edge).await.unwrap();

    assert!(
        store
            .exists_by_pair(
                &org(),
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
        !store
            .exists_by_pair(
                &org(),
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
async fn edge_with_source_persists_and_retrieves_source() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();

    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-1".into(),
        ResourceKind::Knowledge,
        "know-1".into(),
        RelationType::Produces,
        None,
    )
    .unwrap()
    .with_source(ResourceKind::Task, "task-1".into());

    store.save(&mut edge).await.unwrap();

    let fetched = store.find_by_id(&edge.id()).await.unwrap().unwrap();

    assert_eq!(fetched.source_kind(), Some(&ResourceKind::Task));
    assert_eq!(fetched.source_id(), Some("task-1"));
}

#[tokio::test]
async fn edge_list_by_org_returns_all_and_filters_by_rel_type() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();

    let mut e1 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    let mut e2 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t2".to_string(),
        ResourceKind::Task,
        "t3".to_string(),
        RelationType::Spawns,
        None,
    )
    .unwrap();
    store.save(&mut e1).await.unwrap();
    store.save(&mut e2).await.unwrap();

    let all = store
        .list_by_org(&org(), None, PageParams::default(), false, None)
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);

    let spawns_only = store
        .list_by_org(
            &org(),
            Some(&RelationType::Spawns),
            PageParams::default(),
            false,
            None,
        )
        .await
        .unwrap();
    assert_eq!(spawns_only.items.len(), 1);
    assert_eq!(spawns_only.items[0].from_id(), "t2");
}

#[tokio::test]
async fn delete_knowledge_cleans_up_associated_edges() {
    let s = state();
    let knowledge_store = MemoryKnowledgeStore::new(s.clone());
    let edge_store = MemoryEdgeStore::new(s);
    let o = org();

    let mut entry = Knowledge::new(
        o.clone(),
        Some(proj("myapp")),
        ns("/"),
        KnowledgePath::new("test-decision").unwrap(),
        KnowledgeKind::Decision,
        "Test".to_string(),
        "content".to_string(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut entry).await.unwrap();
    let kid = entry.id().to_string();

    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-1".to_string(),
        ResourceKind::Knowledge,
        kid.clone(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge).await.unwrap();

    let before = edge_store
        .list_by_org(&org(), None, PageParams::default(), false, None)
        .await
        .unwrap();
    assert_eq!(before.items.len(), 1);

    entry.mark_deleted().unwrap();
    knowledge_store.save(&mut entry).await.unwrap();
    knowledge_store.delete(&entry.id()).await.unwrap();
    edge_store
        .delete_all_for(&org(), &ResourceKind::Knowledge, &kid)
        .await
        .unwrap();

    let after = edge_store
        .list_by_org(&org(), None, PageParams::default(), false, None)
        .await
        .unwrap();
    assert_eq!(after.items.len(), 0);
}

#[tokio::test]
async fn split_task_creates_spawns_edges() {
    use orchy_application::{SplitTask, SplitTaskCommand, SubtaskInput};

    let s = state();
    let task_store = Arc::new(MemoryTaskStore::new(s.clone()));
    let edge_store = Arc::new(MemoryEdgeStore::new(s));
    let o = org();

    let mut parent = Task::new(
        o.clone(),
        proj("myapp"),
        ns("/"),
        "Parent task".to_string(),
        "desc".to_string(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    task_store.save(&mut parent).await.unwrap();
    let parent_id = parent.id().to_string();

    let cmd = SplitTaskCommand {
        task_id: parent_id.clone(),
        subtasks: vec![
            SubtaskInput {
                title: "Sub A".to_string(),
                description: "desc".to_string(),
                acceptance_criteria: None,
                priority: None,
                assigned_roles: None,
                depends_on: None,
            },
            SubtaskInput {
                title: "Sub B".to_string(),
                description: "desc".to_string(),
                acceptance_criteria: None,
                priority: None,
                assigned_roles: None,
                depends_on: None,
            },
        ],
        created_by: None,
    };

    let split = SplitTask::new(
        task_store.clone() as Arc<dyn orchy_core::task::TaskStore>,
        edge_store.clone() as Arc<dyn EdgeStore>,
    );
    split.execute(cmd).await.unwrap();

    let edges = edge_store
        .list_by_org(
            &org(),
            Some(&RelationType::Spawns),
            PageParams::default(),
            false,
            None,
        )
        .await
        .unwrap();
    assert_eq!(edges.items.len(), 2);
    assert!(edges.items.iter().all(|e| e.from_id() == parent_id));
}

#[tokio::test]
async fn delete_by_pair_removes_matching_edge() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();
    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".into(),
        ResourceKind::Task,
        "t2".into(),
        RelationType::DependsOn,
        None,
    )
    .unwrap();
    store.save(&mut edge).await.unwrap();

    store
        .delete_by_pair(
            &org(),
            &ResourceKind::Task,
            "t1",
            &ResourceKind::Task,
            "t2",
            &RelationType::DependsOn,
        )
        .await
        .unwrap();

    assert!(store.find_by_id(&edge.id()).await.unwrap().is_none());
}

#[tokio::test]
async fn split_task_creates_depends_on_edges_for_subtask_deps() {
    use orchy_application::{PostTask, PostTaskCommand, SplitTask, SplitTaskCommand, SubtaskInput};

    let s = state();
    let task_store = Arc::new(MemoryTaskStore::new(s.clone()));
    let edge_store = Arc::new(MemoryEdgeStore::new(s));

    let tasks: Arc<dyn orchy_core::task::TaskStore> = task_store.clone();
    let edges: Arc<dyn EdgeStore> = edge_store.clone();

    let post = PostTask::new(tasks.clone());
    let split = SplitTask::new(tasks.clone(), edges.clone());

    let dep = post
        .execute(PostTaskCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            title: "Dep".into(),
            description: "desc".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
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
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
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
                acceptance_criteria: None,
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
    let dep_edges = edge_store
        .find_from(
            &o,
            &ResourceKind::Task,
            &sub.id,
            &[RelationType::DependsOn],
            None,
        )
        .await
        .unwrap();

    assert_eq!(dep_edges.len(), 1);
    assert_eq!(dep_edges[0].to_id(), dep.id.as_str());
}

#[tokio::test]
async fn delete_by_pair_ignores_different_rel_type() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();
    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".into(),
        ResourceKind::Task,
        "t2".into(),
        RelationType::Spawns,
        None,
    )
    .unwrap();
    store.save(&mut edge).await.unwrap();

    store
        .delete_by_pair(
            &org(),
            &ResourceKind::Task,
            "t1",
            &ResourceKind::Task,
            "t2",
            &RelationType::DependsOn,
        )
        .await
        .unwrap();

    assert!(store.find_by_id(&edge.id()).await.unwrap().is_some());
}

#[tokio::test]
async fn knowledge_search_returns_score() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let mut entry = Knowledge::new(
        org(),
        Some(proj("test")),
        Namespace::root(),
        KnowledgePath::new("search-target").unwrap(),
        KnowledgeKind::Note,
        "PostgreSQL indexing".into(),
        "We use GIN indexes for full text search".into(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    store.save(&mut entry).await.unwrap();

    let results = store
        .search(&org(), "GIN indexes for full text", None, None, 20)
        .await
        .unwrap();

    assert!(!results.is_empty());
    let (_, score) = &results[0];
    assert!(score.is_some());
}

#[tokio::test]
async fn get_task_with_context_can_include_dependencies_and_linked_knowledge() {
    use orchy_application::{
        AddEdge, AddEdgeCommand, GetTaskWithContext, GetTaskWithContextCommand, PostTask,
        PostTaskCommand, WriteKnowledge, WriteKnowledgeCommand,
    };

    let s = state();
    let tasks: Arc<dyn orchy_core::task::TaskStore> = Arc::new(MemoryTaskStore::new(s.clone()));
    let edges: Arc<dyn EdgeStore> = Arc::new(MemoryEdgeStore::new(s.clone()));
    let knowledge: Arc<dyn orchy_core::knowledge::KnowledgeStore> =
        Arc::new(MemoryKnowledgeStore::new(s));

    let post_task = PostTask::new(tasks.clone());
    let add_edge = AddEdge::new(edges.clone());
    let write_knowledge = WriteKnowledge::new(knowledge.clone(), edges.clone(), None);
    let get_task = GetTaskWithContext::new(tasks, edges, knowledge);

    let dep = post_task
        .execute(PostTaskCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            title: "Dep".into(),
            description: "Dependency".into(),
            acceptance_criteria: None,
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    let task = post_task
        .execute(PostTaskCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            title: "Main".into(),
            description: "Main task".into(),
            acceptance_criteria: Some("Done when tests pass".into()),
            priority: None,
            assigned_roles: None,
            created_by: None,
        })
        .await
        .unwrap();

    add_edge
        .execute(AddEdgeCommand {
            org_id: "test-org".into(),
            from_kind: "task".into(),
            from_id: task.id.clone(),
            to_kind: "task".into(),
            to_id: dep.id.clone(),
            rel_type: "depends_on".into(),
            created_by: None,
            if_not_exists: false,
        })
        .await
        .unwrap();

    write_knowledge
        .execute(WriteKnowledgeCommand {
            org_id: "test-org".into(),
            project: "test".into(),
            namespace: None,
            path: "tasks/main/ctx".into(),
            kind: "context".into(),
            title: "Main context".into(),
            content: "Long context body for task implementation".into(),
            tags: Some(vec!["task-context".into()]),
            version: None,
            agent_id: None,
            metadata: None,
            metadata_remove: None,
            valid_from: None,
            valid_until: None,
            task_id: Some(task.id.clone()),
        })
        .await
        .unwrap();

    let ctx = get_task
        .execute(GetTaskWithContextCommand {
            task_id: task.id.clone(),
            org_id: "test-org".into(),
            include_dependencies: true,
            include_knowledge: true,
            knowledge_limit: 5,
            knowledge_kind: Some("context".into()),
            knowledge_tag: Some("task-context".into()),
            knowledge_content_limit: 8,
        })
        .await
        .unwrap();

    assert_eq!(ctx.dependencies.len(), 1);
    assert_eq!(ctx.dependencies[0].id, dep.id);
    assert_eq!(ctx.knowledge.len(), 1);
    assert!(ctx.knowledge[0].content.len() <= 8);
    assert_eq!(
        ctx.task.acceptance_criteria.as_deref(),
        Some("Done when tests pass")
    );
}

#[tokio::test]
async fn search_knowledge_task_proximity_boost() {
    use orchy_application::{SearchKnowledge, SearchKnowledgeCommand};

    let s = state();
    let knowledge_store = Arc::new(MemoryKnowledgeStore::new(s.clone()));
    let edge_store = Arc::new(MemoryEdgeStore::new(s));
    let o = org();

    let mut k = Knowledge::new(
        o.clone(),
        Some(proj("p")),
        Namespace::root(),
        KnowledgePath::new("auth-decision").unwrap(),
        KnowledgeKind::Decision,
        "Authentication Decision".to_string(),
        "We chose JWT tokens for authentication".to_string(),
        vec![],
        HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut k).await.unwrap();

    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-123".to_string(),
        ResourceKind::Knowledge,
        k.id().to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge).await.unwrap();

    let cmd_no_boost = SearchKnowledgeCommand {
        org_id: o.to_string(),
        query: "authentication".to_string(),
        namespace: None,
        kind: None,
        limit: Some(10),
        project: None,
        min_score: None,
        anchor_kind: None,
        anchor_id: None,
        task_id: None,
    };
    let results_no_boost = SearchKnowledge::new(
        knowledge_store.clone() as Arc<dyn orchy_core::knowledge::KnowledgeStore>,
        None,
        edge_store.clone() as Arc<dyn EdgeStore>,
    )
    .execute(cmd_no_boost)
    .await
    .unwrap();

    let cmd_with_boost = SearchKnowledgeCommand {
        org_id: o.to_string(),
        query: "authentication".to_string(),
        namespace: None,
        kind: None,
        limit: Some(10),
        project: None,
        min_score: None,
        anchor_kind: None,
        anchor_id: None,
        task_id: Some("task-123".to_string()),
    };
    let results_with_boost = SearchKnowledge::new(
        knowledge_store.clone() as Arc<dyn orchy_core::knowledge::KnowledgeStore>,
        None,
        edge_store.clone() as Arc<dyn EdgeStore>,
    )
    .execute(cmd_with_boost)
    .await
    .unwrap();

    assert!(!results_no_boost.is_empty());
    assert!(!results_with_boost.is_empty());

    let score_no_boost = results_no_boost[0].score.unwrap_or(0.0);
    let score_with_boost = results_with_boost[0].score.unwrap_or(0.0);
    assert!(
        score_with_boost > score_no_boost,
        "expected boosted score {} > unboosted {}",
        score_with_boost,
        score_no_boost
    );
}

#[tokio::test]
async fn edge_invalidate_hides_from_only_active_queries() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();
    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    store.save(&mut edge).await.unwrap();

    let found = store
        .find_from(&o, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    edge.invalidate().unwrap();
    store.save(&mut edge).await.unwrap();

    let found = store
        .find_from(&o, &ResourceKind::Task, "t1", &[], None)
        .await
        .unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn assemble_context_returns_linked_knowledge() {
    use orchy_application::{AssembleContext, AssembleContextCommand};

    let s = state();
    let knowledge_store: Arc<dyn orchy_core::knowledge::KnowledgeStore> =
        Arc::new(MemoryKnowledgeStore::new(s.clone()));
    let edge_store: Arc<dyn EdgeStore> = Arc::new(MemoryEdgeStore::new(s.clone()));
    let task_store: Arc<dyn orchy_core::task::TaskStore> = Arc::new(MemoryTaskStore::new(s));
    let o = org();
    let p = proj("p");

    let mut decision = Knowledge::new(
        o.clone(),
        Some(p.clone()),
        Namespace::root(),
        KnowledgePath::new("auth-decision").unwrap(),
        KnowledgeKind::Decision,
        "Auth Decision".to_string(),
        "We chose JWT for auth".to_string(),
        vec![],
        std::collections::HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut decision).await.unwrap();

    let mut note = Knowledge::new(
        o.clone(),
        Some(p.clone()),
        Namespace::root(),
        KnowledgePath::new("recent-note").unwrap(),
        KnowledgeKind::Note,
        "Recent Note".to_string(),
        "Found a bug in login flow".to_string(),
        vec![],
        std::collections::HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut note).await.unwrap();

    let mut edge1 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-abc".to_string(),
        ResourceKind::Knowledge,
        decision.id().to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge1).await.unwrap();

    let mut edge2 = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-abc".to_string(),
        ResourceKind::Knowledge,
        note.id().to_string(),
        RelationType::RelatedTo,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge2).await.unwrap();

    let svc = AssembleContext::new(edge_store, task_store, knowledge_store);
    let resp = svc
        .execute(AssembleContextCommand {
            org_id: o.to_string(),
            kind: "task".to_string(),
            id: "task-abc".to_string(),
            max_tokens: None,
        })
        .await
        .unwrap();

    assert!(!resp.core_facts.is_empty(), "expected core_facts");
    assert!(resp.core_facts.iter().any(|k| k.path == "auth-decision"));

    let all_paths: Vec<_> = resp
        .recent_changes
        .iter()
        .chain(resp.relevant_decisions.iter())
        .map(|k| k.path.clone())
        .collect();
    assert!(
        all_paths.contains(&"recent-note".to_string()),
        "expected recent-note in output"
    );
}

#[tokio::test]
async fn edge_as_of_returns_snapshot_at_past_timestamp() {
    let s = state();
    let store = MemoryEdgeStore::new(s);
    let o = org();
    let mut edge = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "t1".to_string(),
        ResourceKind::Knowledge,
        "k1".to_string(),
        RelationType::Produces,
        None,
    )
    .unwrap();
    store.save(&mut edge).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let midpoint = edge.created_at() + Duration::milliseconds(5);
    edge.invalidate().unwrap();
    store.save(&mut edge).await.unwrap();

    let found = store
        .find_from(&o, &ResourceKind::Task, "t1", &[], Some(midpoint))
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    let after_invalidation = edge.valid_until().unwrap() + Duration::seconds(1);
    let found = store
        .find_from(
            &org(),
            &ResourceKind::Task,
            "t1",
            &[],
            Some(after_invalidation),
        )
        .await
        .unwrap();
    assert!(found.is_empty());

    let before_creation = edge.created_at() - Duration::seconds(1);
    let found = store
        .find_from(
            &org(),
            &ResourceKind::Task,
            "t1",
            &[],
            Some(before_creation),
        )
        .await
        .unwrap();
    assert!(found.is_empty());
}

#[tokio::test]
async fn assemble_context_surfaces_decision_above_log() {
    use orchy_application::{AssembleContext, AssembleContextCommand};

    let s = state();
    let knowledge_store: Arc<dyn orchy_core::knowledge::KnowledgeStore> =
        Arc::new(MemoryKnowledgeStore::new(s.clone()));
    let edge_store: Arc<dyn EdgeStore> = Arc::new(MemoryEdgeStore::new(s.clone()));
    let task_store: Arc<dyn orchy_core::task::TaskStore> = Arc::new(MemoryTaskStore::new(s));
    let o = org();
    let p = proj("p");

    let mut decision = Knowledge::new(
        o.clone(),
        Some(p.clone()),
        Namespace::root(),
        KnowledgePath::new("important-decision").unwrap(),
        KnowledgeKind::Decision,
        "Important Decision".to_string(),
        "We chose Rust for performance".to_string(),
        vec![],
        std::collections::HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut decision).await.unwrap();

    let mut log = Knowledge::new(
        o.clone(),
        Some(p.clone()),
        Namespace::root(),
        KnowledgePath::new("activity-log").unwrap(),
        KnowledgeKind::Log,
        "Activity Log".to_string(),
        "Ran some tests".to_string(),
        vec![],
        std::collections::HashMap::new(),
    )
    .unwrap();
    knowledge_store.save(&mut log).await.unwrap();

    let mut edge_d = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-x".to_string(),
        ResourceKind::Knowledge,
        decision.id().to_string(),
        RelationType::RelatedTo,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge_d).await.unwrap();

    let mut edge_l = Edge::new(
        o.clone(),
        ResourceKind::Task,
        "task-x".to_string(),
        ResourceKind::Knowledge,
        log.id().to_string(),
        RelationType::RelatedTo,
        None,
    )
    .unwrap();
    edge_store.save(&mut edge_l).await.unwrap();

    let svc = AssembleContext::new(edge_store, task_store, knowledge_store);
    let resp = svc
        .execute(AssembleContextCommand {
            org_id: o.to_string(),
            kind: "task".to_string(),
            id: "task-x".to_string(),
            max_tokens: None,
        })
        .await
        .unwrap();

    assert!(
        !resp.relevant_decisions.is_empty(),
        "expected decision in relevant_decisions"
    );
    assert!(
        resp.relevant_decisions
            .iter()
            .any(|k| k.path == "important-decision")
    );

    assert!(resp.recent_changes.iter().any(|k| k.path == "activity-log"));
}

#[tokio::test]
async fn agent_resume_preserves_identity() {
    let s = state();
    let store = MemoryAgentStore::new(s);
    let alias = Alias::new("coder-1").unwrap();

    let mut agent = Agent::register(
        org(),
        proj("myapp"),
        Namespace::root(),
        alias.clone(),
        vec!["dev".into()],
        "first session".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    store.save(&mut agent).await.unwrap();
    let original_id = agent.id().clone();

    let found = store
        .find_by_alias(&org(), &proj("myapp"), &alias)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id(), &original_id);

    let mut resumed = found;
    resumed
        .resume(
            Namespace::root(),
            vec!["dev".into(), "reviewer".into()],
            "second session".into(),
        )
        .unwrap();
    store.save(&mut resumed).await.unwrap();

    assert_eq!(resumed.id(), &original_id);
    assert_eq!(resumed.roles(), &["dev", "reviewer"]);
    assert_eq!(resumed.description(), "second session");
}

#[tokio::test]
async fn agent_status_derived_from_last_seen() {
    let s = state();
    let store = MemoryAgentStore::new(s);
    let mut agent = Agent::register(
        org(),
        proj("myapp"),
        Namespace::root(),
        Alias::new("worker").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    store.save(&mut agent).await.unwrap();
    assert_eq!(agent.derived_status(30, 300), "active");

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = store.find_timed_out(0).await.unwrap();
    assert!(
        timed_out.iter().any(|a| a.id() == agent.id()),
        "agent should appear in timed-out list when timeout=0"
    );

    agent.heartbeat().unwrap();
    store.save(&mut agent).await.unwrap();

    let fetched = store.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.derived_status(30, 300), "active");
}

#[tokio::test]
async fn task_claim_start_touch_complete_lifecycle() {
    let s = state();
    let store = MemoryTaskStore::new(s);
    let agent_id = AgentId::new();

    let mut task = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "lifecycle test".into(),
        "full lifecycle".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    store.save(&mut task).await.unwrap();
    assert_eq!(task.status(), TaskStatus::Pending);

    task.claim(agent_id.clone()).unwrap();
    store.save(&mut task).await.unwrap();
    assert_eq!(task.status(), TaskStatus::Claimed);

    task.start(&agent_id).unwrap();
    store.save(&mut task).await.unwrap();
    assert_eq!(task.status(), TaskStatus::InProgress);

    task.touch();
    store.save(&mut task).await.unwrap();

    task.complete(Some("done".into())).unwrap();
    store.save(&mut task).await.unwrap();
    assert_eq!(task.status(), TaskStatus::Completed);
    assert_eq!(task.result_summary(), Some("done"));
}

#[tokio::test]
async fn task_without_staleness_config_is_never_stale() {
    let s = state();
    let store = MemoryTaskStore::new(s);
    let agent_id = AgentId::new();

    let mut task = Task::new(
        org(),
        proj("proj"),
        Namespace::root(),
        "no-stale test".into(),
        "".into(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    store.save(&mut task).await.unwrap();

    task.claim(agent_id).unwrap();
    store.save(&mut task).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let fetched = store.find_by_id(&task.id()).await.unwrap().unwrap();
    assert!(
        !fetched.is_stale(),
        "task without stale_after_secs should never be stale"
    );
    assert_eq!(fetched.stale_after_secs(), None);
}

#[tokio::test]
async fn knowledge_path_roundtrip_through_store() {
    let s = state();
    let store = MemoryKnowledgeStore::new(s);
    let path = KnowledgePath::new("auth/jwt-strategy").unwrap();

    let mut entry = Knowledge::new(
        org(),
        Some(proj("myapp")),
        Namespace::root(),
        path.clone(),
        KnowledgeKind::Decision,
        "JWT Strategy".into(),
        "We chose RS256".into(),
        vec!["auth".into()],
        HashMap::new(),
    )
    .unwrap();
    store.save(&mut entry).await.unwrap();

    let fetched = store
        .find_by_path(&org(), Some(&proj("myapp")), &Namespace::root(), &path)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(fetched.path(), &path);
    assert_eq!(fetched.path().as_str(), "auth/jwt-strategy");
    assert_eq!(fetched.kind(), KnowledgeKind::Decision);
    assert_eq!(fetched.title(), "JWT Strategy");
    assert_eq!(fetched.content(), "We chose RS256");
}
