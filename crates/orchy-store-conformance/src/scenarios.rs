use std::collections::HashMap;

use orchy_core::agent::AgentId;
use orchy_core::error::Error;
use orchy_core::graph::{Edge, RelationType};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgePath};
use orchy_core::message::{Message, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
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
        title.to_string(),
        "description".to_string(),
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
        title.to_string(),
        "content".to_string(),
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
    let mut t1 = build_task("auth-task", vec!["auth".to_string()]);
    bundle.tasks.save(&mut t1).await.unwrap();

    let mut t2 = build_task("authorization-task", vec!["authorization".to_string()]);
    bundle.tasks.save(&mut t2).await.unwrap();

    let page = bundle
        .tasks
        .list(
            TaskFilter {
                tag: Some("auth".to_string()),
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
        .update("Database choice".to_string(), "v2 content".to_string())
        .unwrap();
    bundle.knowledge.save(&mut entry).await.unwrap();

    stale_copy
        .update("Database choice".to_string(), "stale content".to_string())
        .unwrap();
    let result = bundle.knowledge.save(&mut stale_copy).await;

    assert!(
        matches!(result, Err(Error::VersionMismatch { .. })),
        "concurrent save must return VersionMismatch, got: {result:?}"
    );
}

pub async fn message_claim_visibility(bundle: &Bundle) {
    let sender = AgentId::new();
    let agent_a = AgentId::new();
    let agent_b = AgentId::new();

    let mut msg = Message::new(
        org(),
        project(),
        Namespace::root(),
        sender.clone(),
        MessageTarget::Broadcast,
        "hello everyone".to_string(),
        None,
        vec![],
    )
    .unwrap();
    msg.claim(agent_a.clone()).unwrap();
    bundle.messages.save(&mut msg).await.unwrap();

    let unread_b = bundle
        .messages
        .find_unread(
            &agent_b,
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
            &agent_a,
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
        "task-a".to_string(),
        ResourceKind::Task,
        "task-b".to_string(),
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
