use std::sync::Arc;

use orchy_application::claim_task::ClaimTaskCommand;
use orchy_application::complete_task::CompleteTaskCommand;
use orchy_application::create_document::CreateDocumentCommand;
use orchy_application::create_task::CreateTaskCommand;
use orchy_application::edit_document::{EditDocumentCommand, EditMode};
use orchy_application::send_message::SendMessageCommand;
use orchy_application::split_task::SplitTaskCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_core::{ActorStore, Clock, EventLog, IdGenerator, ReadWatermarks, Search};
use orchy_store_vault::blob::{BlobStore, FsBlobStore};
use orchy_store_vault::documents::VaultDocumentStore;
use orchy_store_vault::edges::VaultEdgeStore;
use orchy_store_vault::eventlog::EventuaryLog;
use orchy_store_vault::messages::VaultMessageStore;
use orchy_store_vault::roster::{FileLeaseStore, VaultActorStore};
use orchy_store_vault::search::VaultSearch;
use orchy_store_vault::tasks::VaultTaskStore;
use orchy_store_vault::time::{SystemClock, UlidGenerator};
use orchy_store_vault::vault::Vault;
use orchy_store_vault::watermarks::FileWatermarks;

const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const ACTOR: &str = "claude@01ARZ3NDEKTSV4RRFFQ69G5FAV";

