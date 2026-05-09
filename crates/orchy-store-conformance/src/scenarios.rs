use std::collections::HashMap;

use orchy_core::agent::{Agent, Alias};
use orchy_core::error::Error;
use orchy_core::graph::{Edge, RelationType};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgePath};
use orchy_core::message::{Message, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_lock::ResourceLock;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus};

use crate::Bundle;

fn org() -> OrganizationId {
    OrganizationId::new("test-org").unwrap()
}

fn project() -> ProjectId {
    ProjectId::try_from("test-project").unwrap()
}

fn build_task(title: &str, tags: Vec<String>) -> Task {
    let mut t = Task::new(
        org(),
        project(),
        Namespace::root(),
        title.to_owned(),
        "description".to_owned(),
        None,
        Priority::Normal,
        vec![],
        None,
        false,
    )
    .unwrap();
    for tag in tags {
        t.add_tag(tag).unwrap();
    }
    t
}

fn build_knowledge(path: &str, title: &str) -> Knowledge {
    Knowledge::new(
        org(),
        Some(project()),
        Namespace::root(),
        KnowledgePath::new(path).unwrap(),
        KnowledgeKind::Decision,
        title.to_owned(),
        "content".to_owned(),
        vec![],
        HashMap::new(),
    )
    .unwrap()
}

pub async fn task_save_then_find_returns_same(bundle: &Bundle) {
    let mut t = build_task("conformance-task", vec![]);
    bundle.tasks.save(&mut t).await.unwrap();

    let found = bundle.tasks.find_by_id(&t.id()).await.unwrap().unwrap();
    assert_eq!(found.title(), "conformance-task");
    assert_eq!(found.status(), TaskStatus::Pending);
    assert_eq!(found.id(), t.id());
}

pub async fn task_filter_tag_does_not_match_substring(bundle: &Bundle) {
    let mut t1 = build_task("auth-task", vec!["auth".to_owned()]);
    bundle.tasks.save(&mut t1).await.unwrap();

    let mut t2 = build_task("authorization-task", vec!["authorization".to_owned()]);
    bundle.tasks.save(&mut t2).await.unwrap();

    let page = bundle
        .tasks
        .list(
            TaskFilter {
                tag: Some("auth".to_owned()),
                org_id: Some(org()),
                project: Some(project()),
                ..Default::default()
            },
            PageParams::unbounded(),
        )
        .await
        .unwrap();

    assert_eq!(
        page.items.len(),
        1,
        "tag filter must be exact, not substring: got {:?}",
        page.items.iter().map(|t| t.title()).collect::<Vec<_>>()
    );
    assert_eq!(page.items[0].id(), t1.id());
}

pub async fn knowledge_optimistic_concurrency(bundle: &Bundle) {
    let mut entry = build_knowledge("decisions/db", "Database choice");
    bundle.knowledge.save(&mut entry).await.unwrap();

    let mut stale_copy = entry.clone();

    entry
        .update("Database choice".to_owned(), "v2 content".to_owned())
        .unwrap();
    bundle.knowledge.save(&mut entry).await.unwrap();

    stale_copy
        .update("Database choice".to_owned(), "stale content".to_owned())
        .unwrap();
    let result = bundle.knowledge.save(&mut stale_copy).await;

    assert!(
        matches!(result, Err(Error::VersionMismatch { .. })),
        "concurrent save must return VersionMismatch, got: {result:?}"
    );
}

