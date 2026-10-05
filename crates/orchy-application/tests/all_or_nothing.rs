use std::sync::Arc;

use orchy_application::consolidate_documents::ConsolidateDocumentsCommand;
use orchy_application::create_document::CreateDocumentCommand;
use orchy_application::merge_tasks::MergeTasksCommand;
use orchy_application::read_document::ReadDocumentCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_core::{EventLog, EventQuery};
use orchy_store_memory::MemoryBackend;

fn app() -> (Application, MemoryBackend) {
    let backend = MemoryBackend::new();
    let app = Application::new(ApplicationDeps {
        documents: Arc::clone(&backend.documents) as _,
        skills: Arc::clone(&backend.skills) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        messages: Arc::clone(&backend.messages) as _,
        edges: Arc::clone(&backend.edges) as _,
        actors: Arc::clone(&backend.actors) as _,
        leases: Arc::clone(&backend.leases) as _,
        watermarks: Arc::clone(&backend.watermarks) as _,
        search: Arc::clone(&backend.search) as _,
        integrity: Arc::clone(&backend.integrity) as _,
        log: Arc::clone(&backend.log) as _,
        clock: Arc::clone(&backend.clock) as _,
        ids: Arc::clone(&backend.ids) as _,
        unit_of_work: Arc::clone(&backend.unit_of_work) as _,
    });
    (app, backend)
}

async fn document(app: &Application, kind: &str, title: &str) -> String {
    app.create_document
        .execute(CreateDocumentCommand {
            kind: kind.to_owned(),
            title: title.to_owned(),
            body: Some("text".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
        .document
        .id
}

async fn status(app: &Application, id: &str) -> Option<String> {
    app.read_document
        .execute(ReadDocumentCommand {
            document_id: id.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap()
        .document
        .status
}

#[tokio::test]
async fn a_consolidation_refused_halfway_supersedes_nothing() {
    let (app, backend) = app();
    let first = document(&app, "note", "first").await;
    let proposal = document(&app, "candidate", "proposal").await;
    let into = document(&app, "note", "into").await;
    let events = backend
        .log
        .replay(&EventQuery::default())
        .await
        .unwrap()
        .len();

    let refused = app
        .consolidate_documents
        .execute(ConsolidateDocumentsCommand {
            sources: vec![first.clone(), proposal],
            into,
        })
        .await;
    assert!(refused.is_err(), "a candidate is not superseded");
    assert_eq!(
        status(&app, &first).await.as_deref(),
        Some("active"),
        "the source handled before the refusal is untouched"
    );
    assert_eq!(
        backend
            .log
            .replay(&EventQuery::default())
            .await
            .unwrap()
            .len(),
        events,
        "and nothing was recorded"
    );
}

#[tokio::test]
async fn a_merge_refused_halfway_moves_and_retires_nothing() {
    let (app, _) = app();
    let task = |title: &str| {
        let title = title.to_owned();
        let app = &app;
        async move {
            app.create_task
                .execute(orchy_application::create_task::CreateTaskCommand {
                    title,
                    ..Default::default()
                })
                .await
                .unwrap()
                .id
        }
    };
    let keep = task("keep").await;
    let first = task("first").await;
    let finished = task("finished").await;
    app.cancel_task
        .execute(orchy_application::cancel_task::CancelTaskCommand {
            task_id: finished.clone(),
            actor: "lead@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            reason: "dropped".to_owned(),
        })
        .await
        .unwrap();

    let refused = app
        .merge_tasks
        .execute(MergeTasksCommand {
            keep,
            others: vec![first.clone(), finished],
            actor: "lead@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
        })
        .await;
    assert!(refused.is_err(), "a cancelled task cannot be superseded");
    let first = app
        .get_task
        .execute(orchy_application::get_task::GetTaskCommand { task_id: first })
        .await
        .unwrap();
    assert_eq!(
        first.task.status, "pending",
        "the first duplicate stays open"
    );
}