struct Fixture {
    app: Application,
    root: tempfile::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let blobs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::new(root.path()));
        let vault = Arc::new(Vault::open(Arc::clone(&blobs)).await.unwrap());

        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let ids: Arc<dyn IdGenerator> = Arc::new(UlidGenerator::new());
        let log: Arc<dyn EventLog> = Arc::new(
            EventuaryLog::open(
                root.path().join("events"),
                "orchy",
                ACTOR.parse().unwrap(),
                orchy_core::MachineId::new(MACHINE).unwrap(),
                orchy_store_vault::eventlog::DEFAULT_PARTITIONS,
            )
            .unwrap(),
        );

        let actors: Arc<dyn ActorStore> = Arc::new(VaultActorStore::new(Arc::clone(&vault)));
        let documents = Arc::new(VaultDocumentStore::new(
            Arc::clone(&vault),
            Arc::clone(&log),
        ));

        let deps = ApplicationDeps {
            search: Arc::new(VaultSearch::new(Arc::clone(&documents))) as Arc<dyn Search>,
            documents: Arc::clone(&documents) as _,
            tasks: Arc::new(VaultTaskStore::new(Arc::clone(&vault), Arc::clone(&log))),
            messages: Arc::new(VaultMessageStore::new(
                Arc::clone(&vault),
                Arc::clone(&actors),
                Arc::clone(&log),
            )),
            edges: Arc::new(VaultEdgeStore::new(Arc::clone(&vault))),
            actors,
            leases: Arc::new(FileLeaseStore::new(
                root.path().join(".orchy/locks"),
                Arc::clone(&clock),
            )),
            watermarks: Arc::new(FileWatermarks::new(root.path().join(".orchy/read")))
                as Arc<dyn ReadWatermarks>,
            log,
            clock,
            ids,
        };

        Self {
            app: Application::new(deps),
            root,
        }
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root.path().join(relative))
            .unwrap_or_else(|e| panic!("reading {relative}: {e}"))
    }

    fn exists(&self, relative: &str) -> bool {
        self.root.path().join(relative).exists()
    }

    async fn announce(&self) {
        self.app
            .announce_actor
            .execute(orchy_application::announce_actor::AnnounceActorCommand {
                actor: ACTOR.to_owned(),
                roles: vec!["developer".to_owned()],
                ..Default::default()
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn a_created_task_is_a_markdown_file_with_readable_frontmatter() {
    let fixture = Fixture::new().await;
    let task = fixture
        .app
        .create_task
        .execute(CreateTaskCommand {
            title: "Rotate signing keys".to_owned(),
            description: Some("HS256 must go".to_owned()),
            namespace: Some("/backend".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();

    let path = format!("tasks/open/{}.md", task.id);
    assert!(fixture.exists(&path), "task must land in tasks/open");

    let text = fixture.read(&path);
    assert!(
        text.starts_with("---\n"),
        "frontmatter fence first:\n{text}"
    );
    assert!(text.contains(&format!("id: {}", task.id)));
    assert!(text.contains("type: task"));
    assert!(text.contains("status: pending"));
    assert!(text.contains("namespace: /backend"));
    assert!(text.contains("HS256 must go"), "description is the body");
}

#[tokio::test]
async fn completing_a_task_moves_the_file_and_rewrites_its_status() {
    let fixture = Fixture::new().await;
    let task = fixture
        .app
        .create_task
        .execute(CreateTaskCommand {
            title: "ship it".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    fixture
        .app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    fixture
        .app
        .complete_task
        .execute(CompleteTaskCommand {
            task_id: task.id.clone(),
            note: Some("done".to_owned()),
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    assert!(
        !fixture.exists(&format!("tasks/open/{}.md", task.id)),
        "the open copy must not linger"
    );
    let text = fixture.read(&format!("tasks/done/{}.md", task.id));
    assert!(text.contains("status: completed"));
    assert!(text.contains(&format!("claimed_by: {ACTOR}")));
}

#[tokio::test]
async fn the_parent_file_is_rewritten_when_its_last_subtask_finishes() {
    let fixture = Fixture::new().await;
    let parent = fixture
        .app
        .create_task
        .execute(CreateTaskCommand {
            title: "epic".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let children = fixture
        .app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.id.clone(),
            titles: vec!["a".to_owned(), "b".to_owned()],
        })
        .await
        .unwrap()
        .created;

    let parent_open = format!("tasks/open/{}.md", parent.id);
    assert!(fixture.exists(&parent_open));
    assert!(
        fixture
            .read(&format!("tasks/open/{}.md", children[0].id))
            .contains(&format!("parent: task:{}", parent.id)),
        "the hierarchy is stored on the child, and names the kind it points at"
    );
    assert!(
        !fixture.read(&parent_open).contains("subtasks"),
        "the parent file is not rewritten when a child is added"
    );

    for child in &children {
        fixture
            .app
            .claim_task
            .execute(ClaimTaskCommand {
                task_id: child.id.clone(),
                actor: ACTOR.to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        fixture
            .app
            .complete_task
            .execute(CompleteTaskCommand {
                task_id: child.id.clone(),
                note: None,
                actor: ACTOR.to_owned(),
            })
            .await
            .unwrap();
    }

    assert!(!fixture.exists(&parent_open), "the parent moved to done");
    assert!(
        fixture
            .read(&format!("tasks/done/{}.md", parent.id))
            .contains("status: completed"),
        "rollup reached the file on disk"
    );
}

#[tokio::test]
async fn a_documents_own_frontmatter_survives_an_edit_by_orchy() {
    let fixture = Fixture::new().await;
    let document = fixture
        .app
        .create_document
        .execute(CreateDocumentCommand {
            kind: "decision".to_owned(),
            title: "Key rotation".to_owned(),
            namespace: Some("/backend".to_owned()),
            body: Some("# Context\n\nWe use HS256.".to_owned()),
            tags: vec!["auth".to_owned()],
        })
        .await
        .unwrap();

    let path = format!("docs/backend/{}.md", document.id);
    let original = fixture.read(&path);
    assert!(original.contains("type: decision"));
    assert!(original.contains("- auth"));

    std::fs::write(
        fixture.root.path().join(&path),
        original.replace("---\nid:", "---\nreviewer: alan\nid:"),
    )
    .unwrap();

    fixture
        .app
        .edit_document
        .execute(EditDocumentCommand {
            document_id: document.id.clone(),
            content: "Move to RS256.".to_owned(),
            mode: EditMode::Append,
            if_match: None,
        })
        .await
        .unwrap();

    let after = fixture.read(&path);
    assert!(
        after.contains("reviewer: alan"),
        "a field orchy does not own must survive its write:\n{after}"
    );
    assert!(after.contains("Move to RS256."));
    assert!(after.contains("We use HS256."));
}

#[tokio::test]
async fn an_edit_is_refused_when_the_document_changed_since_it_was_read() {
    let fixture = Fixture::new().await;
    let document = fixture
        .app
        .create_document
        .execute(CreateDocumentCommand {
            kind: "note".to_owned(),
            title: "n".to_owned(),
            body: Some("one".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();

    let stale = document.content_hash.clone();
    fixture
        .app
        .edit_document
        .execute(EditDocumentCommand {
            document_id: document.id.clone(),
            content: "two".to_owned(),
            mode: EditMode::Append,
            if_match: None,
        })
        .await
        .unwrap();

    let refused = fixture
        .app
        .edit_document
        .execute(EditDocumentCommand {
            document_id: document.id,
            content: "three".to_owned(),
            mode: EditMode::Append,
            if_match: Some(stale),
        })
        .await;
    assert!(refused.is_err(), "a stale if-match must be refused");
}

#[tokio::test]
async fn a_message_is_one_file_under_its_thread_and_reaches_an_inbox() {
    let fixture = Fixture::new().await;
    fixture.announce().await;

    let message = fixture
        .app
        .send_message
        .execute(SendMessageCommand {
            from: "codex@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            to: vec!["broadcast".to_owned()],
            subject: Some("build is red".to_owned()),
            body: "master is failing".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let path = format!("messages/{}/{}.md", message.thread, message.id);
    assert!(fixture.exists(&path), "a message lives under its thread");
    let text = fixture.read(&path);
    assert!(text.contains("type: message"));
    assert!(text.contains("status: open"));
    assert!(text.contains("master is failing"));

    let inbox = fixture
        .app
        .read_inbox
        .execute(orchy_application::read_inbox::ReadInboxCommand {
            actor: ACTOR.to_owned(),
            all: false,
        })
        .await
        .unwrap();
    assert_eq!(inbox.len(), 1, "the broadcast reaches the announced actor");
}

#[tokio::test]
async fn two_agents_cannot_hold_the_same_task_and_the_lock_survives_the_process() {
    let fixture = Fixture::new().await;
    let task = fixture
        .app
        .create_task
        .execute(CreateTaskCommand {
            title: "contended".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    fixture
        .app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let second = fixture
        .app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id: task.id.clone(),
            actor: "codex@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            ..Default::default()
        })
        .await;
    assert!(second.is_err());

    let locks = std::fs::read_dir(fixture.root.path().join(".orchy/locks")).unwrap();
    assert!(locks.count() > 0, "the lease is a file, not process state");
}

#[tokio::test]
async fn everything_written_is_reread_correctly_by_a_fresh_process() {
    let fixture = Fixture::new().await;
    let task = fixture
        .app
        .create_task
        .execute(CreateTaskCommand {
            title: "survives a restart".to_owned(),
            description: Some("body text".to_owned()),
            priority: Some("high".to_owned()),
            tags: vec!["rust".to_owned()],
            namespace: Some("/backend".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();

    let blobs: Arc<dyn BlobStore> = Arc::new(FsBlobStore::new(fixture.root.path()));
    let reopened = Vault::open(blobs).await.unwrap();
    let store = VaultTaskStore::new(
        Arc::new(reopened),
        Arc::new(orchy_store_memory::MemoryEventLog::new()),
    );

    use orchy_core::TaskStore;
    let found = store
        .get(&orchy_core::Id::new(&task.id).unwrap())
        .await
        .unwrap()
        .expect("task must be found after a reopen");

    assert_eq!(found.title().as_str(), "survives a restart");
    assert_eq!(found.description(), "body text");
    assert_eq!(found.priority().as_str(), "high");
    assert_eq!(found.namespace().as_str(), "/backend");
    assert_eq!(found.tags().len(), 1);
}

#[tokio::test]
async fn recall_finds_a_document_by_a_word_only_in_its_title() {
    let fixture = Fixture::new().await;
    fixture
        .app
        .create_document
        .execute(CreateDocumentCommand {
            kind: "candidate".to_owned(),
            title: "Drop JWT for opaque tokens".to_owned(),
            body: Some("Would remove signing at the cost of a lookup per request.".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();

    let hits = fixture
        .app
        .recall
        .execute(orchy_application::recall::RecallCommand {
            text: "opaque".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(
        hits.len(),
        1,
        "a document whose subject lives in its name must still be findable"
    );
    assert_eq!(
        hits[0].heading.as_deref(),
        Some("Drop JWT for opaque tokens")
    );
}

#[tokio::test]
async fn a_body_match_and_a_title_match_are_separate_hits() {
    let fixture = Fixture::new().await;
    fixture
        .app
        .create_document
        .execute(CreateDocumentCommand {
            kind: "note".to_owned(),
            title: "Rotation policy".to_owned(),
            body: Some("# Detail\n\nRotation happens quarterly.".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();

    let hits = fixture
        .app
        .recall
        .execute(orchy_application::recall::RecallCommand {
            text: "rotation".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(hits.len(), 2, "one for the title, one for the section");
}