pub async fn message_claim_visibility(bundle: &Bundle) {
    let mut sender = Agent::register(
        org(),
        project(),
        Namespace::root(),
        Alias::new("msg-sender").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    bundle.agents.save(&mut sender).await.unwrap();

    let mut agent_a = Agent::register(
        org(),
        project(),
        Namespace::root(),
        Alias::new("msg-agent-a").unwrap(),
        vec![],
        "agent a".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    bundle.agents.save(&mut agent_a).await.unwrap();

    let mut agent_b = Agent::register(
        org(),
        project(),
        Namespace::root(),
        Alias::new("msg-agent-b").unwrap(),
        vec![],
        "agent b".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    bundle.agents.save(&mut agent_b).await.unwrap();

    let mut msg = Message::new(
        org(),
        project(),
        Namespace::root(),
        sender.id().clone(),
        MessageTarget::Broadcast,
        "hello everyone".to_owned(),
        None,
        vec![],
    )
    .unwrap();
    msg.claim(agent_a.id().clone()).unwrap();
    bundle.messages.save(&mut msg).await.unwrap();

    let unread_b = bundle
        .messages
        .find_unread(
            agent_b.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &project(),
            PageParams::unbounded(),
        )
        .await
        .unwrap();

    assert!(
        unread_b.items.is_empty(),
        "message claimed by agent_a must not appear in agent_b inbox"
    );

    let unread_a = bundle
        .messages
        .find_unread(
            agent_a.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &project(),
            PageParams::unbounded(),
        )
        .await
        .unwrap();

    assert_eq!(
        unread_a.items.len(),
        1,
        "message claimed by agent_a must appear in agent_a inbox"
    );
}

pub async fn edge_alias_blocks_normalizes_to_depends_on(bundle: &Bundle) {
    let rel: RelationType = "blocks".parse().unwrap();
    assert_eq!(
        rel,
        RelationType::DependsOn,
        "'blocks' alias must parse to DependsOn"
    );

    let mut edge = Edge::new(
        org(),
        ResourceKind::Task,
        "task-a".to_owned(),
        ResourceKind::Task,
        "task-b".to_owned(),
        rel,
        None,
    )
    .unwrap();
    bundle.edges.save(&mut edge).await.unwrap();

    let found = bundle.edges.find_by_id(&edge.id()).await.unwrap().unwrap();
    assert_eq!(
        *found.rel_type(),
        RelationType::DependsOn,
        "stored edge must preserve DependsOn after round-trip"
    );
}

pub async fn agent_save_then_find_returns_same(bundle: &Bundle) {
    let mut agent = Agent::register(
        org(),
        project(),
        Namespace::root(),
        Alias::new("conformance-agent").unwrap(),
        vec!["tester".to_owned()],
        "conformance agent".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    bundle.agents.save(&mut agent).await.unwrap();

    let found = bundle.agents.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(found.alias(), agent.alias());
    assert_eq!(found.id(), agent.id());
    assert_eq!(found.roles(), agent.roles());
}

pub async fn pagination_respects_limit(bundle: &Bundle) {
    let mut t1 = build_task("task-a", vec![]);
    bundle.tasks.save(&mut t1).await.unwrap();
    let mut t2 = build_task("task-b", vec![]);
    bundle.tasks.save(&mut t2).await.unwrap();
    let mut t3 = build_task("task-c", vec![]);
    bundle.tasks.save(&mut t3).await.unwrap();

    let page = bundle
        .tasks
        .list(
            TaskFilter {
                org_id: Some(org()),
                project: Some(project()),
                ..Default::default()
            },
            PageParams::new(None, Some(2)),
        )
        .await
        .unwrap();

    assert_eq!(page.items.len(), 2, "pagination limit must be respected");
}

pub async fn lock_acquire_and_release(bundle: &Bundle) {
    let mut agent = Agent::register(
        org(),
        project(),
        Namespace::root(),
        Alias::new("lock-agent").unwrap(),
        vec![],
        "lock holder".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    bundle.agents.save(&mut agent).await.unwrap();

    let mut lock = ResourceLock::acquire(
        org(),
        project(),
        Namespace::root(),
        "test-resource".to_owned(),
        agent.id().clone(),
        3600,
    )
    .unwrap();
    bundle.locks.save(&mut lock).await.unwrap();

    let found = bundle
        .locks
        .find(&org(), &project(), &Namespace::root(), "test-resource")
        .await
        .unwrap();
    assert!(found.is_some(), "lock must exist after acquire");

    bundle
        .locks
        .delete(&org(), &project(), &Namespace::root(), "test-resource")
        .await
        .unwrap();

    let gone = bundle
        .locks
        .find(&org(), &project(), &Namespace::root(), "test-resource")
        .await
        .unwrap();
    assert!(gone.is_none(), "lock must be gone after release");
}
