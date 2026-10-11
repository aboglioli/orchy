use std::sync::Arc;

use orchy_application::announce_actor::AnnounceActorCommand;
use orchy_application::leave_session::LeaveSessionCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_store_memory::MemoryBackend;

const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

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

fn actor(alias: &str) -> String {
    format!("{alias}@{MACHINE}")
}

async fn announce(app: &Application, alias: &str, session: Option<String>) -> String {
    app.announce_actor
        .execute(AnnounceActorCommand {
            actor: actor(alias),
            session,
            ..Default::default()
        })
        .await
        .unwrap()
        .session
        .unwrap()
        .token
}

#[tokio::test]
async fn announcing_with_your_own_session_resumes_it() {
    let app = app();
    let first = announce(&app, "coder-1", None).await;
    let again = announce(&app, "coder-1", Some(first.clone())).await;
    assert_eq!(first, again);
}

#[tokio::test]
async fn another_agents_or_an_ended_session_starts_a_new_one() {
    let app = app();
    let theirs = announce(&app, "coder-1", None).await;
    let mine = announce(&app, "coder-2", Some(theirs.clone())).await;
    assert_ne!(mine, theirs, "a session never moves to another agent");

    app.leave_session
        .execute(LeaveSessionCommand {
            actor: actor("coder-2"),
            session: mine.clone(),
        })
        .await
        .unwrap();
    let after = announce(&app, "coder-2", Some(mine.clone())).await;
    assert_ne!(after, mine, "an ended session is not resumed");
}

#[tokio::test]
async fn only_the_agent_a_session_belongs_to_can_end_it() {
    let app = app();
    let token = announce(&app, "coder-1", None).await;
    let refused = app
        .leave_session
        .execute(LeaveSessionCommand {
            actor: actor("coder-2"),
            session: token,
        })
        .await;
    assert!(refused.is_err());
}
