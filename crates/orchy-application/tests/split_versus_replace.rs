use std::sync::Arc;

use orchy_application::claim_task::ClaimTaskCommand;
use orchy_application::complete_task::CompleteTaskCommand;
use orchy_application::create_task::CreateTaskCommand;
use orchy_application::get_task::GetTaskCommand;
use orchy_application::replace_task::ReplaceTaskCommand;
use orchy_application::split_task::SplitTaskCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_store_memory::MemoryBackend;

const ACTOR: &str = "claude@01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn app() -> Application {
    let backend = MemoryBackend::new();
    Application::new(ApplicationDeps {
        documents: Arc::clone(&backend.documents) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        messages: Arc::clone(&backend.messages) as _,
        edges: Arc::clone(&backend.edges) as _,
        actors: Arc::clone(&backend.actors) as _,
        leases: Arc::clone(&backend.leases) as _,
        watermarks: Arc::clone(&backend.watermarks) as _,
        search: Arc::clone(&backend.search) as _,
        log: Arc::clone(&backend.log) as _,
        types: Arc::clone(&backend.types) as _,
        relations: Arc::clone(&backend.relations) as _,
        clock: Arc::clone(&backend.clock) as _,
        ids: Arc::clone(&backend.ids) as _,
    })
}

async fn create(app: &Application, title: &str) -> String {
    app.create_task
        .execute(CreateTaskCommand {
            title: title.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
}

async fn finish(app: &Application, id: &str) {
    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: id.to_owned(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    app.complete_task
        .execute(CompleteTaskCommand {
            task_id: id.to_owned(),
            note: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();
}

async fn status(app: &Application, id: &str) -> String {
    app.get_task
        .execute(GetTaskCommand {
            task_id: id.to_owned(),
        })
        .await
        .unwrap()
        .task
        .status
}

#[tokio::test]
async fn split_keeps_the_goal_open_as_an_umbrella() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["design".to_owned(), "build".to_owned()],
        })
        .await
        .unwrap()
        .created;

    assert_eq!(status(&app, &goal).await, "pending", "the goal waits");
    for child in &children {
        assert_eq!(
            child.parent.as_deref(),
            Some(goal.as_str()),
            "a subtask points at its umbrella"
        );
    }

    finish(&app, &children[0].id).await;
    assert_eq!(status(&app, &goal).await, "pending", "one is not enough");
    finish(&app, &children[1].id).await;
    assert_eq!(status(&app, &goal).await, "completed");
}

#[tokio::test]
async fn replace_retires_the_original_and_leaves_the_new_tasks_free() {
    let app = app();
    let original = create(&app, "rewrite auth").await;
    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: original.clone(),
            titles: vec!["rotate keys".to_owned(), "drop hs256".to_owned()],
            reason: Some("too coarse".to_owned()),
        })
        .await
        .unwrap();

    assert_eq!(response.replaced.status, "superseded");
    assert_eq!(status(&app, &original).await, "superseded");

    for created in &response.created {
        assert_eq!(
            created.parent, None,
            "a replacement is not a subtask of what it replaced"
        );
        assert_eq!(created.status, "pending");
    }
}

#[tokio::test]
async fn a_replacement_records_what_it_superseded() {
    let app = app();
    let original = create(&app, "rewrite auth").await;
    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: original.clone(),
            titles: vec!["rotate keys".to_owned()],
            reason: None,
        })
        .await
        .unwrap();

    let links = app
        .get_task
        .execute(GetTaskCommand {
            task_id: response.created[0].id.clone(),
        })
        .await
        .unwrap()
        .edges;

    assert!(
        links
            .iter()
            .any(|e| e.relation == "supersedes" && e.to.ends_with(&original)),
        "provenance must survive: {links:?}"
    );
}

#[tokio::test]
async fn replacing_a_subtask_keeps_its_work_under_the_same_goal() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let subtask = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["do everything".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();

    let response = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: subtask,
            titles: vec!["step one".to_owned(), "step two".to_owned()],
            reason: None,
        })
        .await
        .unwrap();

    for created in &response.created {
        assert_eq!(
            created.parent.as_deref(),
            Some(goal.as_str()),
            "the branch must not detach from the goal above it"
        );
    }
    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the goal now waits for the replacements instead"
    );
}

#[tokio::test]
async fn a_superseded_subtask_does_not_spoil_its_parents_completion() {
    let app = app();
    let goal = create(&app, "ship auth").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["keep".to_owned(), "rework".to_owned()],
        })
        .await
        .unwrap()
        .created;

    let replacements = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: children[1].id.clone(),
            titles: vec!["reworked".to_owned()],
            reason: None,
        })
        .await
        .unwrap()
        .created;

    finish(&app, &children[0].id).await;
    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the replacement is still open"
    );

    finish(&app, &replacements[0].id).await;
    assert_eq!(
        status(&app, &goal).await,
        "completed",
        "superseded carries no verdict; the real completions decide"
    );
}

#[tokio::test]
async fn replacing_every_subtask_keeps_the_goal_open_for_the_replacements() {
    let app = app();
    let goal = create(&app, "abandoned approach").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: goal.clone(),
            titles: vec!["a".to_owned(), "b".to_owned()],
        })
        .await
        .unwrap()
        .created;

    for child in &children {
        app.replace_task
            .execute(ReplaceTaskCommand {
                task_id: child.id.clone(),
                titles: vec![format!("{} redone", child.title)],
                reason: None,
            })
            .await
            .unwrap();
    }

    assert_eq!(
        status(&app, &goal).await,
        "pending",
        "the replacements inherited the goal, so it still waits"
    );
}

#[tokio::test]
async fn replacing_a_finished_task_is_refused() {
    let app = app();
    let done = create(&app, "already done").await;
    finish(&app, &done).await;

    let refused = app
        .replace_task
        .execute(ReplaceTaskCommand {
            task_id: done,
            titles: vec!["too late".to_owned()],
            reason: None,
        })
        .await;
    assert!(refused.is_err(), "work already done was not replaced");
}
