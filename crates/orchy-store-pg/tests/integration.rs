use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use orchy_core::agent::{Agent, AgentStore, Alias};
use orchy_core::message::{Message, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId as CoreOrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{Priority, Task, TaskFilter, TaskStatus, TaskStore};
use orchy_events::io::{Reader, Writer};
use orchy_events::{ConsumerGroupId, Event, OrganizationId, StartFrom};
use orchy_store_pg::{PgDatabase, PgEventWriter, PgReader, PgReaderConfig, *};

async fn start_postgres() -> (ContainerAsync<GenericImage>, sqlx::PgPool) {
    let container = GenericImage::new("pgvector/pgvector", "0.8.2-pg17")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_USER", "orchy")
        .with_env_var("POSTGRES_PASSWORD", "orchy")
        .with_env_var("POSTGRES_DB", "orchy")
        .start()
        .await
        .expect("postgres start");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://orchy:orchy@127.0.0.1:{port}/orchy");
    let db = PgDatabase::new(&url, None).await.unwrap();
    db.run_migrations(&PgDatabase::migrations_dir())
        .await
        .unwrap();
    let pool = db.pool();
    (container, pool)
}

fn proj(s: &str) -> ProjectId {
    ProjectId::try_from(s).unwrap()
}

fn org() -> CoreOrganizationId {
    CoreOrganizationId::new("default").unwrap()
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn agent_save_and_find() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p);
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
    agents.save(&mut agent).await.unwrap();

    assert_eq!(agent.derived_status(30, 300), "active");
    assert_eq!(agent.roles(), &["coder".to_string()]);

    let fetched = agents.find_by_id(agent.id()).await.unwrap().unwrap();
    assert_eq!(fetched.id(), agent.id());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn agent_save_updates_existing() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p);
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
    agents.save(&mut agent).await.unwrap();

    let before = agent.last_seen();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    agent.heartbeat().unwrap();
    agents.save(&mut agent).await.unwrap();

    let updated = agents.find_by_id(agent.id()).await.unwrap().unwrap();
    assert!(updated.last_seen() > before);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn agent_save_and_fetch_roundtrip() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p);
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
    agents.save(&mut agent).await.unwrap();
    agents.save(&mut agent).await.unwrap();
    let _fetched = agents.find_by_id(agent.id()).await.unwrap().unwrap();
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn agent_find_timed_out() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p);
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
    agents.save(&mut agent).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    let timed_out = agents.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));

    agents.save(&mut agent).await.unwrap();
    let timed_out = agents.find_timed_out(0).await.unwrap();
    assert!(timed_out.iter().any(|a| a.id() == agent.id()));
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn task_save_and_get() {
    let (_container, p) = start_postgres().await;
    let tasks = PgTaskStore::new(p);

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
    tasks.save(&mut task).await.unwrap();

    let fetched = tasks.find_by_id(&task.id()).await.unwrap().unwrap();
    assert_eq!(fetched.status(), TaskStatus::Pending);
    assert_eq!(fetched.title(), "Do thing");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn task_save_persists_event_log() {
    let (_container, p) = start_postgres().await;
    let tasks = PgTaskStore::new(p.clone());
    let organization = org();
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
    tasks.save(&mut task).await.unwrap();

    let reader = PgReader::new(
        p,
        PgReaderConfig {
            organization: OrganizationId::new(organization.as_str()).unwrap(),
            consumer_group_id: None,
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(10),
            batch_size: 10,
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader.read().await.unwrap();
    let mut events = Vec::new();
    while let Some(msg) = stream.next().await {
        let msg = msg.unwrap();
        events.push(msg.into_event());
    }
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].topic().as_str(), "task.created");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn task_list_sorted_by_priority() {
    let (_container, p) = start_postgres().await;
    let tasks = PgTaskStore::new(p);

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
    tasks.save(&mut low).await.unwrap();

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
    tasks.save(&mut critical).await.unwrap();

    let page = tasks
        .list(TaskFilter::default(), PageParams::unbounded())
        .await
        .unwrap();
    assert_eq!(page.items[0].title(), "critical");
    assert_eq!(page.items[1].title(), "low");
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn message_save_and_find_unread() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("sender-agent").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("receiver-agent").unwrap(),
        vec![],
        "receiver".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut to_agent).await.unwrap();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        from_agent.id().clone(),
        MessageTarget::Agent(to_agent.id().clone()),
        "hello".into(),
        None,
        vec![],
    )
    .unwrap();
    messages.save(&mut msg).await.unwrap();
    assert_eq!(msg.status(), MessageStatus::Pending);

    let pr = proj("test-project");
    let unread = messages
        .find_unread(
            to_agent.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(unread.items.len(), 1);
    assert_eq!(unread.items[0].body(), "hello");
    assert_eq!(unread.items[0].status(), MessageStatus::Pending);

    let msg_id = unread.items[0].id();
    messages.mark_read(to_agent.id(), &[msg_id]).await.unwrap();

    let after = messages
        .find_unread(
            to_agent.id(),
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(after.items.is_empty());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn message_find_by_id_and_mark_read() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut from_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("from-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut from_agent).await.unwrap();

    let mut to_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("to-agent").unwrap(),
        vec![],
        "".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut to_agent).await.unwrap();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        from_agent.id().clone(),
        MessageTarget::Agent(to_agent.id().clone()),
        "hi".into(),
        None,
        vec![],
    )
    .unwrap();
    messages.save(&mut msg).await.unwrap();

    let mut fetched = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    fetched.mark_read().unwrap();
    messages.save(&mut fetched).await.unwrap();

    let read = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(read.status(), MessageStatus::Read);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn message_find_by_id_preserves_claim_state() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);

    let mut sender_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("claim-sender").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut sender_agent).await.unwrap();

    let mut claimer_agent = Agent::register(
        org(),
        proj("test-project"),
        Namespace::root(),
        Alias::new("claim-claimer").unwrap(),
        vec![],
        "claimer".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut claimer_agent).await.unwrap();
    let claimer = claimer_agent.id().clone();

    let mut msg = Message::new(
        org(),
        proj("test-project"),
        Namespace::root(),
        sender_agent.id().clone(),
        MessageTarget::Broadcast,
        "claimable".into(),
        None,
        vec![],
    )
    .unwrap();
    msg.claim(claimer.clone()).unwrap();
    messages.save(&mut msg).await.unwrap();

    let fetched = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert_eq!(fetched.claimed_by(), Some(&claimer));

    let mut fetched = fetched;
    fetched.unclaim(&claimer).unwrap();
    messages.save(&mut fetched).await.unwrap();

    let unclaimed = messages.find_by_id(&msg.id()).await.unwrap().unwrap();
    assert!(unclaimed.claimed_by().is_none());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn message_find_unread_includes_broadcast_until_agent_reads_it() {
    let (_container, p) = start_postgres().await;
    let agents = PgAgentStore::new(p.clone());
    let messages = PgMessageStore::new(p);
    let pr = proj("proj");

    let mut sender_agent = Agent::register(
        org(),
        pr.clone(),
        Namespace::root(),
        Alias::new("bcast-sender").unwrap(),
        vec![],
        "sender".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut sender_agent).await.unwrap();
    let sender = sender_agent.id().clone();

    let mut receiver_agent = Agent::register(
        org(),
        pr.clone(),
        Namespace::root(),
        Alias::new("bcast-receiver").unwrap(),
        vec![],
        "receiver".into(),
        None,
        HashMap::new(),
        None,
    )
    .unwrap();
    agents.save(&mut receiver_agent).await.unwrap();
    let receiver = receiver_agent.id().clone();

    let mut msg = Message::new(
        org(),
        pr.clone(),
        Namespace::root(),
        sender.clone(),
        MessageTarget::Broadcast,
        "to all".into(),
        None,
        vec![],
    )
    .unwrap();
    messages.save(&mut msg).await.unwrap();

    let pending = messages
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert_eq!(pending.items.len(), 1);

    let sender_pending = messages
        .find_unread(
            &sender,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(sender_pending.items.is_empty());

    messages.mark_read(&receiver, &[msg.id()]).await.unwrap();

    let after_read = messages
        .find_unread(
            &receiver,
            &[],
            &Namespace::root(),
            None,
            &org(),
            &pr,
            PageParams::unbounded(),
        )
        .await
        .unwrap();
    assert!(after_read.items.is_empty());
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn pg_reader_streaming_yields_events_in_order() {
    let (_container, p) = start_postgres().await;
    let writer = PgEventWriter::new(p.clone());
    let org_events = OrganizationId::new("orgx").unwrap();
    for i in 0..3 {
        let e = Event::create(
            "orgx",
            "/x",
            "thing.happened",
            format!("k{i}"),
            orchy_events::Payload::from_string(format!("v{i}")),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    let reader = PgReader::new(
        p,
        PgReaderConfig {
            organization: org_events,
            consumer_group_id: Some(ConsumerGroupId::new("test-group").unwrap()),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: Some(3),
            batch_size: 10,
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader.read().await.unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = stream.next().await {
        let msg = msg.unwrap();
        msg.ack().await.unwrap();
        keys.push(msg.event().key().as_str().to_string());
    }
    assert_eq!(keys, vec!["k0", "k1", "k2"]);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn pg_reader_bounded_terminates_after_limit() {
    let (_container, p) = start_postgres().await;
    let writer = PgEventWriter::new(p.clone());
    let org_events = OrganizationId::new("orgy").unwrap();
    for i in 0..5 {
        let e = Event::create(
            "orgy",
            "/x",
            "thing.happened",
            format!("k{i}"),
            orchy_events::Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    let reader = PgReader::new(
        p,
        PgReaderConfig {
            organization: org_events,
            consumer_group_id: None,
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
            batch_size: 10,
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader.read().await.unwrap();
    let mut count = 0;
    while let Some(_msg) = stream.next().await {
        count += 1;
    }
    assert_eq!(count, 3);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn pg_reader_resumes_from_offset() {
    let (_container, p) = start_postgres().await;
    let writer = PgEventWriter::new(p.clone());
    let org_events = OrganizationId::new("orgz").unwrap();
    for i in 0..4 {
        let e = Event::create(
            "orgz",
            "/x",
            "thing.happened",
            format!("k{i}"),
            orchy_events::Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    let group = ConsumerGroupId::new("resume-group").unwrap();

    {
        let reader = PgReader::new(
            p.clone(),
            PgReaderConfig {
                organization: org_events.clone(),
                consumer_group_id: Some(group.clone()),
                start_from: StartFrom::Earliest,
                topics: None,
                namespace_prefix: None,
                end_at: None,
                limit: Some(2),
                batch_size: 10,
                poll_interval: Duration::from_millis(50),
            },
        );
        let mut stream = reader.read().await.unwrap();
        for _ in 0..2 {
            let msg = stream.next().await.unwrap().unwrap();
            msg.ack().await.unwrap();
        }
    }

    let reader2 = PgReader::new(
        p,
        PgReaderConfig {
            organization: org_events,
            consumer_group_id: Some(group),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: Some(2),
            batch_size: 10,
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader2.read().await.unwrap();
    let msg = stream.next().await.unwrap().unwrap();
    assert_eq!(msg.event().key().as_str(), "k2");
}
