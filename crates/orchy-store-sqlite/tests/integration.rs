use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::Result as OrchyResult;
use orchy_core::knowledge::service::KnowledgeService;
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore, WriteKnowledge};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_store_sqlite::SqliteBackend;

struct NoopEmbeddings;

impl EmbeddingsProvider for NoopEmbeddings {
    async fn embed(&self, _text: &str) -> OrchyResult<Vec<f32>> {
        Ok(vec![])
    }

    async fn embed_batch(&self, _texts: &[&str]) -> OrchyResult<Vec<Vec<f32>>> {
        Ok(vec![])
    }

    fn model(&self) -> &str {
        "noop"
    }

    fn dimensions(&self) -> u32 {
        0
    }
}

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
async fn task_save_overwrites_existing() {
    let store = backend();
    let mut task = Task::new(
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

    TaskStore::save(&store, &mut task).await.unwrap();

    let mut updated = Task::restore(RestoreTask {
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
        tags: vec![],
        result_summary: Some("done".into()),
        notes: vec![],
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
async fn task_dependency_stored() {
    let store = backend();

    let mut dep = Task::new(
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
    TaskStore::save(&store, &mut dep).await.unwrap();

    let mut task = Task::new(
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
    TaskStore::save(&store, &mut task).await.unwrap();

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

#[tokio::test]
async fn knowledge_search_fts_finds_content() {
    let store = backend();
    let mut entry = Knowledge::new(
        proj("p"),
        Namespace::root(),
        "auth/jwt".into(),
        KnowledgeKind::Note,
        "JWT notes".into(),
        "Use RS256 for asymmetric cryptography verification.".into(),
        vec![],
        None,
        HashMap::new(),
    )
    .unwrap();
    KnowledgeStore::save(&store, &mut entry).await.unwrap();

    let hits = KnowledgeStore::search(&store, "cryptography", None, None, 10)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path(), "auth/jwt");
}

#[tokio::test]
async fn knowledge_service_metadata_merge_and_remove() {
    let store = Arc::new(backend());
    let svc: KnowledgeService<SqliteBackend, NoopEmbeddings> =
        KnowledgeService::new(store, None::<Arc<NoopEmbeddings>>);

    let mut md = HashMap::new();
    md.insert("a".into(), "1".into());
    svc.write(WriteKnowledge {
        project: proj("p"),
        namespace: Namespace::root(),
        path: "meta-test".into(),
        kind: KnowledgeKind::Note,
        title: "t".into(),
        content: "body".into(),
        tags: vec![],
        expected_version: None,
        agent_id: None,
        metadata: md,
        metadata_remove: vec![],
    })
    .await
    .unwrap();

    let mut md2 = HashMap::new();
    md2.insert("b".into(), "2".into());
    let entry = svc
        .write(WriteKnowledge {
            project: proj("p"),
            namespace: Namespace::root(),
            path: "meta-test".into(),
            kind: KnowledgeKind::Note,
            title: "t".into(),
            content: "body2".into(),
            tags: vec![],
            expected_version: None,
            agent_id: None,
            metadata: md2,
            metadata_remove: vec!["a".into()],
        })
        .await
        .unwrap();

    assert_eq!(entry.metadata().get("a"), None);
    assert_eq!(entry.metadata().get("b").map(String::as_str), Some("2"));

    let entry = svc
        .patch_metadata(
            &proj("p"),
            &Namespace::root(),
            "meta-test",
            HashMap::from([("c".into(), "3".into())]),
            vec!["b".into()],
            None,
        )
        .await
        .unwrap();
    assert_eq!(entry.metadata().get("b"), None);
    assert_eq!(entry.metadata().get("c").map(String::as_str), Some("3"));
}
