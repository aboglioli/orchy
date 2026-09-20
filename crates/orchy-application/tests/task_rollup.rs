use std::sync::Arc;

use orchy_application::cancel_task::CancelTaskCommand;
use orchy_application::claim_task::ClaimTaskCommand;
use orchy_application::complete_task::CompleteTaskCommand;
use orchy_application::create_task::CreateTaskCommand;
use orchy_application::fail_task::FailTaskCommand;
use orchy_application::split_task::SplitTaskCommand;
use orchy_application::{Application, ApplicationDeps};
use orchy_store_memory::MemoryBackend;

const ACTOR: &str = "claude@01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn app() -> (Application, MemoryBackend) {
    let backend = MemoryBackend::new();
    let deps = ApplicationDeps {
        documents: Arc::clone(&backend.documents) as _,
        tasks: Arc::clone(&backend.tasks) as _,
        messages: Arc::clone(&backend.messages) as _,
        edges: Arc::clone(&backend.edges) as _,
        actors: Arc::clone(&backend.actors) as _,
        leases: Arc::clone(&backend.leases) as _,
        watermarks: Arc::clone(&backend.watermarks) as _,
        search: Arc::clone(&backend.search) as _,
        log: Arc::clone(&backend.log) as _,
        clock: Arc::clone(&backend.clock) as _,
        ids: Arc::clone(&backend.ids) as _,
    };
    (Application::new(deps), backend)
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

#[tokio::test]
async fn completing_the_last_subtask_completes_the_parent() {
    let (app, _) = app();
    let parent = create(&app, "ship auth").await;
    let split = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.clone(),
            titles: vec!["design".to_owned(), "implement".to_owned()],
        })
        .await
        .unwrap();
    assert_eq!(split.created.len(), 2);

    finish(&app, &split.created[0].id).await;
    let after_first = app
        .get_task
        .execute(orchy_application::get_task::GetTaskCommand {
            task_id: parent.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        after_first.task.status, "pending",
        "the parent must wait for every child"
    );

    let response = app
        .complete_task
        .execute(CompleteTaskCommand {
            task_id: split.created[1].id.clone(),
            note: None,
            actor: ACTOR.to_owned(),
        })
        .await;
    // the second child must be claimed first
    assert!(response.is_err());

    finish(&app, &split.created[1].id).await;
    let after_all = app
        .get_task
        .execute(orchy_application::get_task::GetTaskCommand { task_id: parent })
        .await
        .unwrap();
    assert_eq!(after_all.task.status, "completed");
}

#[tokio::test]
async fn rollup_walks_all_the_way_to_the_grandparent() {
    let (app, _) = app();
    let grandparent = create(&app, "epic").await;
    let parent = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: grandparent.clone(),
            titles: vec!["feature".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();
    let child = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.clone(),
            titles: vec!["unit".to_owned()],
        })
        .await
        .unwrap()
        .created[0]
        .id
        .clone();

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: child.clone(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let response = app
        .complete_task
        .execute(CompleteTaskCommand {
            task_id: child,
            note: None,
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    let touched: Vec<&str> = response.ancestors.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        touched,
        vec![parent.as_str(), grandparent.as_str()],
        "one completion must cascade two levels, nearest first"
    );
    assert!(response.ancestors.iter().all(|t| t.status == "completed"));
}

#[tokio::test]
async fn a_failed_child_fails_the_parent() {
    let (app, _) = app();
    let parent = create(&app, "ship auth").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.clone(),
            titles: vec!["a".to_owned(), "b".to_owned()],
        })
        .await
        .unwrap()
        .created;

    finish(&app, &children[0].id).await;
    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: children[1].id.clone(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let response = app
        .fail_task
        .execute(FailTaskCommand {
            task_id: children[1].id.clone(),
            reason: "broken".to_owned(),
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(response.ancestors.len(), 1);
    assert_eq!(
        response.ancestors[0].status, "failed",
        "one failure fails the goal"
    );
}

#[tokio::test]
async fn a_parent_whose_children_are_all_cancelled_is_cancelled() {
    let (app, _) = app();
    let parent = create(&app, "abandoned").await;
    let children = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent.clone(),
            titles: vec!["a".to_owned(), "b".to_owned()],
        })
        .await
        .unwrap()
        .created;

    for child in &children {
        app.claim_task
            .execute(ClaimTaskCommand {
                task_id: child.id.clone(),
                actor: ACTOR.to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    app.cancel_task
        .execute(CancelTaskCommand {
            task_id: children[0].id.clone(),
            reason: "not needed".to_owned(),
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();
    let response = app
        .cancel_task
        .execute(CancelTaskCommand {
            task_id: children[1].id.clone(),
            reason: "not needed".to_owned(),
            actor: ACTOR.to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(response.ancestors[0].status, "cancelled");
}

#[tokio::test]
async fn splitting_twice_adds_only_the_missing_children() {
    let (app, _) = app();
    let parent = create(&app, "goal").await;
    let cmd = SplitTaskCommand {
        task_id: parent.clone(),
        titles: vec!["a".to_owned(), "b".to_owned()],
    };
    app.split_task.execute(cmd.clone()).await.unwrap();

    let again = app
        .split_task
        .execute(SplitTaskCommand {
            task_id: parent,
            titles: vec!["a".to_owned(), "c".to_owned()],
        })
        .await
        .unwrap();

    assert_eq!(again.created.len(), 1, "only `c` is new");
    assert_eq!(again.created[0].title, "c");
    assert_eq!(again.skipped, vec!["a".to_owned()]);
}

#[tokio::test]
async fn a_second_agent_cannot_claim_a_held_task() {
    let (app, _) = app();
    let id = create(&app, "contended").await;

    app.claim_task
        .execute(ClaimTaskCommand {
            task_id: id.clone(),
            actor: ACTOR.to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let second = app
        .claim_task
        .execute(ClaimTaskCommand {
            task_id: id,
            actor: "codex@01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            ..Default::default()
        })
        .await;
    assert!(second.is_err(), "the lease must refuse the second claimant");
}
