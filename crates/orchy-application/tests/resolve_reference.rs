use std::sync::Arc;

use orchy_application::create_task::CreateTaskCommand;
use orchy_application::resolve_reference::{ReferenceKind, ResolveReferenceCommand};
use orchy_application::{Application, ApplicationDeps, ApplicationError};
use orchy_core::DomainError;
use orchy_store_memory::MemoryBackend;

fn app() -> Application {
    let backend = MemoryBackend::new();
    Application::new(ApplicationDeps {
        documents: Arc::clone(&backend.documents) as _,
        skills: Arc::clone(&backend.skills) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        messages: Arc::clone(&backend.messages) as _,
        edges: Arc::clone(&backend.edges) as _,
        actors: Arc::clone(&backend.actors) as _,
        sessions: Arc::clone(&backend.sessions) as _,
        leases: Arc::clone(&backend.leases) as _,
        watermarks: Arc::clone(&backend.watermarks) as _,
        search: Arc::clone(&backend.search) as _,
        integrity: Arc::clone(&backend.integrity) as _,
        log: Arc::clone(&backend.log) as _,
        clock: Arc::clone(&backend.clock) as _,
        ids: Arc::clone(&backend.ids) as _,
        unit_of_work: Arc::clone(&backend.unit_of_work) as _,
    })
}

async fn create(app: &Application, title: &str) {
    app.create_task
        .execute(CreateTaskCommand {
            title: title.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_fragment_shared_by_the_first_and_the_last_of_many_tasks_is_ambiguous() {
    let app = app();
    create(&app, "the needle, first").await;
    for n in 0..1000 {
        create(&app, &format!("hay {n}")).await;
    }
    create(&app, "the needle, last").await;

    let result = app
        .resolve_reference
        .execute(ResolveReferenceCommand {
            kind: ReferenceKind::Task,
            input: "needle".to_owned(),
        })
        .await;
    assert!(
        matches!(
            result,
            Err(ApplicationError::Domain(DomainError::Ambiguous {
                count: 2,
                ..
            }))
        ),
        "{result:?}"
    );
}
